//! IFrame and frame tree handling.
//!
//! Supports recursive discovery and parsing of `<iframe>` and `<frame>` elements.
//! Frame IDs use a dot-separated depth path (e.g., "0", "0.1", "0.1.3").

use std::fmt;

use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use tracing::{instrument, trace, warn};
use url::Url;

use crate::semantic::build_unique_selector;

// ---------------------------------------------------------------------------
// FrameId
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FrameId(pub String);

impl FrameId {
    pub fn root() -> Self { Self("0".to_string()) }

    pub fn child(&self, index: usize) -> Self { Self(format!("{}.{}", self.0, index)) }

    pub fn depth(&self) -> usize { self.0.split('.').count().saturating_sub(1) }

    pub fn is_root(&self) -> bool { self.0 == "0" }

    pub fn as_str(&self) -> &str { &self.0 }

    pub fn parent(&self) -> Option<Self> {
        if self.is_root() {
            return None;
        }
        let pos = self.0.rfind('.')?;
        Some(Self(self.0[..pos].to_string()))
    }
}

impl fmt::Display for FrameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}

impl Default for FrameId {
    fn default() -> Self { Self::root() }
}

// ---------------------------------------------------------------------------
// FrameData
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameData {
    pub id: FrameId,
    pub url: String,
    /// Raw HTML string for this frame's content. `None` if fetch failed.
    pub html: Option<String>,
    pub srcdoc: Option<String>,
    pub sandbox: Option<String>,
    pub sandbox_tokens: Vec<String>,
    pub parent_id: Option<FrameId>,
    pub child_frames: Vec<FrameData>,
    pub load_error: Option<String>,
}

impl FrameData {
    pub fn scripts_blocked(&self) -> bool {
        if self.sandbox_tokens.is_empty() {
            return false;
        }
        !self.sandbox_tokens.iter().any(|t| t == "allow-scripts")
    }

    pub fn is_cross_origin_sandboxed(&self) -> bool {
        if self.sandbox_tokens.is_empty() {
            return false;
        }
        !self.sandbox_tokens.iter().any(|t| t == "allow-same-origin")
    }

    /// Parse the raw HTML string into a `scraper::Html` document.
    /// Returns `None` if the frame has no HTML content.
    pub fn parsed_html(&self) -> Option<Html> {
        self.html.as_ref().map(|s| Html::parse_document(s))
    }
}

// ---------------------------------------------------------------------------
// FrameTree
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameTree {
    pub root: FrameData,
}

impl FrameTree {
    #[instrument(skip(root_html, http_client), level = "debug")]
    pub async fn build(
        root_html: Html,
        root_url: &str,
        base_url: &str,
        http_client: &rquest::Client,
        max_depth: usize,
    ) -> Self {
        let root_html_str = root_html.html().to_string();
        let root = Self::build_frame(
            FrameId::root(),
            root_html_str,
            root_url,
            base_url,
            None,
            http_client,
            max_depth,
        )
        .await;

        Self { root }
    }

    #[instrument(skip(html, http_client), level = "trace")]
    async fn build_frame(
        id: FrameId,
        html: String,
        url: &str,
        base_url: &str,
        parent_id: Option<FrameId>,
        http_client: &rquest::Client,
        remaining_depth: usize,
    ) -> FrameData {
        let depth = id.depth();
        let can_recurse = remaining_depth > 0 && depth < remaining_depth;

        let discovered = if can_recurse {
            let parsed = Html::parse_document(&html);
            discover_iframes(&parsed)
        } else {
            Vec::new()
        };

        let child_frames = if can_recurse && !discovered.is_empty() {
            fetch_children(discovered, base_url, http_client, remaining_depth).await
        } else {
            Vec::new()
        };

        FrameData {
            id: id.clone(),
            url: url.to_string(),
            html: Some(html),
            srcdoc: None,
            sandbox: None,
            sandbox_tokens: Vec::new(),
            parent_id,
            child_frames,
            load_error: None,
        }
    }

    pub fn empty(root_html: Html, root_url: &str) -> Self {
        Self {
            root: FrameData {
                id: FrameId::root(),
                url: root_url.to_string(),
                html: Some(root_html.html().to_string()),
                srcdoc: None,
                sandbox: None,
                sandbox_tokens: Vec::new(),
                parent_id: None,
                child_frames: Vec::new(),
                load_error: None,
            },
        }
    }

    pub fn frame_count(&self) -> usize { Self::count_frames(&self.root) }

    fn count_frames(frame: &FrameData) -> usize {
        1 + frame
            .child_frames
            .iter()
            .map(Self::count_frames)
            .sum::<usize>()
    }

    pub fn max_depth(&self) -> usize { Self::measure_depth(&self.root, 0) }

