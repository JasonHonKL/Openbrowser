use std::{cell::OnceCell, sync::Arc, time::Instant};

use pardus_debug::{Initiator, NetworkRecord, ResourceType};
use scraper::{ElementRef, Html, Selector};
use serde::Serialize;
use url::Url;

use crate::{
    app::App,
    frame::{FrameId, FrameTree},
    interact::element::{ElementHandle, element_to_handle},
    navigation::graph::NavigationGraph,
    push::EarlyScanner,
    resource::{
        ResourceConfig, ResourceFetcher, ResourceKind, ResourceScheduler, scheduler::ResourceTask,
    },
    semantic::tree::{SemanticNode, SemanticRole, SemanticTree},
};

// ---------------------------------------------------------------------------
// Redirect chain types
// ---------------------------------------------------------------------------

/// One hop in an HTTP redirect chain.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RedirectHop {
    /// The URL that issued the redirect.
    pub from: String,
    /// The target URL from the Location header.
    pub to: String,
    /// The HTTP status code (301, 302, 303, 307, 308).
    pub status: u16,
}

/// The full redirect chain captured during an HTTP request.
///
/// Ordered from first redirect to last. Empty when no redirects occurred.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RedirectChain {
    pub hops: Vec<RedirectHop>,
}

impl RedirectChain {
    pub fn is_empty(&self) -> bool { self.hops.is_empty() }

    /// The original URL before any redirects.
    pub fn original_url(&self) -> Option<&str> { self.hops.first().map(|h| h.from.as_str()) }
}

/// Serializable snapshot of a page's state.
///
/// Used to transfer page data over the wire (e.g., via CDP WebSocket)
/// without exposing the non-serializable `scraper::Html` type.
#[derive(Debug, Clone, Serialize)]
pub struct PageSnapshot {
    pub url: String,
    pub status: u16,
    pub content_type: Option<String>,
    pub title: Option<String>,
    pub html: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_chain: Option<RedirectChain>,
}

pub struct Page {
    pub url: String,
    pub status: u16,
    pub content_type: Option<String>,
    pub html: Html,
    pub base_url: String,
    pub csp: Option<crate::csp::CspPolicySet>,
    pub frame_tree: Option<FrameTree>,
    pub cached_tree: OnceCell<Arc<SemanticTree>>,
    pub redirect_chain: Option<RedirectChain>,
}

struct FetchedResponse {
    final_url: String,
    status: u16,
    content_type: Option<String>,
    resp_headers: Vec<(String, String)>,
    http_version: String,
    body_bytes: Vec<u8>,
    redirect_hops: Vec<RedirectHop>,
    started_at: String,
    elapsed_ms: u128,
}

impl Page {
    #[must_use = "ignoring Result may silently swallow navigation errors"]
    pub async fn from_url(app: &Arc<App>, url: &str) -> anyhow::Result<Self> {
        Self::fetch_and_create(app, url, 0).await
    }

    /// Fetch a URL with streaming semantic parsing.
    ///
    /// Like `from_url()` but uses `StreamingHtmlParser` to discover elements
    /// as HTTP chunks arrive. The optional `event_sink` receives nodes in
    /// real-time. Returns `(Page, StreamingParseStats)`.
    #[must_use = "ignoring Result may silently swallow navigation errors"]
    pub async fn from_url_streaming(
        app: &Arc<App>,
        url: &str,
        event_sink: Option<std::sync::Arc<dyn crate::parser::StreamingEventSink + Send + Sync>>,
    ) -> anyhow::Result<(Self, crate::parser::StreamingParseStats)> {
        let mut current_url = url.to_string();
        let mut depth = 0;

        loop {
            if depth >= Self::MAX_REDIRECT_DEPTH {
                anyhow::bail!(
                    "Redirect depth exceeded ({} >= {}) for {}",
                    depth,
                    Self::MAX_REDIRECT_DEPTH,
                    current_url
                );
            }

            let (page, stats) =
                Self::fetch_and_create_single_streaming(app, &current_url, event_sink.clone())
                    .await?;

            if let Some(refresh_url) = page.meta_refresh_url() {
                tracing::debug!(target: "page", "meta refresh redirect: {} -> {}", current_url, refresh_url);
                current_url = refresh_url;
                depth += 1;
                continue;
            }

            return Ok((page, stats));
        }
    }

    /// Fetch a URL and create a Page, routing to PDF extraction when appropriate.
    ///
    /// The HTTP pipeline runs in this order:
    /// 1. Request deduplication (skip if same URL already in-flight)
    /// 2. Request interception (before-request: block / redirect / modify / mock)
    /// 3. HTTP fetch with retry (exponential backoff for transient failures)
    /// 4. Response interception (after-response: modify / block)
    /// 5. Meta refresh redirect detection (up to MAX_REDIRECT_DEPTH)
    pub(crate) async fn fetch_and_create(
        app: &Arc<App>,
        url: &str,
        mut depth: usize,
    ) -> anyhow::Result<Self> {
        let mut current_url = url.to_string();

        loop {
            if depth >= Self::MAX_REDIRECT_DEPTH {
                anyhow::bail!(
                    "Redirect depth exceeded ({} >= {}) for {}",
                    depth,
                    Self::MAX_REDIRECT_DEPTH,
                    current_url
                );
            }

            let page = Self::fetch_and_create_single(app, &current_url).await?;

            if let Some(refresh_url) = page.meta_refresh_url() {
                tracing::debug!(target: "page", "meta refresh redirect: {} -> {}", current_url, refresh_url);
                current_url = refresh_url;
                depth += 1;
                continue;
            }

            return Ok(page);
        }
    }

    async fn run_fetch_pipeline(
        app: &Arc<App>,
        url: &str,
    ) -> anyhow::Result<(FetchedResponse, String)> {
        let url_key = crate::dedup::dedup_key(url);

        if app.dedup.is_enabled() {
            match app.dedup.enter(&url_key).await {
                crate::dedup::DedupEntry::Cached(result) => {
                    let resp = FetchedResponse::from_dedup_result(result);
                    return Ok((resp, url_key));
                }
                crate::dedup::DedupEntry::Wait(notify) => {
                    notify.notified().await;
                    if let Some(result) = app.dedup.get_completed(&url_key) {
                        let resp = FetchedResponse::from_dedup_result(result);
                        return Ok((resp, url_key));
                    }
                }
                crate::dedup::DedupEntry::Proceed => {}
            }
        }

        let mut req_ctx = crate::intercept::RequestContext {
            url: url.to_string(),
            method: "GET".to_string(),
            headers: std::collections::HashMap::new(),
            body: None,
            resource_type: ResourceType::Document,
            initiator: Initiator::Navigation,
            is_navigation: true,
        };

        let action = app.interceptors.run_before_request(&mut req_ctx).await;

        let effective_url = match action {
            crate::intercept::InterceptAction::Block => {
                anyhow::bail!("Request to '{}' blocked by interceptor", url);
            }
            crate::intercept::InterceptAction::Redirect(target) => {
                app.validate_url(&target)?;
                target
            }
            crate::intercept::InterceptAction::Mock(mock) => {
                tracing::debug!("interceptor mocked response for {}", url);
                let resp = FetchedResponse::from_mock(&req_ctx.url, &mock);
                return Ok((resp, url_key));
            }
            crate::intercept::InterceptAction::Modify(_)
            | crate::intercept::InterceptAction::Continue => req_ctx.url.clone(),
        };

        if effective_url != url {
            app.validate_url(&effective_url)?;
        }

        let started_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let start = Instant::now();

        let retry_config = app.config.read().retry.clone();
        let (response, redirect_hops) =
            Self::fetch_with_retry(app, &effective_url, &req_ctx.headers, &retry_config).await?;

        let http_version = format_http_version(response.version());
        let status = response.status().as_u16();
        let final_url = response.url().to_string();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let resp_headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .filter_map(|(k, v)| Some((k.to_string(), v.to_str().ok()?.to_string())))
            .collect();

        let resp_ctx = crate::intercept::ResponseContext {
            url: final_url.clone(),
            status,
            headers: resp_headers.iter().cloned().collect(),
            body: None,
            resource_type: ResourceType::Document,
        };
        let post_action = app
            .interceptors
            .run_after_response(&mut {
                let mut ctx = resp_ctx;
                ctx
            })
            .await;

        if let crate::intercept::InterceptAction::Block = post_action {
            app.dedup.remove(&url_key);
            anyhow::bail!("Response from '{}' blocked by interceptor", final_url);
        }

        let body_bytes = response
            .bytes()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read response body: {}", e))?;

        let elapsed_ms = start.elapsed().as_millis();
        let body_size = body_bytes.len();

        record_main_request(
            app,
            &effective_url,
            &final_url,
            status,
            &content_type,
            body_size,
            elapsed_ms,
            &resp_headers,
            started_at.clone(),
            &http_version,
        );

        let body_bytes_vec = body_bytes.to_vec();

        let dedup_result = crate::dedup::DedupResult {
            url: final_url.clone(),
            status,
            body: body_bytes_vec.clone(),
            content_type: content_type.clone(),
            headers: resp_headers.clone(),
            http_version: http_version.clone(),
        };
        app.dedup.complete(&url_key, dedup_result);

        Ok((
            FetchedResponse {
                final_url,
                status,
                content_type,
                resp_headers,
                http_version,
                body_bytes: body_bytes_vec,
                redirect_hops,
                started_at,
                elapsed_ms,
            },
            url_key,
        ))
    }

