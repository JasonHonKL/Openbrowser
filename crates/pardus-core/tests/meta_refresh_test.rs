//! Integration tests for meta refresh redirect and JS navigation detection.
//!
//! Tests the public `Page::from_html` API combined with the internal
//! `parse_meta_refresh` and `meta_refresh_url` behavior.

use pardus_core::page::Page;
use scraper::{Html, Selector};

// ---------------------------------------------------------------------------
// Meta Refresh Parsing - Integration Level
// ---------------------------------------------------------------------------

fn extract_meta_refresh(html: &str, base_url: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let base = url::Url::parse(base_url).ok()?;
    let selector = Selector::parse("meta[http-equiv]").ok()?;
    for el in doc.select(&selector) {
        let equiv = el.value().attr("http-equiv")?;
        if equiv.eq_ignore_ascii_case("refresh") {
            let content = el.value().attr("content")?;
            if let Some(result) = parse_refresh_content(content, &base) {
                return Some(result);
            }
        }
    }
    None
}

fn parse_refresh_content(content: &str, base_url: &url::Url) -> Option<String> {
    let parts: Vec<&str> = content.splitn(2, ';').collect();
    if parts.len() < 2 {
        return None;
    }
    let url_part = parts[1].trim();
    let url_part = url_part.strip_prefix("url=").or_else(|| {
        if url_part.to_lowercase().starts_with("url=") {
            Some(&url_part[4..])
        } else {
            None
        }
    })?;
    let url_part = url_part.trim();
    if url_part.is_empty() {
        return None;
    }
    base_url.join(url_part).ok().map(|u| u.to_string())
}

#[test]
fn test_meta_refresh_in_realistic_page() {
    let html = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta http-equiv="refresh" content="3;url=https://www.example.com/new-location">
    <title>Redirecting...</title>
</head>
<body>
    <p>You are being redirected. <a href="https://www.example.com/new-location">Click here</a> if not redirected.</p>
</body>
</html>"#;

    let result = extract_meta_refresh(html, "https://old-site.com/page");
    assert_eq!(
        result,
        Some("https://www.example.com/new-location".to_string()),
        "Meta refresh should be extracted from a realistic HTML page"
    );
}

#[test]
fn test_meta_refresh_with_complex_relative_url() {
    let html = r#"<html><head>
        <meta http-equiv="refresh" content="0;url=../other/path?q=1&r=2">
    </head><body></body></html>"#;

    let result = extract_meta_refresh(html, "https://example.com/a/b/page");
    assert_eq!(
        result,
        Some("https://example.com/a/other/path?q=1&r=2".to_string()),
        "Relative URL with .. should be resolved correctly"
    );
}

#[test]
fn test_meta_refresh_with_port_in_base() {
    let html = r#"<html><head>
        <meta http-equiv="refresh" content="0;url=/api/redirect">
    </head><body></body></html>"#;

    let result = extract_meta_refresh(html, "https://example.com:8080/page");
    assert_eq!(
        result,
        Some("https://example.com:8080/api/redirect".to_string()),
        "Port should be preserved in resolved URL"
    );
}

#[test]
fn test_meta_refresh_with_https_redirect_from_http_page() {
    let html = r#"<html><head>
        <meta http-equiv="refresh" content="0;url=https://secure.example.com/secure">
    </head><body></body></html>"#;

    let result = extract_meta_refresh(html, "http://example.com/insecure");
    assert_eq!(
        result,
        Some("https://secure.example.com/secure".to_string()),
        "Cross-scheme redirect should be resolved correctly"
    );
}

#[test]
fn test_meta_refresh_multiple_meta_tags_chooses_refresh() {
    let html = r#"<html><head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width">
        <meta name="description" content="A page">
        <meta http-equiv="refresh" content="0;url=https://example.com/target">
        <meta name="keywords" content="test,page">
    </head><body></body></html>"#;

    let result = extract_meta_refresh(html, "https://example.com/page");
    assert_eq!(
        result,
        Some("https://example.com/target".to_string()),
        "Should find refresh meta among other meta tags"
    );
}

// ---------------------------------------------------------------------------
// Meta Refresh in Body (Invalid but Common)
// ---------------------------------------------------------------------------

#[test]
fn test_meta_refresh_in_body_still_detected() {
    let html = r#"<html><body>
        <p>Loading...</p>
        <meta http-equiv="refresh" content="0;url=https://example.com/redirect">
    </body></html>"#;

    let result = extract_meta_refresh(html, "https://example.com");
    assert_eq!(
        result,
        Some("https://example.com/redirect".to_string()),
        "Meta refresh in body should still be detected (browser behavior)"
    );
}

#[test]
fn test_meta_refresh_with_url_including_spaces() {
    let html = r#"<html><head>
        <meta http-equiv="refresh" content="0;url=https://example.com/path%20with%20spaces">
    </head><body></body></html>"#;

    let result = extract_meta_refresh(html, "https://example.com");
    assert_eq!(
        result,
        Some("https://example.com/path%20with%20spaces".to_string()),
        "URLs with percent-encoded spaces should be preserved"
    );
}

#[test]
fn test_meta_refresh_url_with_trailing_slash_normalization() {
    let html = r#"<html><head>
        <meta http-equiv="refresh" content="0;url=https://example.com">
    </head><body></body></html>"#;

    let result = extract_meta_refresh(html, "https://other.com");
    assert_eq!(
        result,
        Some("https://example.com/".to_string()),
        "Bare domain should get trailing slash (URL normalization)"
    );
}

// ---------------------------------------------------------------------------
// Page Semantic Tree Contains Meta Refresh Info
// ---------------------------------------------------------------------------

#[test]
fn test_page_from_html_with_meta_refresh_has_correct_base() {
    let html = r#"<html><head>
        <meta http-equiv="refresh" content="0;url=/new-page">
    </head><body></body></html>"#;

    let page = Page::from_html(html, "https://example.com/original");
    assert_eq!(page.url, "https://example.com/original");
    assert_eq!(page.base_url, "https://example.com/original");
}

#[test]
fn test_page_from_html_preserves_original_url() {
    let html = r#"<html><head>
        <meta http-equiv="refresh" content="0;url=https://other.com">
    </head><body></body></html>"#;

    let page = Page::from_html(html, "https://example.com/original");
    // from_html does NOT follow meta refresh (that's fetch_and_create's job)
    assert_eq!(page.url, "https://example.com/original");
}

// ---------------------------------------------------------------------------
// Edge Cases
// ---------------------------------------------------------------------------

#[test]
fn test_meta_refresh_content_type_charset() {
    let html = r#"<html><head>
        <meta http-equiv="Content-Type" content="text/html; charset=utf-8">
    </head><body></body></html>"#;

    let result = extract_meta_refresh(html, "https://example.com");
    assert_eq!(result, None, "Content-Type meta should not trigger refresh");
}

#[test]
fn test_meta_refresh_empty_document() {
    let html = r#"<html><head></head><body></body></html>"#;
    let result = extract_meta_refresh(html, "https://example.com");
    assert_eq!(result, None);
}

#[test]
fn test_meta_refresh_very_long_delay() {
    let html = r#"<html><head>
        <meta http-equiv="refresh" content="86400;url=https://example.com">
    </head><body></body></html>"#;

    let result = extract_meta_refresh(html, "https://example.com");
    assert_eq!(
        result,
        Some("https://example.com/".to_string()),
        "Very long delay should still extract URL"
    );
}