    fn measure_depth(frame: &FrameData, current: usize) -> usize {
        if frame.child_frames.is_empty() {
            return current;
        }
        frame
            .child_frames
            .iter()
            .map(|c| Self::measure_depth(c, current + 1))
            .max()
            .unwrap_or(current)
    }

    pub fn find_frame(&self, id: &FrameId) -> Option<&FrameData> {
        if self.root.id == *id {
            return Some(&self.root);
        }
        for child in &self.root.child_frames {
            if let Some(f) = Self::find_in_subtree(child, id) {
                return Some(f);
            }
        }
        None
    }

    fn find_in_subtree<'a>(frame: &'a FrameData, id: &FrameId) -> Option<&'a FrameData> {
        if frame.id == *id {
            return Some(frame);
        }
        for child in &frame.child_frames {
            if let Some(f) = Self::find_in_subtree(child, id) {
                return Some(f);
            }
        }
        None
    }

    pub fn all_frames(&self) -> Vec<&FrameData> {
        let mut result = Vec::new();
        Self::collect_frames(&self.root, &mut result);
        result
    }

    fn collect_frames<'a>(frame: &'a FrameData, result: &mut Vec<&'a FrameData>) {
        result.push(frame);
        for child in &frame.child_frames {
            Self::collect_frames(child, result);
        }
    }
}

// ---------------------------------------------------------------------------
// Iframe discovery and fetching
// ---------------------------------------------------------------------------

struct DiscoveredFrame {
    #[allow(dead_code)]
    index: usize,
    #[allow(dead_code)]
    selector: String,
    src: Option<String>,
    srcdoc: Option<String>,
    sandbox: Option<String>,
}

fn discover_iframes(parent_html: &Html) -> Vec<(usize, DiscoveredFrame)> {
    let iframe_selector = Selector::parse("iframe, frame").unwrap();
    let mut discovered = Vec::new();

    for (idx, el) in parent_html.select(&iframe_selector).enumerate() {
        let src = el.value().attr("src").map(|s| s.to_string());
        let srcdoc = el.value().attr("srcdoc").map(|s| s.to_string());
        let sandbox = el.value().attr("sandbox").map(|s| s.to_string());

        discovered.push((
            idx,
            DiscoveredFrame {
                index: idx,
                selector: build_unique_selector(&el, parent_html),
                src,
                srcdoc,
                sandbox,
            },
        ));
    }

    if !discovered.is_empty() {
        trace!("discovered {} child frames", discovered.len());
    }

    discovered
}

fn fetch_children<'a>(
    discovered: Vec<(usize, DiscoveredFrame)>,
    parent_base_url: &'a str,
    http_client: &'a rquest::Client,
    remaining_depth: usize,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<FrameData>> + Send + 'a>> {
    Box::pin(async move {
        let mut results = Vec::with_capacity(discovered.len());

        for (idx, frame_info) in discovered {
            let frame_id = FrameId::root().child(idx);
            let base_url = parent_base_url.to_string();
            let child = build_child_frame(
                frame_id,
                frame_info,
                &base_url,
                http_client,
                remaining_depth,
            )
            .await;
            results.push(child);
        }

        results.sort_by_key(|f| f.id.0.clone());
        results
    })
}

async fn build_child_frame(
    id: FrameId,
    frame_info: DiscoveredFrame,
    parent_base_url: &str,
    http_client: &rquest::Client,
    remaining_depth: usize,
) -> FrameData {
    let parent_id = id.parent();
    let sandbox_tokens = frame_info
        .sandbox
        .as_deref()
        .map(|s| s.split_whitespace().map(String::from).collect())
        .unwrap_or_default();

    if let Some(ref srcdoc_content) = frame_info.srcdoc {
        let srcdoc_owned = srcdoc_content.to_string();
        let discovered = if remaining_depth > 1 {
            let parsed = Html::parse_document(&srcdoc_owned);
            discover_iframes(&parsed)
        } else {
            Vec::new()
        };
        let child_frames = if remaining_depth > 1 {
            fetch_children(
                discovered,
                parent_base_url,
                http_client,
                remaining_depth - 1,
            )
            .await
        } else {
            Vec::new()
        };

        return FrameData {
            id,
            url: "about:srcdoc".to_string(),
            html: Some(srcdoc_owned),
            srcdoc: Some(srcdoc_content.to_string()),
            sandbox: frame_info.sandbox,
            sandbox_tokens,
            parent_id,
            child_frames,
            load_error: None,
        };
    }

    let src_url = match &frame_info.src {
        Some(s) if !s.is_empty() => {
            match Url::parse(parent_base_url).and_then(|base| base.join(s)) {
                Ok(resolved) => resolved.to_string(),
                Err(_) => s.clone(),
            }
        }
        _ => "about:blank".to_string(),
    };

    let fetch_result = fetch_frame_content(&src_url, http_client).await;

    match fetch_result {
        Ok((fetched_html_str, fetched_url)) => {
            let discovered = if remaining_depth > 1 {
                let parsed = Html::parse_document(&fetched_html_str);
                discover_iframes(&parsed)
            } else {
                Vec::new()
            };
            let child_frames = if remaining_depth > 1 {
                fetch_children(discovered, &fetched_url, http_client, remaining_depth - 1).await
            } else {
                Vec::new()
            };

            FrameData {
                id,
                url: fetched_url,
                html: Some(fetched_html_str),
                srcdoc: None,
                sandbox: frame_info.sandbox,
                sandbox_tokens,
                parent_id,
                child_frames,
                load_error: None,
            }
        }
        Err(e) => {
            warn!("failed to fetch iframe {} {}: {}", id, src_url, e);
            FrameData {
                id,
                url: src_url,
                html: None,
                srcdoc: None,
                sandbox: frame_info.sandbox,
                sandbox_tokens,
                parent_id,
                child_frames: Vec::new(),
                load_error: Some(e.to_string()),
            }
        }
    }
}

