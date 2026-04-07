use std::sync::Arc;

use url::Url;

use super::actions::InteractionResult;
use crate::{app::App, page::Page};

/// Scroll direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollDirection {
    Up,
    Down,
    ToTop,
    ToBottom,
}

/// Simulate scroll by detecting URL-based pagination patterns.
///
/// Many infinite-scroll pages use query parameters like `?page=2` or `?offset=20`.
/// This function detects these patterns and fetches the next page.
///
/// Limitation: Without JS execution, AJAX-loaded content cannot be detected.
pub async fn scroll(
    app: &Arc<App>,
    page: &Page,
    direction: ScrollDirection,
) -> anyhow::Result<InteractionResult> {
    let next_url = detect_next_page_url(&page.url, direction);

    match next_url {
        Some(url) => {
            let new_page = Page::from_url(app, &url).await?;
            Ok(InteractionResult::Scrolled {
                url,
                page: new_page,
            })
        }
        None => Ok(InteractionResult::ElementNotFound {
            selector: String::new(),
            reason: "no pagination pattern detected in URL. Try enabling JS execution for \
                     AJAX-based infinite scroll."
                .to_string(),
        }),
    }
}

/// Detect pagination patterns in the URL and compute the next/previous page URL.
fn detect_next_page_url(current_url: &str, direction: ScrollDirection) -> Option<String> {
    let mut url = Url::parse(current_url).ok()?;

    // Strategy 1: ?page=N
    let page_value = url
        .query_pairs()
        .find(|(k, _)| k == "page")
        .map(|(_, v)| v.to_string());
    if let Some(page_str) = page_value {
        if let Ok(page_num) = page_str.parse::<u32>() {
            let next_page = match direction {
                ScrollDirection::Down | ScrollDirection::ToBottom => page_num + 1,
                ScrollDirection::Up | ScrollDirection::ToTop => page_num.saturating_sub(1),
            };
            if next_page == 0 {
                return None;
            }
            set_query_param(&mut url, "page", &next_page.to_string());
            return Some(url.to_string());
        }
    }

    // Strategy 2: ?offset=N (assume page size of 20)
    let offset_value = url
        .query_pairs()
        .find(|(k, _)| k == "offset")
        .map(|(_, v)| v.to_string());
    if let Some(offset_str) = offset_value {
        if let Ok(offset) = offset_str.parse::<u32>() {
            let page_size = detect_page_size(&url).unwrap_or(20);
            let next_offset = match direction {
                ScrollDirection::Down | ScrollDirection::ToBottom => offset + page_size,
                ScrollDirection::Up | ScrollDirection::ToTop => offset.saturating_sub(page_size),
            };
            set_query_param(&mut url, "offset", &next_offset.to_string());
            return Some(url.to_string());
        }
    }

    // Strategy 3: ?start=N (common in search results)
    let start_value = url
        .query_pairs()
        .find(|(k, _)| k == "start")
        .map(|(_, v)| v.to_string());
    if let Some(start_str) = start_value {
        if let Ok(start) = start_str.parse::<u32>() {
            let step = detect_page_size(&url).unwrap_or(10);
            let next_start = match direction {
                ScrollDirection::Down | ScrollDirection::ToBottom => start + step,
                ScrollDirection::Up | ScrollDirection::ToTop => start.saturating_sub(step),
            };
            set_query_param(&mut url, "start", &next_start.to_string());
            return Some(url.to_string());
        }
    }

    // Strategy 4: Path-based pagination (/page/2, /p/2)
    let path = url.path().to_string();
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    for i in (1..segments.len()).rev() {
        let prev = segments[i - 1].to_lowercase();
        if (prev == "page" || prev == "p") && segments[i].parse::<u32>().is_ok() {
            if let Ok(page_num) = segments[i].parse::<u32>() {
                let next_page = match direction {
                    ScrollDirection::Down | ScrollDirection::ToBottom => page_num + 1,
                    ScrollDirection::Up | ScrollDirection::ToTop => page_num.saturating_sub(1),
                };
                if next_page == 0 {
                    return None;
                }
                let mut owned_segments: Vec<String> =
                    segments.iter().map(|s| s.to_string()).collect();
                owned_segments[i] = next_page.to_string();
                let new_path = format!("/{}", owned_segments.join("/"));
                url.set_path(&new_path);
                return Some(url.to_string());
            }
        }
    }

    None
}

fn detect_page_size(url: &Url) -> Option<u32> {
    url.query_pairs()
        .find(|(k, _)| k == "limit" || k == "per_page" || k == "count" || k == "size")
        .and_then(|(_, v)| v.parse::<u32>().ok())
}

fn set_query_param(url: &mut Url, key: &str, value: &str) {
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    if let Some(pair) = pairs.iter_mut().find(|(k, _)| k == key) {
        pair.1 = value.to_string();
    } else {
        pairs.push((key.to_string(), value.to_string()));
    }

    {
        let mut query = url.query_pairs_mut();
        query.clear();
        for (k, v) in &pairs {
            query.append_pair(k, v);
        }
    }
}
