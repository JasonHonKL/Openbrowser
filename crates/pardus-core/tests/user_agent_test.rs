//! Tests for the user-agent fix in build_http_client.
//!
//! Before the fix, `.default_headers()` was called AFTER `.user_agent()`,
//! but since rquest's `default_headers()` uses `std::mem::swap` (replacing all
//! headers), the User-Agent was silently lost. The fix reorders the calls so
//! `.user_agent()` is applied last.

use pardus_core::BrowserConfig;

/// The default User-Agent string set in BrowserConfig.
const EXPECTED_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

// ---------------------------------------------------------------------------
// Unit tests (no network)
// ---------------------------------------------------------------------------

#[test]
fn test_config_default_user_agent_is_chrome() {
    let config = BrowserConfig::default();
    let ua = config.effective_user_agent();
    assert!(
        ua.contains("Chrome"),
        "default user-agent should contain 'Chrome', got: {ua}"
    );
    assert!(
        ua.contains("Mozilla/5.0"),
        "default user-agent should start with 'Mozilla/5.0', got: {ua}"
    );
}

#[test]
fn test_config_custom_user_agent_is_preserved() {
    let custom_ua = "TestBot/1.0";
    let mut config = BrowserConfig::default();
    config.user_agent = custom_ua.to_string();
    assert_eq!(
        config.effective_user_agent(),
        custom_ua,
        "custom user-agent should be returned by effective_user_agent()"
    );
}

// ---------------------------------------------------------------------------
// Integration tests (require network)
// ---------------------------------------------------------------------------

/// Verify that the HTTP client built by `build_http_client` actually sends
/// the User-Agent header. Uses httpbin.org/headers which echoes request
/// headers back as JSON.
///
/// This is the core regression test for the bug where `.default_headers()`
/// was wiping the User-Agent.
#[tokio::test]
async fn test_http_client_sends_user_agent() {
    let config = BrowserConfig::default();
    let client = pardus_core::app::build_http_client(&config)
        .expect("build_http_client should succeed");

    let resp = client
        .get("https://httpbin.org/headers")
        .send()
        .await
        .expect("request to httpbin should succeed");

    let body: serde_json::Value = resp
        .json()
        .await
        .expect("response should be valid JSON");

    let headers = body
        .get("headers")
        .expect("response should contain 'headers' object");

    let ua = headers
        .get("User-Agent")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    assert!(
        !ua.is_empty(),
        "User-Agent header must not be empty — the build_http_client fix may have regressed"
    );
    assert_eq!(
        ua, EXPECTED_UA,
        "User-Agent should match the default BrowserConfig value"
    );
}

/// Verify that a custom user-agent is sent when configured.
#[tokio::test]
async fn test_http_client_sends_custom_user_agent() {
    let custom_ua = "PardusTestBot/2.0 (Integration Test)";
    let mut config = BrowserConfig::default();
    config.user_agent = custom_ua.to_string();

    let client = pardus_core::app::build_http_client(&config)
        .expect("build_http_client should succeed");

    let resp = client
        .get("https://httpbin.org/headers")
        .send()
        .await
        .expect("request to httpbin should succeed");

    let body: serde_json::Value = resp
        .json()
        .await
        .expect("response should be valid JSON");

    let ua = body
        .get("headers")
        .and_then(|h| h.get("User-Agent"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    assert_eq!(
        ua, custom_ua,
        "custom User-Agent should be sent, got: {ua}"
    );
}

/// Verify that Chrome-like sec-ch-ua headers are also present (not wiped
/// by the header ordering).
#[tokio::test]
async fn test_http_client_sends_sec_ch_ua_headers() {
    let config = BrowserConfig::default();
    let client = pardus_core::app::build_http_client(&config)
        .expect("build_http_client should succeed");

    let resp = client
        .get("https://httpbin.org/headers")
        .send()
        .await
        .expect("request to httpbin should succeed");

    let body: serde_json::Value = resp
        .json()
        .await
        .expect("response should be valid JSON");

    let headers = body.get("headers").expect("should contain headers");

    let sec_ch_ua = headers
        .get("Sec-Ch-Ua")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    assert!(
        sec_ch_ua.contains("Chrome"),
        "sec-ch-ua should mention Chrome, got: {sec_ch_ua}"
    );

    let sec_fetch_dest = headers
        .get("Sec-Fetch-Dest")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    assert_eq!(
        sec_fetch_dest, "document",
        "sec-fetch-dest should be 'document', got: {sec_fetch_dest}"
    );
}
