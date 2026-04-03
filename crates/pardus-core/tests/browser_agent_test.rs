//! Tests for browser agent (human-like browser headers) feature.
//!
//! Tests that BrowserAgentConfig correctly generates realistic browser headers
//! and that they are applied to HTTP requests.

use pardus_core::{BrowserConfig, BrowserAgentConfig, RefererPolicy};

// ---------------------------------------------------------------------------
// BrowserAgentConfig Tests
// ---------------------------------------------------------------------------

#[test]
fn test_browser_agent_config_default_disabled() {
    let config = BrowserAgentConfig::default();
    assert!(!config.enabled);
}

#[test]
fn test_chrome_macos_profile() {
    let config = BrowserAgentConfig::chrome_macos();
    assert!(config.enabled);
    assert!(config.user_agent.contains("Chrome"));
    assert!(config.user_agent.contains("Macintosh"));
    assert!(config.sec_fetch_headers);
    assert!(!config.dnt);
    assert!(config.keep_alive);
}

#[test]
fn test_chrome_windows_profile() {
    let config = BrowserAgentConfig::chrome_windows();
    assert!(config.enabled);
    assert!(config.user_agent.contains("Chrome"));
    assert!(config.user_agent.contains("Windows NT 10.0"));
    assert!(config.sec_fetch_headers);
}

#[test]
fn test_firefox_macos_profile() {
    let config = BrowserAgentConfig::firefox_macos();
    assert!(config.enabled);
    assert!(config.user_agent.contains("Firefox"));
    assert!(config.user_agent.contains("Macintosh"));
    assert!(!config.sec_fetch_headers);
    assert!(config.dnt);
}

#[test]
fn test_safari_macos_profile() {
    let config = BrowserAgentConfig::safari_macos();
    assert!(config.enabled);
    assert!(config.user_agent.contains("Safari"));
    assert!(config.user_agent.contains("Version/17.1"));
    assert!(config.sec_fetch_headers);
}

#[test]
fn test_browser_agent_headers_generation() {
    let config = BrowserAgentConfig::chrome_macos();
    let headers = config.to_headers();
    let header_map: std::collections::HashMap<\u0026str, String> = headers.into_iter().collect();
    assert!(header_map.contains_key("Accept"));
    assert!(header_map.contains_key("Accept-Language"));
    assert!(header_map.contains_key("Accept-Encoding"));
    assert!(header_map.contains_key("Cache-Control"));
    assert!(header_map.contains_key("Connection"));
    assert!(header_map.contains_key("Sec-Fetch-Dest"));
    assert!(header_map.contains_key("Sec-Fetch-Mode"));
    assert!(header_map.contains_key("Sec-Fetch-Site"));
    assert!(header_map.contains_key("Sec-Fetch-User"));
    assert!(header_map.contains_key("Upgrade-Insecure-Requests"));
}

#[test]
fn test_firefox_headers_no_sec_fetch() {
    let config = BrowserAgentConfig::firefox_macos();
    let headers = config.to_headers();
    let header_map: std::collections::HashMap<\u0026str, String> = headers.into_iter().collect();
    assert!(!header_map.contains_key("Sec-Fetch-Dest"));
    assert!(header_map.contains_key("DNT"));
}

#[test]
fn test_browser_config_with_browser_agent() {
    let agent_config = BrowserAgentConfig::chrome_macos();
    let browser_config = BrowserConfig::default()
        .with_browser_agent(agent_config);
    assert!(browser_config.browser_agent.enabled);
    assert!(browser_config.browser_agent.user_agent.contains("Chrome"));
}

#[test]
fn test_effective_user_agent_with_browser_agent() {
    let agent_config = BrowserAgentConfig::chrome_macos();
    let browser_config = BrowserConfig::default()
        .with_browser_agent(agent_config);
    let ua = browser_config.effective_user_agent();
    assert!(ua.contains("Chrome"));
    assert!(!ua.contains("PardusBrowser"));
}

#[test]
fn test_effective_user_agent_without_browser_agent() {
    let browser_config = BrowserConfig::default();
    let ua = browser_config.effective_user_agent();
    assert!(ua.contains("PardusBrowser"));
}

#[test]
fn test_browser_agent_request_delay_range() {
    let chrome = BrowserAgentConfig::chrome_macos();
    let firefox = BrowserAgentConfig::firefox_macos();
    let safari = BrowserAgentConfig::safari_macos();
    assert_eq!(chrome.request_delay_ms, (100, 500));
    assert_eq!(firefox.request_delay_ms, (150, 600));
    assert_eq!(safari.request_delay_ms, (200, 800));
}

#[test]
fn test_referer_policy_default() {
    let config = BrowserAgentConfig::chrome_macos();
    assert!(matches!(config.referer_policy, RefererPolicy::Always));
}

#[test]
fn test_accept_header_content() {
    let config = BrowserAgentConfig::chrome_macos();
    let headers = config.to_headers();
    let header_map: std::collections::HashMap<\u0026str, String> = headers.into_iter().collect();
    let accept = header_map.get("Accept").expect("Accept header should exist");
    assert!(accept.contains("text/html"));
    assert!(accept.contains("application/xhtml+xml"));
}

#[test]
fn test_accept_language_header() {
    let chrome = BrowserAgentConfig::chrome_macos();
    let firefox = BrowserAgentConfig::firefox_macos();
    let chrome_headers: std::collections::HashMap<\u0026str, String> = chrome.to_headers().into_iter().collect();
    let firefox_headers: std::collections::HashMap<\u0026str, String> = firefox.to_headers().into_iter().collect();
    assert_eq!(chrome_headers.get("Accept-Language").unwrap(), "en-US,en;q=0.9");
    assert_eq!(firefox_headers.get("Accept-Language").unwrap(), "en-US,en;q=0.5");
}
