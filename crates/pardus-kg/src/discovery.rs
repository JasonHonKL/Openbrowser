use scraper::{Html, Selector};
use std::sync::LazyLock as Lazy;

use pardus_core::NavigationGraph;

use crate::state::ViewStateId;
use crate::transition::Trigger;

/// Result of discovering transitions from a single page.
pub struct DiscoveredTransition {
    /// Target URL to enqueue.
    pub target_url: String,
    /// The trigger that causes this transition.
    pub trigger: Trigger,
}

/// Discover all link-click transitions from a navigation graph.
pub fn discover_link_transitions(
    nav_graph: &NavigationGraph,
    _root_origin: &str,
    _parent_id: &ViewStateId,
) -> Vec<DiscoveredTransition> {
    nav_graph
        .internal_links
        .iter()
        .map(|route| {
            let selector = format!("a[href=\"{}\"]", route.url);
            DiscoveredTransition {
                target_url: route.url.clone(),
                trigger: Trigger::LinkClick {
                    url: route.url.clone(),
                    label: route.label.clone(),
                    selector: Some(selector),
                },
            }
        })
        .collect()
}

/// Discover hash navigation transitions from page HTML.
/// NavigationGraph skips href="#" links, so we need our own selector.
pub fn discover_hash_transitions(html: &Html, page_url: &str) -> Vec<DiscoveredTransition> {
    static HASH_LINK: Lazy<Selector> =
        Lazy::new(|| Selector::parse("a[href^='#']").expect("valid selector"));

    let mut results = Vec::new();
    for el in html.select(&HASH_LINK) {
        let href = el.value().attr("href").unwrap_or("");
        if href == "#" || href == "#!" {
            continue;
        }
        let fragment = href.trim_start_matches('#').to_string();
        if fragment.is_empty() {
            continue;
        }
        let label: String = el.text().collect::<Vec<_>>().join(" ").trim().to_string();
        // For hash nav, the target URL is the same page with the fragment
        let target_url = if page_url.contains('#') {
            format!(
                "{}#{}",
                page_url.split('#').next().unwrap_or(page_url),
                fragment
            )
        } else {
            format!("{}#{}", page_url, fragment)
        };

        results.push(DiscoveredTransition {
            target_url,
            trigger: Trigger::HashNavigation {
                fragment,
                label: if label.is_empty() { None } else { Some(label) },
            },
        });
    }
    results
}

/// Discover pagination transitions by detecting URL patterns.
/// Generalized from pardus-core interact::scroll logic.
pub fn discover_pagination_transitions(page_url: &str) -> Vec<DiscoveredTransition> {
    let mut results = Vec::new();

    let Some(mut url) = url::Url::parse(page_url).ok() else {
        return results;
    };

    // Strategy 1: ?page=N
    if let Some(page_str) = url
        .query_pairs()
        .find(|(k, _)| k == "page")
        .map(|(_, v)| v.to_string())
    {
        if let Ok(page_num) = page_str.parse::<u32>() {
            if page_num > 0 {
                let next_page = page_num + 1;
                let next_url = set_query_param_val(&url, "page", &next_page.to_string());
                results.push(DiscoveredTransition {
                    target_url: next_url.clone(),
                    trigger: Trigger::Pagination {
                        from_url: page_url.to_string(),
                        to_url: next_url,
                    },
                });
            }
        }
    }

    // Strategy 2: ?offset=N
    if let Some(offset_str) = url
        .query_pairs()
        .find(|(k, _)| k == "offset")
        .map(|(_, v)| v.to_string())
    {
        if let Ok(offset) = offset_str.parse::<u32>() {
            let page_size = detect_page_size(&url).unwrap_or(20);
            let next_offset = offset + page_size;
            let next_url = set_query_param_val(&url, "offset", &next_offset.to_string());
            results.push(DiscoveredTransition {
                target_url: next_url.clone(),
                trigger: Trigger::Pagination {
                    from_url: page_url.to_string(),
                    to_url: next_url,
                },
            });
        }
    }

    // Strategy 3: ?start=N
    if let Some(start_str) = url
        .query_pairs()
        .find(|(k, _)| k == "start")
        .map(|(_, v)| v.to_string())
    {
        if let Ok(start) = start_str.parse::<u32>() {
            let step = detect_page_size(&url).unwrap_or(10);
            let next_start = start + step;
            let next_url = set_query_param_val(&url, "start", &next_start.to_string());
            results.push(DiscoveredTransition {
                target_url: next_url.clone(),
                trigger: Trigger::Pagination {
                    from_url: page_url.to_string(),
                    to_url: next_url,
                },
            });
        }
    }

    // Strategy 4: Path-based /page/N
    let path = url.path().to_string();
    let segments: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    for i in (1..segments.len()).rev() {
        let prev = segments[i - 1].to_lowercase();
        if (prev == "page" || prev == "p") && segments[i].parse::<u32>().is_ok() {
            if let Ok(page_num) = segments[i].parse::<u32>() {
                let next_page = page_num + 1;
                let mut new_segments = segments.clone();
                new_segments[i] = next_page.to_string();
                let new_path = format!("/{}", new_segments.join("/"));
                url.set_path(&new_path);
                results.push(DiscoveredTransition {
                    target_url: url.to_string(),
                    trigger: Trigger::Pagination {
                        from_url: page_url.to_string(),
                        to_url: url.to_string(),
                    },
                });
                break;
            }
        }
    }

    results
}

