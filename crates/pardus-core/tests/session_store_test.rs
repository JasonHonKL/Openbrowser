//! Tests for SessionStore — ephemeral sessions, cookies, headers, localStorage.
//!
//! Verifies that ephemeral sessions work correctly, localStorage CRUD works,
//! and that header parsing produces expected values.

use base64::Engine;
use pardus_core::SessionStore;
use std::path::PathBuf;

fn tmp_dir() -> PathBuf {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    std::env::temp_dir().join(format!("pardus-test-session-{}", ms))
}

// ---------------------------------------------------------------------------
// Creation
// ---------------------------------------------------------------------------

#[test]
fn test_ephemeral_session_create() {
    let dir = tmp_dir();
    let store = SessionStore::ephemeral("test", &dir).expect("should create ephemeral");
    assert_eq!(store.session_name(), "test");
    assert_eq!(store.cookie_count(), 0);
    assert_eq!(store.header_count(), 0);
}

#[test]
fn test_persistent_session_create() {
    let dir = tmp_dir();
    let store = SessionStore::load("persistent-test", &dir).expect("should create persistent");
    assert_eq!(store.session_name(), "persistent-test");
}

// ---------------------------------------------------------------------------
// Cookies
// ---------------------------------------------------------------------------

#[test]
fn test_set_and_count_cookies() {
    let dir = tmp_dir();
    let store = SessionStore::ephemeral("test", &dir).unwrap();
    assert_eq!(store.cookie_count(), 0);

    store.set_cookie("session", "abc123", "example.com", "/");
    assert_eq!(store.cookie_count(), 1);
    store.set_cookie("theme", "dark", "example.com", "/");
    assert_eq!(store.cookie_count(), 2);
}

#[test]
fn test_clear_cookies() {
    let dir = tmp_dir();
    let store = SessionStore::ephemeral("test", &dir).unwrap();
    store.set_cookie("a", "1", "example.com", "/");
    store.set_cookie("b", "2", "example.com", "/");
    assert_eq!(store.cookie_count(), 2);
    store.clear_cookies();
    assert_eq!(store.cookie_count(), 0);
}

#[test]
fn test_delete_cookie() {
    let dir = tmp_dir();
    let store = SessionStore::ephemeral("test", &dir).unwrap();
    store.set_cookie("keep", "1", "example.com", "/");
    store.set_cookie("remove", "2", "example.com", "/");
    let deleted = store.delete_cookie("remove", "example.com", "/");
    assert!(deleted);
    assert_eq!(store.cookie_count(), 1);
}

#[test]
fn test_all_cookies() {
    let dir = tmp_dir();
    let store = SessionStore::ephemeral("test", &dir).unwrap();
    store.set_cookie("a", "1", "x.com", "/");
    store.set_cookie("b", "2", "y.com", "/");
    let cookies = store.all_cookies();
    assert_eq!(cookies.len(), 2);
}

// ---------------------------------------------------------------------------
// Headers
// ---------------------------------------------------------------------------

#[test]
fn test_add_and_count_headers() {
    let dir = tmp_dir();
    let store = SessionStore::ephemeral("test", &dir).unwrap();
    assert_eq!(store.header_count(), 0);
    store.add_header("Authorization", "Bearer token123");
    assert_eq!(store.header_count(), 1);
    store.add_header("X-Custom", "value");
    assert_eq!(store.header_count(), 2);
}

// ---------------------------------------------------------------------------
// localStorage — persistent session needed (ephemeral sets no_local_storage: true)
// ---------------------------------------------------------------------------

#[test]
fn test_local_storage_crud() {
    let dir = tmp_dir();
    let store = SessionStore::load("test", &dir).unwrap();
    assert!(store.local_storage_get("https://example.com", "key").is_none());
    store.local_storage_set("https://example.com", "key", "value");
    assert_eq!(
        store.local_storage_get("https://example.com", "key"),
        Some("value".to_string())
    );
    let keys = store.local_storage_keys("https://example.com");
    assert_eq!(keys, vec!["key"]);
    store.local_storage_remove("https://example.com", "key");
    assert!(store.local_storage_get("https://example.com", "key").is_none());
}

#[test]
fn test_local_storage_origins() {
    let dir = tmp_dir();
    let store = SessionStore::load("test", &dir).unwrap();
    store.local_storage_set("https://a.com", "k", "v");
    store.local_storage_set("https://b.com", "k", "v");
    let origins = store.local_storage_origins();
    assert_eq!(origins.len(), 2);
    assert!(origins.contains(&"https://a.com".to_string()));
    assert!(origins.contains(&"https://b.com".to_string()));
}

#[test]
fn test_local_storage_clear() {
    let dir = tmp_dir();
    let store = SessionStore::load("test", &dir).unwrap();
    store.local_storage_set("https://example.com", "a", "1");
    store.local_storage_set("https://example.com", "b", "2");
    store.local_storage_clear("https://example.com");
    assert!(store.local_storage_keys("https://example.com").is_empty());
}

// ---------------------------------------------------------------------------
// Auth header parsing
// ---------------------------------------------------------------------------

#[test]
fn test_parse_bearer_auth() {
    let result = SessionStore::parse_auth_header("bearer:abc123");
    assert_eq!(
        result,
        Some(("Authorization".to_string(), "Bearer abc123".to_string()))
    );
}

#[test]
fn test_parse_basic_auth() {
    // parse_auth_header expects "basic:user:pass" and base64-encodes it
    let result = SessionStore::parse_auth_header("basic:user:pass");
    let expected_b64 = base64::engine::general_purpose::STANDARD.encode("user:pass");
    assert_eq!(
        result,
        Some(("Authorization".to_string(), format!("Basic {}", expected_b64)))
    );
}

#[test]
fn test_parse_auth_invalid() {
    assert!(SessionStore::parse_auth_header("invalid").is_none());
    assert!(SessionStore::parse_auth_header("").is_none());
}

#[test]
fn test_parse_custom_header() {
    let result = SessionStore::parse_custom_header("X-API-Key: mykey123");
    assert_eq!(
        result,
        Some(("X-API-Key".to_string(), "mykey123".to_string()))
    );
}

#[test]
fn test_parse_custom_header_no_colon() {
    assert!(SessionStore::parse_custom_header("no-colon-here").is_none());
}
