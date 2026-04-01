use scraper::{Html, Selector};
use std::sync::Arc;
use std::time::Instant;
use url::Url;

use crate::app::App;
use crate::semantic::tree::SemanticTree;
use crate::navigation::graph::NavigationGraph;

use pardus_debug::{NetworkRecord, ResourceType, Initiator};

pub struct Page {
    pub url: String,
    pub status: u16,
    pub content_type: Option<String>,
    pub html: Html,
    pub base_url: String,
}

impl Page {
    pub async fn from_url(app: &Arc<App>, url: &str) -> anyhow::Result<Self> {
        let start = Instant::now();

        let response = app.http_client.get(url).send().await?;
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

        let body = response.text().await?;
        let body_size = body.len();
        let timing_ms = start.elapsed().as_millis();

        record_main_request(app, url, &final_url, status, &content_type, body_size, timing_ms, &resp_headers);

        let html = Html::parse_document(&body);
        let base_url = Self::extract_base_url(&html, &final_url);

        Ok(Self {
            url: final_url,
            status,
            content_type,
            html,
            base_url,
        })
    }

    pub async fn from_url_with_js(app: &Arc<App>, url: &str, wait_ms: u32) -> anyhow::Result<Self> {
        let start = Instant::now();

        let response = app.http_client.get(url).send().await?;
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

        let body = response.text().await?;
        let body_size = body.len();
        let timing_ms = start.elapsed().as_millis();

        record_main_request(app, url, &final_url, status, &content_type, body_size, timing_ms, &resp_headers);

        let base_url = Self::extract_base_url(&Html::parse_document(&body), &final_url);
        let final_body = crate::js::execute_js(&body, &base_url, wait_ms).await?;

        let html = Html::parse_document(&final_body);
        let base_url = Self::extract_base_url(&html, &final_url);

        Ok(Self {
            url: final_url,
            status,
            content_type,
            html,
            base_url,
        })
    }

    pub fn from_html(html_str: &str, url: &str) -> Self {
        let html = Html::parse_document(html_str);
        let base_url = Self::extract_base_url(&html, url);
        Self {
            url: url.to_string(),
            status: 200,
            content_type: Some("text/html".to_string()),
            html,
            base_url,
        }
    }

    pub fn title(&self) -> Option<String> {
        let selector = Selector::parse("title").ok()?;
        self.html
            .select(&selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
    }

    pub fn semantic_tree(&self) -> SemanticTree {
        SemanticTree::build(&self.html, &self.base_url)
    }

    pub fn navigation_graph(&self) -> NavigationGraph {
        NavigationGraph::build(&self.html, &self.url)
    }

    pub fn discover_subresources(&self, log: &Arc<std::sync::Mutex<pardus_debug::NetworkLog>>) {
        let start_id = {
            let log = log.lock().unwrap();
            log.next_id()
        };

        let subresources = pardus_debug::discover::discover_subresources(
            &self.html,
            &self.base_url,
            start_id,
        );

        let mut log = log.lock().unwrap();
        for record in subresources {
            log.push(record);
        }
    }

    pub async fn fetch_subresources(
        client: &reqwest::Client,
        log: &Arc<std::sync::Mutex<pardus_debug::NetworkLog>>,
    ) {
        pardus_debug::fetch::fetch_subresources(client, log, 6).await;
    }

    fn extract_base_url(html: &Html, fallback: &str) -> String {
        if let Ok(selector) = Selector::parse("base[href]") {
            if let Some(base_el) = html.select(&selector).next() {
                if let Some(href) = base_el.value().attr("href") {
                    if let Ok(resolved) = Url::parse(fallback)
                        .and_then(|base| base.join(href))
                    {
                        return resolved.to_string();
                    }
                }
            }
        }
        fallback.to_string()
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
    }.to_string()
}