async fn fetch_frame_content(
    url: &str,
    client: &rquest::Client,
) -> anyhow::Result<(String, String)> {
    let response = client.get(url).send().await?;

    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("HTTP {} for iframe {}", status.as_u16(), url);
    }

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if !content_type.contains("text/html") && !content_type.contains("application/xhtml") {
        anyhow::bail!(
            "iframe content type '{}' is not HTML for {}",
            content_type,
            url
        );
    }

    let final_url = response.url().to_string();
    let body = response.text().await?;

    Ok((body, final_url))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_id_root() {
        let root = FrameId::root();
        assert_eq!(root.as_str(), "0");
        assert!(root.is_root());
        assert_eq!(root.depth(), 0);
        assert!(root.parent().is_none());
    }

    #[test]
    fn test_frame_id_child() {
        let root = FrameId::root();
        let child = root.child(0);
        let child2 = root.child(1);
        assert_eq!(child.as_str(), "0.0");
        assert_eq!(child.depth(), 1);
        assert!(!child.is_root());
        assert_eq!(child.parent().as_ref().unwrap().as_str(), "0");
        assert_eq!(child2.as_str(), "0.1");
    }

    #[test]
    fn test_frame_id_nested() {
        let root = FrameId::root();
        let child = root.child(2);
        let grandchild = child.child(3);
        assert_eq!(grandchild.as_str(), "0.2.3");
        assert_eq!(grandchild.depth(), 2);
        assert_eq!(grandchild.parent().unwrap().as_str(), "0.2");
    }

    #[test]
    fn test_sandbox_tokens() {
        let mut frame = FrameData {
            id: FrameId::root(),
            url: String::new(),
            html: None,
            srcdoc: None,
            sandbox: Some("allow-scripts allow-forms".to_string()),
            sandbox_tokens: vec!["allow-scripts".to_string(), "allow-forms".to_string()],
            parent_id: None,
            child_frames: Vec::new(),
            load_error: None,
        };
        assert!(!frame.scripts_blocked());
        assert!(frame.is_cross_origin_sandboxed());

        frame.sandbox_tokens = vec!["allow-forms".to_string()];
        assert!(frame.scripts_blocked());

        frame.sandbox_tokens = vec!["allow-scripts".to_string()];
        assert!(frame.is_cross_origin_sandboxed());

        frame.sandbox_tokens = Vec::new();
        assert!(!frame.scripts_blocked());
        assert!(!frame.is_cross_origin_sandboxed());
    }

    #[test]
    fn test_empty_frame_tree() {
        let html = Html::parse_document("<html><body>Hello</body></html>");
        let tree = FrameTree::empty(html, "https://example.com");
        assert_eq!(tree.frame_count(), 1);
        assert_eq!(tree.max_depth(), 0);
        assert!(tree.find_frame(&FrameId::root()).is_some());
        assert!(
            tree.find_frame(&FrameId::child(&FrameId::root(), 0))
                .is_none()
        );
    }

    #[test]
    fn test_discover_iframes_in_html() {
        let html = Html::parse_document(
            r#"<!DOCTYPE html><html><body>
            <iframe src="https://example.com/widget" id="widget"></iframe>
            <iframe srcdoc="<p>inline</p>"></iframe>
            <div><iframe name="ads" src="https://ads.example.com/banner"></iframe></div>
            <iframe></iframe>
            </body></html>"#,
        );

        let iframe_selector = Selector::parse("iframe, frame").unwrap();
        let count = html.select(&iframe_selector).count();
        assert_eq!(count, 4);
    }
}