    async fn fetch_and_create_single(app: &Arc<App>, url: &str) -> anyhow::Result<Self> {
        let (fetched, url_key) = Self::run_fetch_pipeline(app, url).await?;
        fetched.into_page(app, &url_key, None).await
    }

    async fn fetch_and_create_single_streaming(
        app: &Arc<App>,
        url: &str,
        event_sink: Option<std::sync::Arc<dyn crate::parser::StreamingEventSink + Send + Sync>>,
    ) -> anyhow::Result<(Self, crate::parser::StreamingParseStats)> {
        let (fetched, url_key) = Self::run_fetch_pipeline(app, url).await?;
        let page = fetched.into_page(app, &url_key, event_sink).await?;
        let stats = crate::parser::StreamingParseStats::default();
        Ok((page, stats))
    }

    fn meta_refresh_url(&self) -> Option<String> {
        if let Ok(base_url) = Url::parse(&self.base_url) {
            Self::parse_meta_refresh(&self.html, &base_url)
        } else {
            None
        }
    }

    /// Create a Page from a mocked response (interceptor returned Mock action).
    fn from_mock_response(url: &str, mock: &crate::intercept::MockResponse) -> Self {
        let body_str = String::from_utf8_lossy(&mock.body).to_string();
        let html = Html::parse_document(&body_str);
        let content_type = mock.headers.get("content-type").cloned();
        Self {
            url: url.to_string(),
            status: mock.status,
            content_type,
            html,
            base_url: url.to_string(),
            csp: None,
            frame_tree: None,
            cached_tree: OnceCell::new(),
            redirect_chain: None,
        }
    }

    /// Create a Page from a deduplicated cached result.
    fn from_dedup_result(result: &crate::dedup::DedupResult) -> anyhow::Result<Self> {
        let body_str = String::from_utf8_lossy(&result.body).to_string();
        let html = Html::parse_document(&body_str);

        validate_content_type_pub(result.content_type.as_deref(), &result.url)?;

        Ok(Self {
            url: result.url.clone(),
            status: result.status,
            content_type: result.content_type.clone(),
            html,
            base_url: result.url.clone(),
            csp: None,
            frame_tree: None,
            cached_tree: OnceCell::new(),
            redirect_chain: None,
        })
    }