/// Detect page size from query params.
fn detect_page_size(url: &url::Url) -> Option<u32> {
    url.query_pairs()
        .find(|(k, _)| matches!(k.as_ref(), "limit" | "per_page" | "count" | "size"))
        .and_then(|(_, v)| v.parse::<u32>().ok())
}

/// Set a query parameter value on a URL, returning the new URL string.
fn set_query_param_val(url: &url::Url, key: &str, value: &str) -> String {
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    if let Some(pair) = pairs.iter_mut().find(|(k, _)| k == key) {
        pair.1 = value.to_string();
    } else {
        pairs.push((key.to_string(), value.to_string()));
    }

    let mut url = url.clone();
    {
        let mut query = url.query_pairs_mut();
        query.clear();
        for (k, v) in &pairs {
            query.append_pair(k, v);
        }
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_page_param() {
        let results = discover_pagination_transitions("https://example.com/blog?page=1");
        assert_eq!(results.len(), 1);
        assert!(results[0].target_url.contains("page=2"));
    }

    #[test]
    fn test_pagination_offset_param() {
        let results = discover_pagination_transitions("https://example.com/list?offset=0");
        assert_eq!(results.len(), 1);
        assert!(results[0].target_url.contains("offset=20"));
    }

    #[test]
    fn test_pagination_path_based() {
        let results = discover_pagination_transitions("https://example.com/blog/page/1");
        assert_eq!(results.len(), 1);
        assert!(results[0].target_url.contains("/page/2"));
    }

    #[test]
    fn test_no_pagination_on_plain_url() {
        let results = discover_pagination_transitions("https://example.com/about");
        assert!(results.is_empty());
    }

    #[test]
    fn test_hash_navigation() {
        let html_content = r##"
            <html><body>
                <a href="#features">Features</a>
                <a href="#pricing">Pricing</a>
                <a href="#">Skip</a>
            </body></html>
        "##;
        let html = Html::parse_document(html_content);
        let results = discover_hash_transitions(&html, "https://example.com/");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].target_url, "https://example.com/#features");
        assert_eq!(results[1].target_url, "https://example.com/#pricing");
    }

    #[test]
    fn test_hash_navigation_empty_fragment_skipped() {
        let html_content = r##"
            <html><body>
                <a href="#">Top</a>
                <a href="#!">Bang</a>
            </body></html>
        "##;
        let html = Html::parse_document(html_content);
        let results = discover_hash_transitions(&html, "https://example.com/");
        assert!(results.is_empty());
    }
}
