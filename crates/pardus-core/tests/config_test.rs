//! Tests for BrowserConfig defaults and sub-configs.
//!
//! Verifies that all configuration structs produce sensible defaults
//! and that builder methods chain correctly.

use pardus_core::BrowserConfig;

// ---------------------------------------------------------------------------
// BrowserConfig defaults
// ---------------------------------------------------------------------------

#[test]
fn test_default_user_agent_is_chrome() {
    let config = BrowserConfig::default();
    let ua = config.effective_user_agent();
    assert!(ua.starts_with("Mozilla/5.0"));
    assert!(ua.contains("Chrome/131"));
    assert!(ua.contains("Safari/537.36"));
}

#[test]
fn test_default_timeouts() {
    let config = BrowserConfig::default();
    assert_eq!(config.timeout_ms, 10_000);
    assert_eq!(config.wait_ms, 3_000);
}

#[test]
fn test_default_viewport() {
    let config = BrowserConfig::default();
    assert_eq!(config.viewport_width, 1280);
    assert_eq!(config.viewport_height, 720);
}

#[test]
fn test_default_iframe_settings() {
    let config = BrowserConfig::default();
    assert!(config.parse_iframes);
    assert_eq!(config.max_iframe_depth, 5);
}

#[test]
fn test_default_limits() {
    let config = BrowserConfig::default();
    assert_eq!(config.max_upload_size, 50 * 1024 * 1024);
    assert_eq!(config.max_redirects, 10);
}

#[test]
fn test_default_screenshot_options() {
    let config = BrowserConfig::default();
    assert!(config.screenshot_endpoint.is_none());
    assert_eq!(config.screenshot_timeout_ms, 10_000);
    assert!(config.screenshot_chrome_path.is_none());
}

// ---------------------------------------------------------------------------
// ConnectionPoolConfig defaults
// ---------------------------------------------------------------------------

#[test]
fn test_default_connection_pool() {
    let config = BrowserConfig::default();
    assert_eq!(config.connection_pool.max_idle_per_host, 32);
    assert_eq!(config.connection_pool.idle_timeout_secs, 90);
    assert_eq!(config.connection_pool.tcp_keepalive_secs, 60);
    assert!(config.connection_pool.enable_http2);
}

// ---------------------------------------------------------------------------
// PushConfig defaults
// ---------------------------------------------------------------------------

#[test]
fn test_default_push_config() {
    let config = BrowserConfig::default();
    assert!(config.push.enable_push);
    assert_eq!(config.push.max_push_resources, 32);
    assert_eq!(config.push.push_cache_ttl_secs, 30);
}

// ---------------------------------------------------------------------------
// RetryConfig defaults
// ---------------------------------------------------------------------------

#[test]
fn test_default_retry_config() {
    let config = BrowserConfig::default();
    let retry = &config.retry;
    assert_eq!(retry.max_retries, 0); // disabled by default
    assert_eq!(retry.initial_backoff_ms, 100);
    assert_eq!(retry.max_backoff_ms, 10_000);
    assert!((retry.backoff_factor - 2.0).abs() < f64::EPSILON);
    assert_eq!(
        retry.retry_on_statuses,
        vec![408, 429, 500, 502, 503, 504]
    );
}

// ---------------------------------------------------------------------------
// CspConfig defaults
// ---------------------------------------------------------------------------

#[test]
fn test_default_csp_config() {
    let config = BrowserConfig::default();
    assert!(!config.csp.enforce_csp);
    assert!(config.csp.log_report_only);
    assert!(config.csp.override_policy.is_none());
}

// ---------------------------------------------------------------------------
// ProxyConfig defaults
// ---------------------------------------------------------------------------

#[test]
fn test_default_proxy_config() {
    let config = BrowserConfig::default();
    assert!(config.proxy.http_proxy.is_none());
    assert!(config.proxy.https_proxy.is_none());
    assert!(config.proxy.all_proxy.is_none());
    assert!(config.proxy.no_proxy.is_none());
}

// ---------------------------------------------------------------------------
// User-agent override
// ---------------------------------------------------------------------------

#[test]
fn test_custom_user_agent() {
    let mut config = BrowserConfig::default();
    config.user_agent = "CustomBot/1.0".to_string();
    assert_eq!(config.effective_user_agent(), "CustomBot/1.0");
}

#[test]
fn test_dedup_window_default_disabled() {
    let config = BrowserConfig::default();
    assert_eq!(config.dedup_window_ms, 0);
}