    /// Execute HTTP request with configurable retry and exponential backoff.
    ///
    /// Returns the response and any HTTP redirect hops that were captured.
    async fn fetch_with_retry(
        app: &Arc<App>,
        url: &str,
        extra_headers: &std::collections::HashMap<String, String>,
        retry_config: &crate::config::RetryConfig,
    ) -> anyhow::Result<(rquest::Response, Vec<RedirectHop>)> {
        let max_redirects = app.config.read().max_redirects;
        let redirect_hops: Arc<std::sync::Mutex<Vec<RedirectHop>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut attempt = 0u32;

        loop {
            // Clear stale hops from previous retry attempts
            redirect_hops.lock().unwrap().clear();

            let hops_clone = redirect_hops.clone();
            let max = max_redirects;

            let mut request_builder = app.http_client.get(url);

            // Apply interceptor-modified headers
            for (name, value) in extra_headers {
                request_builder = request_builder.header(name.as_str(), value.as_str());
            }

            // Set custom redirect policy to capture each hop
            request_builder =
                request_builder.redirect(rquest::redirect::Policy::custom(move |attempt| {
                    if attempt.previous().len() >= max {
                        return attempt.error("too many redirects");
                    }
                    let from = attempt
                        .previous()
                        .last()
                        .map(|u| u.to_string())
                        .unwrap_or_default();
                    let to = attempt.url().to_string();
                    let status = attempt.status().as_u16();
                    if let Ok(mut hops) = hops_clone.lock() {
                        hops.push(RedirectHop { from, to, status });
                    }
                    attempt.follow()
                }));

            // Build the request so we can retry it
            let request = request_builder
                .build()
                .map_err(|e| anyhow::anyhow!("failed to build request: {}", e))?;

            match app.http_client.execute(request).await {
                Ok(response) => {
                    let status = response.status().as_u16();
                    if retry_config.retry_on_statuses.contains(&status)
                        && attempt < retry_config.max_retries
                    {
                        attempt += 1;
                        let delay = compute_backoff(attempt, retry_config);
                        tracing::debug!(
                            "retry {}/{} for {} (status {}), waiting {}ms",
                            attempt,
                            retry_config.max_retries,
                            url,
                            status,
                            delay,
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        continue;
                    }
                    // Extract collected redirect hops
                    let hops = Arc::try_unwrap(redirect_hops)
                        .map(|m| m.into_inner().unwrap_or_default())
                        .unwrap_or_default();
                    return Ok((response, hops));
                }
                Err(e)
                    if (e.is_timeout() || e.is_connect()) && attempt < retry_config.max_retries =>
                {
                    attempt += 1;
                    let delay = compute_backoff(attempt, retry_config);
                    tracing::debug!(
                        "retry {}/{} for {} ({}), waiting {}ms",
                        attempt,
                        retry_config.max_retries,
                        url,
                        e,
                        delay,
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Create a Page from raw PDF bytes.
    pub fn from_pdf_bytes(
        bytes: &[u8],
        url: &str,
        status: u16,
        content_type: Option<String>,
    ) -> anyhow::Result<Self> {
        let (tree, _title) = crate::pdf::extract_pdf_tree(bytes)?;
        let html = Html::parse_document("<html><body></body></html>");

        Ok(Self {
            url: url.to_string(),
            status,
            content_type,
            html,
            base_url: url.to_string(),
            csp: None,
            frame_tree: None,
            cached_tree: OnceCell::from(Arc::new(tree)),
            redirect_chain: None,
        })
    }

    /// Create a Page from RSS/Atom feed bytes.
    pub fn from_feed_bytes(
        bytes: &[u8],
        url: &str,
        status: u16,
        content_type: Option<String>,
    ) -> anyhow::Result<Self> {
        let (tree, _title) = crate::feed::extract_feed_tree(bytes)?;
        let html = Html::parse_document("<html><body></body></html>");

        Ok(Self {
            url: url.to_string(),
            status,
            content_type,
            html,
            base_url: url.to_string(),
            csp: None,
            frame_tree: None,
            cached_tree: OnceCell::from(Arc::new(tree)),
            redirect_chain: None,
        })
    }

    #[must_use = "ignoring Result may silently swallow navigation errors"]
    #[cfg(feature = "js")]
    pub async fn from_url_with_js(app: &Arc<App>, url: &str, wait_ms: u32) -> anyhow::Result<Self> {
        let mut current_url = url.to_string();
        let mut depth = 0;

        loop {
            if depth >= Self::MAX_REDIRECT_DEPTH {
                anyhow::bail!(
                    "Redirect depth exceeded ({} >= {}) for {}",
                    depth,
                    Self::MAX_REDIRECT_DEPTH,
                    current_url
                );
            }

            let mut page = Self::fetch_and_create(app, &current_url, depth).await?;

            if page.cached_tree.get().is_some() {
                return Ok(page);
            }

            let html_str = page.html.html();
            let base_url = page.base_url.clone();
            let sandbox = &app.config.read().sandbox;
            let user_agent = app.config.read().user_agent.clone();
            let (final_body, _mutations) = crate::js::execute_js(
                &html_str,
                &base_url,
                wait_ms,
                Some(sandbox),
                &user_agent,
                Some(app.cookie_jar.clone()),
            )
            .await?;

            if let Some(nav_href) = Self::parse_js_navigation_href(&final_body) {
                let resolved = Url::parse(&page.url)
                    .and_then(|base| base.join(&nav_href))
                    .map(|u| u.to_string())
                    .unwrap_or_else(|_| nav_href.clone());
                tracing::debug!(target: "page", "JS location redirect: {} -> {}", page.url, resolved);
                current_url = resolved;
                depth += 1;
                continue;
            }

            let html = Html::parse_document(&final_body);
            let base_url = Self::extract_base_url(&html, &page.url, page.csp.as_ref());

            let config = app.config.read();
            let frame_tree = if config.parse_iframes {
                let max_depth = config.max_iframe_depth;
                drop(config);
                Some(
                    FrameTree::build(
                        html.clone(),
                        &page.url,
                        &base_url,
                        &app.http_client,
                        max_depth,
                    )
                    .await,
                )
            } else {
                drop(config);
                None
            };

            page.html = html;
            page.base_url = base_url;
            page.frame_tree = frame_tree;

            return Ok(page);
        }
    }

    /// Returns an error indicating JS support is not compiled in.
    #[cfg(not(feature = "js"))]
    pub async fn from_url_with_js(
        _app: &Arc<App>,
        _url: &str,
        _wait_ms: u32,
    ) -> anyhow::Result<Self> {
        anyhow::bail!("JavaScript execution is not available — rebuild with --features js");
    }

    pub fn from_html(html_str: &str, url: &str) -> Self {
        let html = Html::parse_document(html_str);
        let base_url = Self::extract_base_url(&html, url, None);
        Self {
            url: url.to_string(),
            status: 200,
            content_type: Some("text/html".to_string()),
            html,
            base_url,
            csp: None,
            frame_tree: None,
            cached_tree: OnceCell::new(),
            redirect_chain: None,
        }
    }

    /// Create a Page from HTML string with an already-built frame tree.
    pub fn from_html_with_frame_tree(html_str: &str, url: &str, frame_tree: FrameTree) -> Self {
        let html = Html::parse_document(html_str);
        let base_url = Self::extract_base_url(&html, url, None);
        Self {
            url: url.to_string(),
            status: 200,
            content_type: Some("text/html".to_string()),
            html,
            base_url,
            csp: None,
            frame_tree: Some(frame_tree),
            cached_tree: OnceCell::new(),
            redirect_chain: None,
        }
    }

    /// Create a Page from HTML string with iframe parsing using the given HTTP client.
    pub async fn from_html_with_frames(
        html_str: &str,
        url: &str,
        http_client: &rquest::Client,
        max_depth: usize,
    ) -> Self {
        let html = Html::parse_document(html_str);
        let base_url = Self::extract_base_url(&html, url, None);
        let frame_tree =
            FrameTree::build(html.clone(), url, &base_url, http_client, max_depth).await;
        Self {
            url: url.to_string(),
            status: 200,
            content_type: Some("text/html".to_string()),
            html,
            base_url,
            csp: None,
            frame_tree: Some(frame_tree),
            cached_tree: OnceCell::new(),
            redirect_chain: None,
        }
    }

    pub fn title(&self) -> Option<String> {
        if let Some(tree) = self.cached_tree.get() {
            return tree.root.name.clone();
        }

        let selector = Selector::parse("title").ok()?;
        self.html
            .select(&selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
    }

    /// Find the first element matching a CSS selector.
    pub fn query(&self, selector: &str) -> Option<ElementHandle> {
        let sel = Selector::parse(selector).ok()?;
        let el = self.html.select(&sel).next()?;
        Some(element_to_handle(&el, &self.html))
    }

    /// Find all elements matching a CSS selector.
    pub fn query_all(&self, selector: &str) -> Vec<ElementHandle> {
        let sel = match Selector::parse(selector) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        self.html
            .select(&sel)
            .map(|el| element_to_handle(&el, &self.html))
            .collect()
    }

    /// Find an element by its semantic role and optional name.
    pub fn find_by_role(&self, role: SemanticRole, name: Option<&str>) -> Option<ElementHandle> {
        let tree = self.semantic_tree_ref()?;
        let node = find_node_by_role(&tree.root, &role, name)?;
        node_to_handle(&node, &self.html)
    }

    /// Find an element by its semantic action string and optional name.
    pub fn find_by_action(&self, action: &str, name: Option<&str>) -> Option<ElementHandle> {
        let tree = self.semantic_tree_ref()?;
        let node = find_node_by_action(&tree.root, action, name)?;
        node_to_handle(&node, &self.html)
    }

    /// Find an interactive element by its element ID (e.g., 1, 2, 3).
    /// This is the preferred way for AI agents to reference elements.
    pub fn find_by_element_id(&self, id: usize) -> Option<ElementHandle> {
        let tree = self.semantic_tree_ref()?;
        let node = find_node_by_element_id(&tree.root, id)?;
        node_to_handle(&node, &self.html)
    }

    /// Get all interactive elements from the semantic tree.
    pub fn interactive_elements(&self) -> Vec<ElementHandle> {
        let Some(tree) = self.semantic_tree_ref() else {
            return Vec::new();
        };
        let nodes = collect_interactive(&tree.root);
        nodes
            .into_iter()
            .filter_map(|node| node_to_handle(&node, &self.html))
            .collect()
    }

    /// Check if a CSS selector matches any element in the page.
    pub fn has_selector(&self, selector: &str) -> bool {
        Selector::parse(selector)
            .ok()
            .map(|s| self.html.select(&s).next().is_some())
            .unwrap_or(false)
    }

    /// Extract base URL from HTML (public version for form submission).
    pub(crate) fn extract_base_url_static(html: &Html, fallback: &str) -> String {
        Self::extract_base_url(html, fallback, None)
    }

    pub fn semantic_tree(&self) -> Arc<SemanticTree> {
        self.cached_tree
            .get_or_init(|| {
                if let Some(ref frame_tree) = self.frame_tree {
                    Arc::new(SemanticTree::build_with_frames(&self.html, &self.base_url, frame_tree))
                } else {
                    Arc::new(SemanticTree::build(&self.html, &self.base_url))
                }
            })
            .clone()
    }

    pub fn semantic_tree_ref(&self) -> Option<&SemanticTree> {
        self.cached_tree.get().map(|arc| arc.as_ref())
    }

    /// Get the frame tree for this page (if iframe parsing was enabled).
    pub fn frame_tree(&self) -> Option<&FrameTree> { self.frame_tree.as_ref() }

    /// Find an element in a specific frame by CSS selector.
    pub fn query_in_frame(&self, frame_id: &FrameId, selector: &str) -> Option<ElementHandle> {
        let tree = self.frame_tree.as_ref()?;
        let frame = tree.find_frame(frame_id)?;
        let html = frame.parsed_html()?;
        let sel = Selector::parse(selector).ok()?;
        let el = html.select(&sel).next()?;
        Some(element_to_handle(&el, &html))
    }

    /// Find all elements in a specific frame matching a CSS selector.
    pub fn query_all_in_frame(&self, frame_id: &FrameId, selector: &str) -> Vec<ElementHandle> {
        let tree = match &self.frame_tree {
            Some(t) => t,
            None => return Vec::new(),
        };
        let frame = match tree.find_frame(frame_id) {
            Some(f) => f,
            None => return Vec::new(),
        };
        let html = match frame.parsed_html() {
            Some(h) => h,
            None => return Vec::new(),
        };
        let sel = match Selector::parse(selector) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let results: Vec<ElementHandle> = html
            .select(&sel)
            .map(|el| element_to_handle(&el, &html))
            .collect();
        results
    }

    /// Get the parsed HTML of a specific frame.
    pub fn frame_parsed_html(&self, frame_id: &FrameId) -> Option<Html> {
        let tree = self.frame_tree.as_ref()?;
        let frame = tree.find_frame(frame_id)?;
        frame.parsed_html()
    }

    /// Create a serializable snapshot of this page's state.
    pub fn snapshot(&self) -> PageSnapshot {
        PageSnapshot {
            url: self.url.clone(),
            status: self.status,
            content_type: self.content_type.clone(),
            title: self.title(),
            html: self.html.html(),
            redirect_chain: self.redirect_chain.clone(),
        }
    }

    /// Create a shallow clone by re-parsing the HTML source.
    /// Needed because `scraper::Html` is not `Clone`.
    /// Note: frame_tree is lost during shallow clone since child frames
    /// would need to be re-fetched.
    pub fn clone_shallow(&self) -> Self {
        Self {
            url: self.url.clone(),
            status: self.status,
            content_type: self.content_type.clone(),
            html: Html::parse_document(&self.html.html()),
            base_url: self.base_url.clone(),
            csp: self.csp.clone(),
            frame_tree: None,
            cached_tree: {
                let cell = OnceCell::new();
                if let Some(tree) = self.cached_tree.get() {
                    let _ = cell.set(tree.clone());
                }
                cell
            },
            redirect_chain: self.redirect_chain.clone(),
        }
    }

    pub fn navigation_graph(&self) -> NavigationGraph {
        NavigationGraph::build(&self.html, &self.url)
    }

    pub fn discover_subresources(&self, log: &Arc<std::sync::Mutex<pardus_debug::NetworkLog>>) {
        let start_id = {
            let log = log.lock().unwrap();
            log.next_id()
        };

        let subresources =
            pardus_debug::discover::discover_subresources(&self.html, &self.base_url, start_id);

        let mut log = log.lock().unwrap();
        for record in subresources {
            log.push(record);
        }
    }

    pub async fn fetch_subresources(
        client: &rquest::Client,
        log: &Arc<std::sync::Mutex<pardus_debug::NetworkLog>>,
    ) {
        // 1. Extract unfetched records with their types and IDs
        let entries: Vec<(usize, String, ResourceType)> = {
            let guard = log.lock().unwrap();
            guard
                .records
                .iter()
                .filter(|r| r.status.is_none() && r.error.is_none())
                .map(|r| (r.id, r.url.clone(), r.resource_type.clone()))
                .collect()
        };

        if entries.is_empty() {
            return;
        }

        // 2. Map to ResourceTask with priority based on resource type
        let tasks: Vec<ResourceTask> = entries
            .iter()
            .map(|(_, url, rt)| {
                let kind = resource_kind(rt);
                let priority = resource_type_priority(rt);
                ResourceTask::new(url.clone(), kind, priority)
            })
            .collect();

        // 3. Build scheduler with default config
        let cache = Arc::new(crate::cache::ResourceCache::new(10 * 1024 * 1024));
        let config = ResourceConfig::default();
        let scheduler = Arc::new(ResourceScheduler::new(client.clone(), config, cache));

        // 4. Fetch with priority ordering
        let results = scheduler.schedule_batch(tasks).await;

        // 5. Write results back into NetworkLog
        let mut guard = log.lock().unwrap();
        for result in &results {
            if let Some(record) = guard.records.iter_mut().find(|r| r.url == result.url) {
                if result.error.is_none() {
                    record.status = Some(result.status);
                    record.status_text =
                        Some(if result.status < 400 { "OK" } else { "Error" }.to_string());
                } else {
                    record.error = result.error.clone();
                }
                record.body_size = Some(result.size);
                record.content_type = result.content_type.clone();
                record.timing_ms = Some(result.duration_ms as u128);
                record.response_headers = result.response_headers_vec();
            }
        }
    }

    /// Resolve a URL relative to this page's base URL, preserving
    /// query parameters from the current URL when the relative URL
    /// contains only a query component (e.g., `?page=2`).
    ///
    /// Standard `Url::join` would replace all existing query params
    /// with the new ones. This method merges them instead.
    pub fn resolve_url_preserve_query(&self, href: &str) -> String {
        let base = match Url::parse(&self.base_url) {
            Ok(u) => u,
            Err(_) => return href.to_string(),
        };

        // If href is a query-only string (starts with "?"), merge params
        if href.starts_with('?') {
            let mut merged = base.clone();
            let relative = match Url::parse(&format!("https://dummy.com{}", href)) {
                Ok(u) => u,
                Err(_) => {
                    return base
                        .join(href)
                        .map(|u| u.to_string())
                        .unwrap_or_else(|_| href.to_string());
                }
            };

            let mut pairs: Vec<(String, String)> = base
                .query_pairs()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();

            for (k, v) in relative.query_pairs() {
                if let Some(existing) = pairs.iter_mut().find(|(ek, _)| *ek == k) {
                    existing.1 = v.to_string();
                } else {
                    pairs.push((k.to_string(), v.to_string()));
                }
            }

            {
                let mut qp = merged.query_pairs_mut();
                qp.clear();
                for (k, v) in &pairs {
                    qp.append_pair(k, v);
                }
            }
            return merged.to_string();
        }

        // For all other hrefs, standard resolution
        base.join(href)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| href.to_string())
    }

    fn extract_base_url(
        html: &Html,
        fallback: &str,
        csp: Option<&crate::csp::CspPolicySet>,
    ) -> String {
        if let Ok(selector) = Selector::parse("base[href]") {
            if let Some(base_el) = html.select(&selector).next() {
                if let Some(href) = base_el.value().attr("href") {
                    if let Ok(resolved) = Url::parse(fallback).and_then(|base| base.join(href)) {
                        // CSP: check base-uri directive
                        if let Some(csp_policy) = csp {
                            if let Ok(fallback_url) = Url::parse(fallback) {
                                let origin = fallback_url.origin();
                                if let Ok(resolved_url) = Url::parse(&resolved.to_string()) {
                                    let check = csp_policy.check_base_uri(&origin, &resolved_url);
                                    if !check.allowed {
                                        if let Some(ref directive) = check.violated_directive {
                                            crate::csp::report_violation(
                                                &crate::csp::CspViolation {
                                                    document_uri: fallback.to_string(),
                                                    blocked_uri: resolved.to_string(),
                                                    effective_directive: directive.clone(),
                                                    original_policy: String::new(),
                                                    disposition: crate::csp::Disposition::Enforce,
                                                    status_code: 0,
                                                },
                                            );
                                        }
                                        return fallback.to_string();
                                    }
                                }
                            }
                        }
                        return resolved.to_string();
                    }
                }
            }
        }
        fallback.to_string()
    }

    const MAX_REDIRECT_DEPTH: usize = 5;

    /// Parse `<meta http-equiv="refresh" content="<seconds>; url=<url>">` from HTML.
    ///
    /// Only the first matching meta tag is honored (browser behavior).
    /// Returns `Some(resolved_url)` for navigation, or `None` for reload-only / no tag.
    fn parse_meta_refresh(html: &Html, base_url: &Url) -> Option<String> {
        let selector = Selector::parse("meta[http-equiv]").ok()?;
        for el in html.select(&selector) {
            let equiv = el.value().attr("http-equiv")?;
            if equiv.eq_ignore_ascii_case("refresh") {
                let content = el.value().attr("content")?;
                return Self::parse_refresh_content(content, base_url);
            }
        }
        None
    }

    fn parse_refresh_content(content: &str, base_url: &Url) -> Option<String> {
        let parts: Vec<&str> = content.splitn(2, ';').collect();
        if parts.len() < 2 {
            return None;
        }
        let url_part = parts[1].trim();
        let url_part = url_part
            .strip_prefix("url=")
            .or_else(|| url_part.strip_prefix("URL="))
            .or_else(|| {
                let lower = url_part.to_lowercase();
                if lower.starts_with("url=") {
                    Some(&url_part[4..])
                } else {
                    None
                }
            })
            .or_else(|| {
                let lower = url_part.to_lowercase();
                if lower.starts_with("url ") {
                    let rest = url_part[4..].trim_start();
                    Some(
                        rest.strip_prefix("=")
                            .map(|u| u.trim_start())
                            .unwrap_or(rest),
                    )
                } else {
                    None
                }
            })?;
        let url_part = url_part.trim();

        let url = if url_part.starts_with('\'') && url_part.ends_with('\'')
            || url_part.starts_with('"') && url_part.ends_with('"')
        {
            &url_part[1..url_part.len() - 1]
        } else {
            url_part
        };

        if url.is_empty() {
            return None;
        }

        base_url.join(url).ok().map(|u| u.to_string())
    }

    /// Parse the `data-pardus-navigation-href` attribute from HTML returned
    /// by JS execution to detect `location.href`, `location.assign()`, or
    /// `location.replace()` redirects.
    #[cfg(feature = "js")]
    fn parse_js_navigation_href(html_str: &str) -> Option<String> {
        let doc = Html::parse_document(html_str);
        let selector = Selector::parse("html[data-pardus-navigation-href]").ok()?;
        doc.select(&selector).next().and_then(|el| {
            let href = el.value().attr("data-pardus-navigation-href")?;
            let trimmed = href.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("javascript:")
            {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }
}

fn record_main_request(
    app: &Arc<App>,
    original_url: &str,
    final_url: &str,
    status: u16,
    content_type: &Option<String>,
    body_size: usize,
    timing_ms: u128,
    response_headers: &[(String, String)],
    started_at: String,
    http_version: &str,
) {
    let mut record = NetworkRecord::fetched(
        1,
        "GET".to_string(),
        ResourceType::Document,
        "document · navigation".to_string(),
        final_url.to_string(),
        Initiator::Navigation,
    );
    record.status = Some(status);
    record.status_text = Some(http_status_text(status));
    record.content_type = content_type.clone();
    record.body_size = Some(body_size);
    record.timing_ms = Some(timing_ms);
    record.response_headers = response_headers.to_vec();
    record.started_at = Some(started_at);
    record.http_version = Some(http_version.to_string());

    if original_url != final_url {
        record.redirect_url = Some(final_url.to_string());
    }

    let mut log = app.network_log.lock().unwrap();
    log.push(record);
}

fn http_status_text(status: u16) -> String {
    match status {
        200 => "OK",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "",
    }
    .to_string()
}

fn format_http_version(version: http::Version) -> String {
    match version {
        http::Version::HTTP_09 => "HTTP/0.9",
        http::Version::HTTP_10 => "HTTP/1.0",
        http::Version::HTTP_11 => "HTTP/1.1",
        http::Version::HTTP_2 => "HTTP/2",
        http::Version::HTTP_3 => "HTTP/3",
        _ => "unknown",
    }
    .to_string()
}

/// Validate that the response content type is HTML-compatible.
/// Returns an error for binary or non-text responses (e.g. audio, images).
pub(crate) fn validate_content_type_pub(
    content_type: Option<&str>,
    url: &str,
) -> anyhow::Result<()> {
    if let Some(ct) = content_type {
        let ct_lower = ct.to_lowercase();
        let is_html = ct_lower.contains("text/html")
            || ct_lower.contains("application/xhtml")
            || ct_lower.contains("application/xml");
        let is_text = ct_lower.starts_with("text/");
        let is_feed = ct_lower.contains("application/rss+xml")
            || ct_lower.contains("application/atom+xml")
            || ct_lower.contains("application/feed+json");

        if !is_html && !is_text && !is_feed {
            anyhow::bail!(
                "Unsupported content type '{}' for URL '{}'. Expected HTML or text content.",
                ct.split(';').next().unwrap_or(ct).trim(),
                url
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// HTTP/2 push simulation: speculative early resource fetching
// ---------------------------------------------------------------------------

fn spawn_push_fetches(client: &rquest::Client, html_body: &str, base_url: &str, enabled: bool) {
    if !enabled {
        return;
    }

    let scanner = EarlyScanner::new();
    let result = scanner.scan(html_body, base_url);

    if result.resources.is_empty() {
        return;
    }

    let resources: Vec<crate::resource::Resource> = result.resources;
    let client = client.clone();

    tokio::spawn(async move {
        let config = crate::resource::ResourceConfig::default();
        let fetcher = ResourceFetcher::new(client, config);

        for resource in &resources {
            if let Err(e) = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                fetcher.fetch(&resource.url),
            )
            .await
            {
                tracing::trace!("push fetch failed for {}: {}", resource.url, e);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Semantic tree search helpers
// ---------------------------------------------------------------------------

fn find_node_by_role<'a>(
    node: &'a SemanticNode,
    target_role: &SemanticRole,
    target_name: Option<&str>,
) -> Option<&'a SemanticNode> {
    if std::mem::discriminant(&node.role) == std::mem::discriminant(target_role) {
        match target_name {
            Some(name) => {
                if node.name.as_deref() == Some(name) {
                    return Some(node);
                }
            }
            None => return Some(node),
        }
    }
    for child in &node.children {
        if let Some(found) = find_node_by_role(child, target_role, target_name) {
            return Some(found);
        }
    }
    None
}

fn find_node_by_action<'a>(
    node: &'a SemanticNode,
    action: &str,
    target_name: Option<&str>,
) -> Option<&'a SemanticNode> {
    if node.action.as_deref() == Some(action) {
        match target_name {
            Some(name) => {
                if node.name.as_deref() == Some(name) {
                    return Some(node);
                }
            }
            None => return Some(node),
        }
    }
    for child in &node.children {
        if let Some(found) = find_node_by_action(child, action, target_name) {
            return Some(found);
        }
    }
    None
}

fn find_node_by_element_id<'a>(
    node: &'a SemanticNode,
    target_id: usize,
) -> Option<&'a SemanticNode> {
    if node.element_id == Some(target_id) {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_node_by_element_id(child, target_id) {
            return Some(found);
        }
    }
    None
}

fn collect_interactive(node: &SemanticNode) -> Vec<&SemanticNode> {
    let mut result = Vec::new();
    if node.is_interactive {
        result.push(node);
    }
    for child in &node.children {
        result.extend(collect_interactive(child));
    }
    result
}

/// Try to find a scraper ElementRef matching a SemanticNode.
/// Uses the pre-computed selector stored in the node for reliable lookup.
fn node_to_handle(node: &SemanticNode, html: &Html) -> Option<ElementHandle> {
    // Use the pre-computed selector if available
    if let Some(selector_str) = &node.selector {
        if let Ok(sel) = Selector::parse(selector_str) {
            if let Some(el) = html.select(&sel).next() {
                // Use the pre-computed selector to ensure consistency
                return Some(build_handle_with_selector(&el, selector_str.clone()));
            }
        }
    }

    // Fallback: try to build selectors from node attributes
    let candidates = build_node_selectors(node);

    for candidate in candidates {
        if let Ok(sel) = Selector::parse(&candidate) {
            for el in html.select(&sel) {
                if element_matches_node(&el, node) {
                    return Some(element_to_handle(&el, html));
                }
            }
        }
    }

    None
}

/// Build an ElementHandle with a specific pre-computed selector.
fn build_handle_with_selector(el: &ElementRef, selector: String) -> ElementHandle {
    use crate::interact::element::{compute_action, compute_label};

    let tag = el.value().name().to_lowercase();
    let name_attr = el.value().attr("name").map(|s| s.to_string());
    let href = el.value().attr("href").map(|s| s.to_string());
    let input_type = el.value().attr("type").map(|s| s.to_string());
    let value = el.value().attr("value").map(|s| s.to_string());
    let id = el.value().attr("id").map(|s| s.to_string());
    let is_disabled = el.value().attr("disabled").is_some();

    let action = compute_action(&tag, input_type.as_deref());
    let label = compute_label(&tag, el);

    ElementHandle {
        selector,
        tag,
        id,
        name: name_attr,
        action,
        is_disabled,
        href,
        label,
        input_type,
        value,
        accept: None,
        multiple: false,
    }
}

fn build_node_selectors(node: &SemanticNode) -> Vec<String> {
    let mut selectors = Vec::new();

    // If the node has an href, try a[href="..."]
    if let Some(href) = &node.href {
        selectors.push(format!("{}[href=\"{}\"]", node.tag, href));
    }

    // Tag-based
    match node.tag.as_str() {
        "a" | "button" => {
            if let Some(_name) = &node.name {
                // Can't easily select by text content with CSS,
                // so just use tag
            }
        }
        "input" => {
            // Could try input[type="..."]
        }
        _ => {}
    }

    // Generic tag selector (last resort)
    selectors.push(node.tag.clone());

    selectors
}

fn element_matches_node(el: &ElementRef, node: &SemanticNode) -> bool {
    let tag = el.value().name();
    if tag != node.tag {
        return false;
    }

    // Check href for links
    if node.tag == "a" {
        if let Some(node_href) = &node.href {
            if el.value().attr("href") != Some(node_href.as_str()) {
                // The href might be resolved differently, but check anyway
            }
        }
    }

    // Check name for inputs
    if matches!(node.tag.as_str(), "input" | "select" | "textarea") {
        if let Some(node_name) = &node.name {
            if el.value().attr("name") != Some(node_name.as_str()) {
                return false;
            }
        }
    }

    true
}

/// Compute exponential backoff delay with jitter.
fn compute_backoff(attempt: u32, config: &crate::config::RetryConfig) -> u64 {
    let base = config.initial_backoff_ms as f64 * config.backoff_factor.powi((attempt as i32) - 1);
    // Add up to 30% jitter to spread retries
    let jitter = fastrand::f64() * 0.3 * base;
    let delay = (base + jitter) as u64;
    delay.min(config.max_backoff_ms)
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::*;

    fn parse(html: &str) -> Html { Html::parse_document(html) }

    fn base() -> Url { Url::parse("https://example.com/page").unwrap() }

    // ==================== parse_meta_refresh tests ====================

    #[test]
    fn test_meta_refresh_standard() {
        let html = r#"<html><head><meta http-equiv="refresh" content="0;url=https://other.com"></head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, Some("https://other.com/".to_string()));
    }

    #[test]
    fn test_meta_refresh_with_delay() {
        let html = r#"<html><head><meta http-equiv="refresh" content="5;url=https://other.com"></head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, Some("https://other.com/".to_string()));
    }

    #[test]
    fn test_meta_refresh_relative_url() {
        let html = r#"<html><head><meta http-equiv="refresh" content="0;url=/redirect"></head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, Some("https://example.com/redirect".to_string()));
    }

    #[test]
    fn test_meta_refresh_single_quotes() {
        let html = r#"<html><head><meta http-equiv="refresh" content="0;url='https://other.com'"></head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, Some("https://other.com/".to_string()));
    }

    #[test]
    fn test_meta_refresh_double_quotes() {
        let html = r#"<html><head><meta http-equiv="refresh" content="0;url=&quot;https://other.com&quot;"></head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, Some("https://other.com/".to_string()));
    }

    #[test]
    fn test_meta_refresh_reload_only() {
        let html =
            r#"<html><head><meta http-equiv="refresh" content="30"></head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, None);
    }

    #[test]
    fn test_meta_refresh_no_meta_tag() {
        let html = r#"<html><head><title>Hello</title></head><body><p>Hi</p></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, None);
    }

    #[test]
    fn test_meta_refresh_case_insensitive() {
        let html = r#"<html><head><meta http-equiv="Refresh" content="0;url=https://other.com"></head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, Some("https://other.com/".to_string()));
    }

    #[test]
    fn test_meta_refresh_uppercase_url() {
        let html = r#"<html><head><meta http-equiv="refresh" content="0;URL=https://other.com"></head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, Some("https://other.com/".to_string()));
    }

    #[test]
    fn test_meta_refresh_space_around_equals() {
        let html = r#"<html><head><meta http-equiv="refresh" content="0; url = https://other.com"></head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, Some("https://other.com/".to_string()));
    }

    #[test]
    fn test_meta_refresh_first_tag_wins() {
        let html = r#"<html><head>
            <meta http-equiv="refresh" content="0;url=https://first.com">
            <meta http-equiv="refresh" content="0;url=https://second.com">
        </head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, Some("https://first.com/".to_string()));
    }

    // ==================== parse_js_navigation_href tests ====================

    #[cfg(feature = "js")]
    #[test]
    fn test_parse_js_nav_href_present() {
        let html = r#"<html data-pardus-navigation-href="https://other.com"><head></head><body></body></html>"#;
        let result = Page::parse_js_navigation_href(html);
        assert_eq!(result, Some("https://other.com".to_string()));
    }

    #[cfg(feature = "js")]
    #[test]
    fn test_parse_js_nav_href_empty() {
        let html = r#"<html data-pardus-navigation-href=""><head></head><body></body></html>"#;
        let result = Page::parse_js_navigation_href(html);
        assert_eq!(result, None);
    }

    #[cfg(feature = "js")]
    #[test]
    fn test_parse_js_nav_href_hash() {
        let html =
            r##"<html data-pardus-navigation-href="#section"><head></head><body></body></html>"##;
        let result = Page::parse_js_navigation_href(html);
        assert_eq!(result, None);
    }

    #[cfg(feature = "js")]
    #[test]
    fn test_parse_js_nav_href_javascript() {
        let html = r#"<html data-pardus-navigation-href="javascript:void(0)"><head></head><body></body></html>"#;
        let result = Page::parse_js_navigation_href(html);
        assert_eq!(result, None);
    }

    #[cfg(feature = "js")]
    #[test]
    fn test_parse_js_nav_href_missing() {
        let html = r#"<html><head></head><body></body></html>"#;
        let result = Page::parse_js_navigation_href(html);
        assert_eq!(result, None);
    }

    #[cfg(feature = "js")]
    #[test]
    fn test_parse_js_nav_href_relative() {
        let html =
            r#"<html data-pardus-navigation-href="/new-page"><head></head><body></body></html>"#;
        let result = Page::parse_js_navigation_href(html);
        assert_eq!(result, Some("/new-page".to_string()));
    }

    #[cfg(feature = "js")]
    #[test]
    fn test_parse_js_nav_href_whitespace_trimmed() {
        let html =
            r#"<html data-pardus-navigation-href="  /trimmed  "><head></head><body></body></html>"#;
        let result = Page::parse_js_navigation_href(html);
        assert_eq!(result, Some("/trimmed".to_string()));
    }

    #[cfg(feature = "js")]
    #[test]
    fn test_parse_js_nav_href_data_uri_skipped() {
        let html = r#"<html data-pardus-navigation-href="data:text/html,test"><head></head><body></body></html>"#;
        let result = Page::parse_js_navigation_href(html);
        assert_eq!(result, Some("data:text/html,test".to_string()));
    }

    #[cfg(feature = "js")]
    #[test]
    fn test_parse_js_nav_href_javascript_with_spaces() {
        let html = r#"<html data-pardus-navigation-href="javascript: alert(1)"><head></head><body></body></html>"#;
        let result = Page::parse_js_navigation_href(html);
        assert_eq!(result, None);
    }

    // ==================== parse_refresh_content tests ====================

    #[test]
    fn test_refresh_content_with_query_params() {
        let html = r#"<html><head><meta http-equiv="refresh" content="0;url=https://example.com/redirect?foo=bar&baz=1"></head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(
            result,
            Some("https://example.com/redirect?foo=bar&baz=1".to_string())
        );
    }

    #[test]
    fn test_refresh_content_with_fragment() {
        let html = r#"<html><head><meta http-equiv="refresh" content="0;url=https://example.com/page#section"></head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, Some("https://example.com/page#section".to_string()));
    }

    #[test]
    fn test_refresh_content_empty_url_after_equals() {
        let html = r#"<html><head><meta http-equiv="refresh" content="0;url="></head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, None);
    }

    #[test]
    fn test_refresh_content_url_only_no_semicolon() {
        let html = r#"<html><head><meta http-equiv="refresh" content="url=https://example.com"></head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, None);
    }

    #[test]
    fn test_refresh_content_multiple_semicolons_in_url() {
        let html = r#"<html><head><meta http-equiv="refresh" content="0;url=/path?a=1;b=2"></head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, Some("https://example.com/path?a=1;b=2".to_string()));
    }

    #[test]
    fn test_refresh_content_zero_delay() {
        let html = r#"<html><head><meta http-equiv="refresh" content="0;url=https://example.com"></head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, Some("https://example.com/".to_string()));
    }

    #[test]
    fn test_refresh_content_large_delay() {
        let html = r#"<html><head><meta http-equiv="refresh" content="3600;url=https://example.com"></head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, Some("https://example.com/".to_string()));
    }

    #[test]
    fn test_refresh_content_non_http_meta_tag() {
        let html = r#"<html><head><meta http-equiv="content-type" content="text/html"><meta http-equiv="refresh" content="0;url=https://other.com"></head><body></body></html>"#;
        let result = Page::parse_meta_refresh(&parse(html), &base());
        assert_eq!(result, Some("https://other.com/".to_string()));
    }

    // ==================== meta_refresh_url (Page method) tests ====================

    #[test]
    fn test_page_meta_refresh_url_with_refresh() {
        let html = r#"<html><head><meta http-equiv="refresh" content="0;url=https://other.com"></head><body></body></html>"#;
        let page = Page::from_html(html, "https://example.com");
        assert_eq!(
            page.meta_refresh_url(),
            Some("https://other.com/".to_string())
        );
    }

    #[test]
    fn test_page_meta_refresh_url_without_refresh() {
        let html = r#"<html><head><title>Hello</title></head><body><p>Hi</p></body></html>"#;
        let page = Page::from_html(html, "https://example.com");
        assert_eq!(page.meta_refresh_url(), None);
    }

    #[test]
    fn test_page_meta_refresh_url_relative() {
        let html = r#"<html><head><meta http-equiv="refresh" content="0;url=/new-path"></head><body></body></html>"#;
        let page = Page::from_html(html, "https://example.com/page");
        assert_eq!(
            page.meta_refresh_url(),
            Some("https://example.com/new-path".to_string())
        );
    }

    #[test]
    fn test_page_meta_refresh_url_with_base_tag() {
        let html = r#"<html><head><base href="https://cdn.example.com/"><meta http-equiv="refresh" content="0;url=/assets/page"></head><body></body></html>"#;
        let page = Page::from_html(html, "https://example.com");
        assert_eq!(
            page.meta_refresh_url(),
            Some("https://cdn.example.com/assets/page".to_string())
        );
    }

    // ==================== MAX_REDIRECT_DEPTH tests ====================

    #[test]
    fn test_max_redirect_depth_value() {
        assert_eq!(Page::MAX_REDIRECT_DEPTH, 5);
    }
}

// ---------------------------------------------------------------------------
// OAuth redirect capture
// ---------------------------------------------------------------------------

/// Result of an OAuth-aware navigation that captures redirect callbacks.
pub enum OAuthNavigateResult {
    /// Landed on an intermediate page (e.g., login form, consent screen).
    Page(Page),
    /// Captured a redirect to the callback URL with the authorization code.
    Callback {
        /// The full callback URL.
        url: String,
        /// The authorization code extracted from the query string.
        code: String,
        /// The state parameter from the callback (for CSRF validation).
        state: String,
    },
}

impl Page {
    /// Navigate to a URL with redirect interception for OAuth callback capture.
    ///
    /// Behaves like `from_url` but stops following redirects when the target
    /// URL matches the given `callback_url` prefix. This allows extracting the
    /// `code` and `state` parameters from OAuth/OIDC callbacks.
    ///
    /// If no redirect to the callback URL is encountered, returns the final
    /// page as `OAuthNavigateResult::Page` (e.g., a login form).
    pub async fn navigate_with_redirect_capture(
        app: &Arc<App>,
        url: &str,
        callback_url: &str,
    ) -> anyhow::Result<OAuthNavigateResult> {
        let callback_prefix = callback_url.to_string();
        let captured_redirect: Arc<std::sync::Mutex<Option<String>>> =
            Arc::new(std::sync::Mutex::new(None));

        let max_redirects = app.config.read().max_redirects;
        let redirect_hops: Arc<std::sync::Mutex<Vec<RedirectHop>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));

        let hops_clone = redirect_hops.clone();
        let captured_clone = captured_redirect.clone();
        let max = max_redirects;

        let mut request_builder = app.http_client.get(url);

        request_builder =
            request_builder.redirect(rquest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= max {
                    return attempt.error("too many redirects");
                }

                let target_url = attempt.url().to_string();

                // Check if this redirect targets the callback URL
                if target_url.starts_with(&callback_prefix)
                    || url_matches_callback(attempt.url(), &callback_prefix)
                {
                    if let Ok(mut captured) = captured_clone.lock() {
                        *captured = Some(target_url);
                    }
                    return attempt.stop();
                }

                // Record the redirect hop
                let from = attempt
                    .previous()
                    .last()
                    .map(|u| u.to_string())
                    .unwrap_or_default();
                let status = attempt.status().as_u16();
                if let Ok(mut hops) = hops_clone.lock() {
                    hops.push(RedirectHop {
                        from,
                        to: target_url,
                        status,
                    });
                }

                attempt.follow()
            }));

        let request = request_builder
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build OAuth navigation request: {e}"))?;

        let response = app
            .http_client
            .execute(request)
            .await
            .map_err(|e| anyhow::anyhow!("OAuth navigation request failed: {e}"))?;

        // Check if we captured a redirect to the callback URL
        let captured = captured_redirect.lock().unwrap().take();

        if let Some(callback_target) = captured {
            let parsed = Url::parse(&callback_target)
                .map_err(|e| anyhow::anyhow!("failed to parse callback URL: {e}"))?;

            let params: std::collections::HashMap<String, String> =
                parsed.query_pairs().into_owned().collect();

            let code = params.get("code").cloned().ok_or_else(|| {
                anyhow::anyhow!("callback URL missing 'code' parameter: {}", callback_target)
            })?;
            let state = params.get("state").cloned().ok_or_else(|| {
                anyhow::anyhow!(
                    "callback URL missing 'state' parameter: {}",
                    callback_target
                )
            })?;

            return Ok(OAuthNavigateResult::Callback {
                url: callback_target,
                code,
                state,
            });
        }

        // No redirect captured — landed on an intermediate page (login form, etc.)
        let status = response.status().as_u16();
        let final_url = response.url().to_string();
        let body_bytes = response
            .bytes()
            .await
            .map_err(|e| anyhow::anyhow!("failed to read response body: {e}"))?;
        let body_str = String::from_utf8_lossy(&body_bytes).to_string();

        let html = Html::parse_document(&body_str);

        let page = Self {
            url: final_url,
            status,
            content_type: Some("text/html".to_string()),
            html,
            base_url: url.to_string(),
            csp: None,
            frame_tree: None,
            cached_tree: OnceCell::new(),
            redirect_chain: None,
        };

        Ok(OAuthNavigateResult::Page(page))
    }
}

/// Check if a URL matches the callback URL by comparing scheme+host+port+path.
fn url_matches_callback(url: &url::Url, callback_prefix: &str) -> bool {
    let Ok(cb_url) = url::Url::parse(callback_prefix) else {
        return url.as_str().starts_with(callback_prefix);
    };

    url.scheme() == cb_url.scheme()
        && url.host() == cb_url.host()
        && url.port() == cb_url.port()
        && url.path() == cb_url.path()
}

/// Map `ResourceType` to `ResourceKind` for scheduling.
fn resource_kind(rt: &ResourceType) -> ResourceKind {
    match rt {
        ResourceType::Document => ResourceKind::Document,
        ResourceType::Stylesheet => ResourceKind::Stylesheet,
        ResourceType::Script => ResourceKind::Script,
        ResourceType::Image => ResourceKind::Image,
        ResourceType::Font => ResourceKind::Font,
        ResourceType::Media => ResourceKind::Media,
        _ => ResourceKind::Other,
    }
}

/// Map `ResourceType` to a priority band (lower = higher priority).
///
/// | Band       | Value | Types                        |
/// |------------|-------|------------------------------|
/// | Critical   | 0     | Document, Stylesheet         |
/// | High       | 32    | Script, Font                 |
/// | Normal     | 96    | Fetch, Xhr, WebSocket, Other |
/// | Low        | 160   | Image                        |
/// | Background | 224   | Media                        |
fn resource_type_priority(rt: &ResourceType) -> u8 {
    match rt {
        ResourceType::Document | ResourceType::Stylesheet => 0,
        ResourceType::Script | ResourceType::Font => 32,
        ResourceType::Image => 160,
        ResourceType::Media => 224,
        _ => 96,
    }
}

impl FetchedResponse {
    fn from_dedup_result(result: Arc<crate::dedup::DedupResult>) -> Self {
        Self {
            final_url: result.url.clone(),
            status: result.status,
            content_type: result.content_type.clone(),
            resp_headers: result.headers.clone(),
            http_version: result.http_version.clone(),
            body_bytes: result.body.clone(),
            redirect_hops: Vec::new(),
            started_at: String::new(),
            elapsed_ms: 0,
        }
    }

    fn from_mock(url: &str, mock: &crate::intercept::MockResponse) -> Self {
        let content_type = mock.headers.get("content-type").cloned();
        Self {
            final_url: url.to_string(),
            status: mock.status,
            content_type,
            resp_headers: mock
                .headers
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            http_version: String::new(),
            body_bytes: mock.body.clone(),
            redirect_hops: Vec::new(),
            started_at: String::new(),
            elapsed_ms: 0,
        }
    }

    async fn into_page(
        self,
        app: &Arc<App>,
        url_key: &str,
        event_sink: Option<std::sync::Arc<dyn crate::parser::StreamingEventSink + Send + Sync>>,
    ) -> anyhow::Result<Page> {
        let is_pdf = self.content_type.as_ref().map_or(false, |ct| {
            ct.split(';').next().unwrap_or(ct).trim().to_lowercase() == "application/pdf"
        });

        if is_pdf {
            let body_size = self.body_bytes.len();
            let config = app.config.read();
            if config.sandbox.max_page_size > 0 && body_size > config.sandbox.max_page_size {
                app.dedup.remove(url_key);
                anyhow::bail!(
                    "PDF size ({} bytes) exceeds sandbox limit ({} bytes)",
                    body_size,
                    config.sandbox.max_page_size
                );
            }
            return Page::from_pdf_bytes(
                &self.body_bytes,
                &self.final_url,
                self.status,
                self.content_type,
            );
        }

        let body_str = String::from_utf8_lossy(&self.body_bytes).to_string();
        let body_size = body_str.len();

        if crate::feed::is_feed_content(body_str.as_bytes(), self.content_type.as_deref()) {
            let config = app.config.read();
            if config.sandbox.max_page_size > 0 && body_size > config.sandbox.max_page_size {
                app.dedup.remove(url_key);
                anyhow::bail!(
                    "Feed size ({} bytes) exceeds sandbox limit ({} bytes)",
                    body_size,
                    config.sandbox.max_page_size
                );
            }
            return Page::from_feed_bytes(
                body_str.as_bytes(),
                &self.final_url,
                self.status,
                self.content_type,
            );
        }

        let config = app.config.read();
        if config.sandbox.max_page_size > 0 && body_size > config.sandbox.max_page_size {
            app.dedup.remove(url_key);
            anyhow::bail!(
                "Page size ({} bytes) exceeds sandbox limit ({} bytes)",
                body_size,
                config.sandbox.max_page_size
            );
        }

        validate_content_type_pub(self.content_type.as_deref(), &self.final_url)?;

        let push_enabled = config.push.enable_push && !config.sandbox.disable_push;
        let csp_policy = config.csp.parse_policy(&self.resp_headers);
        drop(config);

        spawn_push_fetches(&app.http_client, &body_str, &self.final_url, push_enabled);

        if let Some(ref sink) = event_sink {
            if let Ok(mut stream_parser) =
                crate::parser::streaming_semantic::StreamingHtmlParser::new(&self.final_url, None)
            {
                if let Ok(new_nodes) = stream_parser.feed(&self.body_bytes) {
                    for node in &new_nodes {
                        sink.emit(node.clone());
                    }
                }
            }
        }

        let html = Html::parse_document(&body_str);
        let base_url = Page::extract_base_url(&html, &self.final_url, csp_policy.as_ref());

        let config = app.config.read();
        let frame_tree = if config.parse_iframes {
            let max_depth = config.max_iframe_depth;
            drop(config);
            Some(
                FrameTree::build(
                    html.clone(),
                    &self.final_url,
                    &base_url,
                    &app.http_client,
                    max_depth,
                )
                .await,
            )
        } else {
            drop(config);
            None
        };

        Ok(Page {
            url: self.final_url,
            status: self.status,
            content_type: self.content_type,
            html,
            base_url,
            csp: csp_policy,
            frame_tree,
            cached_tree: OnceCell::new(),
            redirect_chain: if self.redirect_hops.is_empty() {
                None
            } else {
                Some(RedirectChain {
                    hops: self.redirect_hops,
                })
            },
        })
    }
}
