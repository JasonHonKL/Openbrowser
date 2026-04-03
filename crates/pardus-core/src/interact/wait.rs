use std::sync::Arc;
use std::time::{Duration, Instant};

use scraper::{Html, Selector};
use crate::app::App;
use crate::page::Page;
use super::actions::InteractionResult;

pub async fn wait_for_selector(
    app: &Arc<App>,
    page: &Page,
    selector: &str,
    timeout_ms: u32,
    interval_ms: u32,
) -> anyhow::Result<InteractionResult> {
    if page.has_selector(selector) {
        return Ok(InteractionResult::WaitSatisfied {
            selector: selector.to_string(),
            found: true,
        });
    }

    let timeout = Duration::from_millis(timeout_ms as u64);
    let interval = Duration::from_millis(interval_ms as u64);
    let start = Instant::now();

    while start.elapsed() < timeout {
        tokio::time::sleep(interval).await;

        match Page::from_url(app, &page.url).await {
            Ok(new_page) => {
                if new_page.has_selector(selector) {
                    return Ok(InteractionResult::WaitSatisfied {
                        selector: selector.to_string(),
                        found: true,
                    });
                }
            }
            Err(_) => continue,
        }
    }

    Ok(InteractionResult::WaitSatisfied {
        selector: selector.to_string(),
        found: false,
    })
}

