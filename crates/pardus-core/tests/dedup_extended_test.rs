//! Extended tests for request deduplication edge cases.

use std::sync::Arc;
use pardus_core::dedup::{RequestDedup, DedupEntry, DedupResult, dedup_key};

// ---------------------------------------------------------------------------
// dedup_key normalization
// ---------------------------------------------------------------------------

#[test]
fn dedup_key_normalizes_scheme_case() {
    // URL parsing lowercases scheme, so HTTPS:// -> https://
    let k1 = dedup_key("HTTPS://EXAMPLE.COM/page");
    let k2 = dedup_key("https://example.com/page");
    assert_eq!(k1, k2);
}

#[test]
fn dedup_key_preserves_path() {
    let k1 = dedup_key("https://example.com/a/b/c");
    let k2 = dedup_key("https://example.com/a/b/d");
    assert_ne!(k1, k2);
}

#[test]
fn dedup_key_root_path() {
    let k = dedup_key("https://example.com/");
    assert!(k.ends_with('/'));
}

#[test]
fn dedup_key_no_trailing_slash_bare_domain() {
    let k = dedup_key("https://example.com");
    // Should normalize — URL parsing adds /
    assert!(k.contains("example.com"));
}

#[test]
fn dedup_key_strips_fragment_with_content() {
    let k1 = dedup_key("https://example.com/page#section-1");
    let k2 = dedup_key("https://example.com/page#section-2");
    assert_eq!(k1, k2);
}

#[test]
fn dedup_key_sorted_query_params() {
    let k1 = dedup_key("https://api.example.com/search?q=rust&page=1");
    let k2 = dedup_key("https://api.example.com/search?page=1&q=rust");
    assert_eq!(k1, k2);
}

#[test]
fn dedup_key_different_query_values() {
    let k1 = dedup_key("https://example.com/api?q=foo");
    let k2 = dedup_key("https://example.com/api?q=bar");
    assert_ne!(k1, k2);
}

#[test]
fn dedup_key_port_preserved() {
    let k1 = dedup_key("https://example.com:8080/path");
    let k2 = dedup_key("https://example.com/path");
    assert_ne!(k1, k2);
}

#[test]
fn dedup_key_invalid_url_passthrough() {
    let k = dedup_key("not a url");
    assert_eq!(k, "not a url");
}

#[test]
fn dedup_key_empty_query_vs_no_query() {
    let k1 = dedup_key("https://example.com/path?");
    let k2 = dedup_key("https://example.com/path");
    // Empty query string vs no query may differ
    // Just ensure both don't panic
    assert!(!k1.is_empty());
    assert!(!k2.is_empty());
}

#[test]
fn dedup_key_encoded_params() {
    let k1 = dedup_key("https://example.com/search?q=hello%20world");
    let k2 = dedup_key("https://example.com/search?q=hello+world");
    // Both should normalize the same way
    assert_eq!(k1, k2);
}

// ---------------------------------------------------------------------------
// RequestDedup concurrent flow
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dedup_multiple_proceed_without_complete() {
    let dedup = Arc::new(RequestDedup::new(5000));

    // First request proceeds
    let r1 = dedup.enter("https://example.com/page1").await;
    assert!(matches!(r1, DedupEntry::Proceed));

    // Second URL proceeds (different key)
    let r2 = dedup.enter("https://example.com/page2").await;
    assert!(matches!(r2, DedupEntry::Proceed));
}

#[tokio::test]
async fn dedup_complete_then_remove_allows_proceed() {
    let dedup = RequestDedup::new(5000);
    dedup.enter("https://example.com/page").await;

    dedup.complete("https://example.com/page", DedupResult {
        url: "https://example.com/page".into(),
        status: 200,
        body: b"ok".to_vec(),
        content_type: Some("text/html".into()),
        headers: vec![],
        http_version: "HTTP/1.1".into(),
    });

    // Should be cached
    let cached = dedup.enter("https://example.com/page").await;
    assert!(matches!(cached, DedupEntry::Cached(_)));

    // Remove
    dedup.remove("https://example.com/page");

    // Should proceed again
    let after = dedup.enter("https://example.com/page").await;
    assert!(matches!(after, DedupEntry::Proceed));
}

#[tokio::test]
async fn dedup_get_completed_returns_none_for_unknown() {
    let dedup = RequestDedup::new(5000);
    assert!(dedup.get_completed("https://example.com/nonexistent").is_none());
}

#[tokio::test]
async fn dedup_overwrite_completed_result() {
    let dedup = RequestDedup::new(5000);
    dedup.enter("https://example.com/api").await;

    dedup.complete("https://example.com/api", DedupResult {
        url: "https://example.com/api".into(),
        status: 200,
        body: b"first".to_vec(),
        content_type: None,
        headers: vec![],
        http_version: "HTTP/1.1".into(),
    });

    // Overwrite with new result
    dedup.complete("https://example.com/api", DedupResult {
        url: "https://example.com/api".into(),
        status: 200,
        body: b"second".to_vec(),
        content_type: None,
        headers: vec![],
        http_version: "HTTP/1.1".into(),
    });

    let result = dedup.get_completed("https://example.com/api").unwrap();
    assert_eq!(result.body, b"second");
}

#[tokio::test]
async fn dedup_window_expiry() {
    let dedup = RequestDedup::new(50); // 50ms window
    dedup.enter("https://example.com/page").await;

    dedup.complete("https://example.com/page", DedupResult {
        url: "https://example.com/page".into(),
        status: 200,
        body: b"ok".to_vec(),
        content_type: None,
        headers: vec![],
        http_version: "HTTP/1.1".into(),
    });

    // Should be cached immediately
    let cached = dedup.enter("https://example.com/page").await;
    assert!(matches!(cached, DedupEntry::Cached(_)));

    // Wait for window to expire
    tokio::time::sleep(std::time::Duration::from_millis(60)).await;

    // Should proceed now (window expired)
    let expired = dedup.enter("https://example.com/page").await;
    assert!(matches!(expired, DedupEntry::Proceed));
}

#[tokio::test]
async fn dedup_disabled_always_proceeds() {
    let dedup = RequestDedup::new(0);
    assert!(!dedup.is_enabled());

    // Even after "complete", should not cache because window is 0
    dedup.enter("https://example.com/page").await;
    dedup.complete("https://example.com/page", DedupResult {
        url: "https://example.com/page".into(),
        status: 200,
        body: b"ok".to_vec(),
        content_type: None,
        headers: vec![],
        http_version: "HTTP/1.1".into(),
    });

    // The complete still registers, but the window check (0 < 0) fails
    // Actually, with window 0, enter checks elapsed < window_ms, which is 0 < 0 = false
    // So it will proceed to register as in-flight
    let result = dedup.enter("https://example.com/page").await;
    // The entry exists as Completed but elapsed >= window_ms, so it falls through
    // and registers as in-flight, returning Proceed
    assert!(matches!(result, DedupEntry::Proceed));
}