pub async fn wait_for_selector_with_js(
    app: &Arc<App>,
    page: &Page,
    selector: &str,
    timeout_ms: u32,
    interval_ms: u32,
    js_wait_ms: u32,
) -> anyhow::Result<InteractionResult> {
    if page.has_selector(selector) {
        return Ok(InteractionResult::WaitSatisfied {
            selector: selector.to_string(),
            found: true,
        });
    }

    let timeout = Duration::from_millis(timeout_ms as u64);
    let interval = Duration::from_millis(interval_ms as u64);
    let start = Instant::now();

    while start.elapsed() < timeout {
        tokio::time::sleep(interval).await;

        match Page::from_url_with_js(app, &page.url, js_wait_ms).await {
            Ok(new_page) => {
                if new_page.has_selector(selector) {
                    return Ok(InteractionResult::WaitSatisfied {
                        selector: selector.to_string(),
                        found: true,
                    });
                }
            }
            Err(_) => continue,
        }
    }

    Ok(InteractionResult::WaitSatisfied {
        selector: selector.to_string(),
        found: false,
    })
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum WaitCondition {
    Selector(String),
    ContentLoaded,
    ContentStable,
    NetworkIdle,
    MinInteractiveElements(usize),
    Custom(String),
}

/// Smart wait: wait until content appears "loaded" based on multiple heuristics.
///
/// Checks:
/// 1. No loading indicators visible (spinners, skeletons, loading text).
/// 2. Body has substantial content (min character count).
/// 3. Page HTML is stable across consecutive polls (content fingerprint match).
///
/// Returns `WaitSatisfied { found: true }` when the page appears loaded,
/// or `found: false` on timeout.
pub async fn wait_for_loaded(
    app: &Arc<App>,
    page: &Page,
    timeout_ms: u32,
    interval_ms: u32,
) -> anyhow::Result<InteractionResult> {
    let initial_html_len = page.html.html().len();
    let initial_body_text = extract_body_text(&page.html);
    let initial_text_len = initial_body_text.len();

    if !has_loading_indicators(&page.html) && initial_text_len > 200 {
        return Ok(InteractionResult::WaitSatisfied {
            selector: "content-loaded".to_string(),
            found: true,
        });
    }

    let timeout = Duration::from_millis(timeout_ms as u64);
    let interval = Duration::from_millis(interval_ms as u64);
    let start = Instant::now();
    let mut last_fingerprint = content_fingerprint(&page.html);
    let mut stable_ticks: u32 = 0;
    let required_stable_ticks = 3u32;

    while start.elapsed() < timeout {
        tokio::time::sleep(interval).await;

        match Page::from_url(app, &page.url).await {
            Ok(new_page) => {
                let html = &new_page.html;
                let body_text = extract_body_text(html);
                let current_fingerprint = content_fingerprint(html);

                let has_loaders = has_loading_indicators(html);
                let content_grew = body_text.len() > initial_text_len * 2;
                let html_shrunk = html.html().len() < initial_html_len / 2;

                if !has_loaders && (content_grew || body_text.len() > 500) {
                    return Ok(InteractionResult::WaitSatisfied {
                        selector: "content-loaded".to_string(),
                        found: true,
                    });
                }

                if !html_shrunk && current_fingerprint == last_fingerprint {
                    stable_ticks += 1;
                    if stable_ticks >= required_stable_ticks && body_text.len() > 200 {
                        return Ok(InteractionResult::WaitSatisfied {
                            selector: "content-stable".to_string(),
                            found: true,
                        });
                    }
                } else {
                    stable_ticks = 0;
                }

                last_fingerprint = current_fingerprint;
            }
            Err(_) => continue,
        }
    }

    Ok(InteractionResult::WaitSatisfied {
        selector: "content-loaded".to_string(),
        found: false,
    })
}

/// Wait until the page content stabilizes (no DOM changes between polls).
///
/// Useful for SPAs that progressively render content.
/// Considers the page "stable" when the content fingerprint matches for
/// `required_stable` consecutive polls.
pub async fn wait_for_stable(
    app: &Arc<App>,
    page: &Page,
    timeout_ms: u32,
    interval_ms: u32,
    required_stable: u32,
) -> anyhow::Result<InteractionResult> {
    let timeout = Duration::from_millis(timeout_ms as u64);
    let interval = Duration::from_millis(interval_ms as u64);
    let start = Instant::now();

    let mut last_fp = content_fingerprint(&page.html);
    let mut stable_ticks: u32 = 1;

    while start.elapsed() < timeout {
        tokio::time::sleep(interval).await;

        match Page::from_url(app, &page.url).await {
            Ok(new_page) => {
                let current_fp = content_fingerprint(&new_page.html);
                if current_fp == last_fp {
                    stable_ticks += 1;
                    if stable_ticks >= required_stable {
                        return Ok(InteractionResult::WaitSatisfied {
                            selector: "content-stable".to_string(),
                            found: true,
                        });
                    }
                } else {
                    stable_ticks = 1;
                    last_fp = current_fp;
                }
            }
            Err(_) => continue,
        }
    }

    Ok(InteractionResult::WaitSatisfied {
        selector: "content-stable".to_string(),
        found: false,
    })
}

/// Wait until the page has at least `min_count` interactive elements.
///
/// Useful for pages that load forms or buttons dynamically.
pub async fn wait_for_interactive(
    app: &Arc<App>,
    page: &Page,
    min_count: usize,
    timeout_ms: u32,
    interval_ms: u32,
) -> anyhow::Result<InteractionResult> {
    let tree = page.semantic_tree();
    if tree.stats.actions >= min_count {
        return Ok(InteractionResult::WaitSatisfied {
            selector: "interactive-elements".to_string(),
            found: true,
        });
    }

    let timeout = Duration::from_millis(timeout_ms as u64);
    let interval = Duration::from_millis(interval_ms as u64);
    let start = Instant::now();

    while start.elapsed() < timeout {
        tokio::time::sleep(interval).await;

        match Page::from_url(app, &page.url).await {
            Ok(new_page) => {
                let tree = new_page.semantic_tree();
                if tree.stats.actions >= min_count {
                    return Ok(InteractionResult::WaitSatisfied {
                        selector: "interactive-elements".to_string(),
                        found: true,
                    });
                }
            }
            Err(_) => continue,
        }
    }

    Ok(InteractionResult::WaitSatisfied {
        selector: "interactive-elements".to_string(),
        found: false,
    })
}

/// Wait using a smart condition enum. Dispatches to the appropriate strategy.
pub async fn wait_smart(
    app: &Arc<App>,
    page: &Page,
    condition: &WaitCondition,
    timeout_ms: u32,
    interval_ms: u32,
) -> anyhow::Result<InteractionResult> {
    match condition {
        WaitCondition::Selector(sel) => {
            wait_for_selector(app, page, sel, timeout_ms, interval_ms).await
        }
        WaitCondition::ContentLoaded => {
            wait_for_loaded(app, page, timeout_ms, interval_ms).await
        }
        WaitCondition::ContentStable => {
            wait_for_stable(app, page, timeout_ms, interval_ms, 3).await
        }
        WaitCondition::NetworkIdle => {
            wait_for_stable(app, page, timeout_ms, interval_ms, 5).await
        }
        WaitCondition::MinInteractiveElements(min) => {
            wait_for_interactive(app, page, *min, timeout_ms, interval_ms).await
        }
        WaitCondition::Custom(sel) => {
            wait_for_selector(app, page, sel, timeout_ms, interval_ms).await
        }
    }
}

fn extract_body_text(html: &Html) -> String {
    if let Ok(sel) = Selector::parse("body") {
        if let Some(body) = html.select(&sel).next() {
            return body.text().collect::<String>();
        }
    }
    let mut text = String::new();
    for node in html.tree.nodes() {
        if let Some(t) = node.value().as_text() {
            text.push_str(t);
        }
    }
    text
}

fn has_loading_indicators(html: &Html) -> bool {
    let mut text = String::new();
    for node in html.tree.nodes() {
        if let Some(t) = node.value().as_text() {
            text.push_str(t);
        }
    }
    let text = text.to_lowercase();
    let indicators = [
        "loading",
        "please wait",
        "spinner",
        "skeleton",
        "fetching",
        "retrieving data",
        "processing",
    ];

    let mut count = 0;
    for indicator in &indicators {
        if text.contains(indicator) {
            count += 1;
        }
        if count >= 2 {
            return true;
        }
    }

    if let Ok(sel) = Selector::parse("[class*='loading'], [class*='spinner'], [class*='skeleton'], [role='progressbar'], [aria-busy='true']") {
        if html.select(&sel).next().is_some() {
            return true;
        }
    }

    false
}

/// Fast content fingerprint using blake3 hash of significant DOM text nodes.
/// Skips script/style content for stability.
fn content_fingerprint(html: &Html) -> u64 {
    use blake3::Hasher;

    let mut hasher = Hasher::new();

    if let Ok(body_sel) = Selector::parse("body") {
        if let Some(body) = html.select(&body_sel).next() {
            for node in body.descendants() {
                // Only hash raw text nodes, not element nodes
                if let Some(text) = node.value().as_text() {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        // Check if this text node is inside a script/style/noscript element
                        let mut inside_skip = false;
                        for ancestor in node.ancestors() {
                            if let Some(el) = scraper::ElementRef::wrap(ancestor) {
                                let tag = el.value().name();
                                if matches!(tag, "script" | "style" | "noscript") {
                                    inside_skip = true;
                                    break;
                                }
                            }
                        }
                        if !inside_skip {
                            hasher.update(trimmed.as_bytes());
                        }
                    }
                }
            }
        }
    }

    let hash = hasher.finalize();
    u64::from_le_bytes(hash.as_bytes()[..8].try_into().unwrap_or([0u8; 8]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_loading_indicators_text() {
        let html = Html::parse_document(
            r#"<html><body><div class="spinner">Loading content, please wait...</div></body></html>"#
        );
        assert!(has_loading_indicators(&html));
    }

    #[test]
    fn test_no_loading_indicators() {
        let html = Html::parse_document(
            r#"<html><body><h1>Welcome</h1><p>This is real content.</p></body></html>"#
        );
        assert!(!has_loading_indicators(&html));
    }

    #[test]
    fn test_single_loading_word_no_indicator() {
        let html = Html::parse_document(
            r#"<html><body><p>Processing your request is important to us.</p></body></html>"#
        );
        assert!(!has_loading_indicators(&html));
    }

    #[test]
    fn test_loading_indicator_aria_busy() {
        let html = Html::parse_document(
            r#"<html><body><div aria-busy="true">Loading...</div></body></html>"#
        );
        assert!(has_loading_indicators(&html));
    }

    #[test]
    fn test_loading_indicator_class() {
        let html = Html::parse_document(
            r#"<html><body><div class="loading-spinner">Working...</div></body></html>"#
        );
        assert!(has_loading_indicators(&html));
    }

    #[test]
    fn test_loading_indicator_progressbar_role() {
        let html = Html::parse_document(
            r#"<html><body><div role="progressbar">Loading...</div></body></html>"#
        );
        assert!(has_loading_indicators(&html));
    }

    #[test]
    fn test_loading_indicator_skeleton_class() {
        let html = Html::parse_document(
            r#"<html><body><div class="skeleton-card">...</div></body></html>"#
        );
        assert!(has_loading_indicators(&html));
    }

    #[test]
    fn test_content_fingerprint_stable() {
        let html1 = Html::parse_document(
            "<html><body><h1>Test</h1><p>Hello world</p></body></html>"
        );
        let html2 = Html::parse_document(
            "<html><body><h1>Test</h1><p>Hello world</p></body></html>"
        );
        let fp1 = content_fingerprint(&html1);
        let fp2 = content_fingerprint(&html2);
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn test_content_fingerprint_differs() {
        let html1 = Html::parse_document(
            "<html><body><h1>Test</h1><p>Hello world</p></body></html>"
        );
        let html2 = Html::parse_document(
            "<html><body><h1>Test</h1><p>Different content</p></body></html>"
        );
        let fp1 = content_fingerprint(&html1);
        let fp2 = content_fingerprint(&html2);
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn test_content_fingerprint_ignores_script() {
        let html1 = Html::parse_document(
            r#"<html><body><h1>Test</h1><script>var x = 1;</script><p>Hello</p></body></html>"#
        );
        let html2 = Html::parse_document(
            r#"<html><body><h1>Test</h1><script>var x = 99999;</script><p>Hello</p></body></html>"#
        );
        let fp1 = content_fingerprint(&html1);
        let fp2 = content_fingerprint(&html2);
        assert_eq!(fp1, fp2, "fingerprint should ignore script content differences");
    }

    #[test]
    fn test_content_fingerprint_ignores_style() {
        let html1 = Html::parse_document(
            r#"<html><body><h1>Test</h1><style>.red { color: red; }</style><p>Hello</p></body></html>"#
        );
        let html2 = Html::parse_document(
            r#"<html><body><h1>Test</h1><style>.blue { color: blue; }</style><p>Hello</p></body></html>"#
        );
        let fp1 = content_fingerprint(&html1);
        let fp2 = content_fingerprint(&html2);
        assert_eq!(fp1, fp2, "fingerprint should ignore style content differences");
    }

    #[test]
    fn test_extract_body_text() {
        let html = Html::parse_document(
            "<html><head><title>Ignore</title></head><body><p>Hello</p></body></html>"
        );
        let text = extract_body_text(&html);
        assert!(text.contains("Hello"));
        assert!(!text.contains("Ignore"));
    }

    #[test]
    fn test_extract_body_text_empty() {
        let html = Html::parse_document("<html><body></body></html>");
        let text = extract_body_text(&html);
        assert!(text.trim().is_empty());
    }

    #[test]
    fn test_wait_condition_equality() {
        assert_eq!(WaitCondition::Selector("#foo".to_string()), WaitCondition::Selector("#foo".to_string()));
        assert_eq!(WaitCondition::ContentLoaded, WaitCondition::ContentLoaded);
        assert_ne!(WaitCondition::ContentLoaded, WaitCondition::ContentStable);
        assert_eq!(WaitCondition::MinInteractiveElements(5), WaitCondition::MinInteractiveElements(5));
        assert_ne!(WaitCondition::MinInteractiveElements(3), WaitCondition::MinInteractiveElements(5));
    }

    #[test]
    fn test_wait_condition_custom_equals_selector() {
        let c = WaitCondition::Custom("div.loaded".to_string());
        assert!(matches!(c, WaitCondition::Custom(_)));
    }
}
