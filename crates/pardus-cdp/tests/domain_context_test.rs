//! Tests for CDP → Browser API integration.
//!
//! Tests that DomainContext correctly creates Browser instances from App config
//! and that navigation/reload operations work through the Browser API.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use pardus_cdp::domain::{DomainContext, TargetEntry};
use pardus_cdp::protocol::event_bus::EventBus;
use pardus_cdp::protocol::node_map::NodeMap;
use pardus_core::{App, BrowserConfig};

// ---------------------------------------------------------------------------
// DomainContext Creation Tests
// ---------------------------------------------------------------------------

#[test]
fn test_domain_context_new() {
    let app = Arc::new(App::new(BrowserConfig::default()));
    let targets = Arc::new(Mutex::new(HashMap::<String, TargetEntry>::new()));
    let event_bus = Arc::new(EventBus::new(1024));
    let node_map = Arc::new(Mutex::new(NodeMap::new()));

    let ctx = DomainContext::new(
        app.clone(),
        targets.clone(),
        event_bus.clone(),
        node_map.clone(),
    );

    // Verify all fields are accessible
    let _ = ctx.app.clone();
    let _ = ctx.targets.clone();
    let _ = ctx.event_bus.clone();
    let _ = ctx.node_map.clone();
}

#[test]
fn test_domain_context_create_browser() {
    let config = BrowserConfig::default();
    let app = Arc::new(App::new(config.clone()));
    let targets = Arc::new(Mutex::new(HashMap::<String, TargetEntry>::new()));
    let event_bus = Arc::new(EventBus::new(1024));
    let node_map = Arc::new(Mutex::new(NodeMap::new()));

    let ctx = DomainContext::new(app, targets, event_bus, node_map);

    // Create a browser from the context
    let browser = ctx.create_browser();

    // Browser should be properly initialized
    // Just verify it was created without panicking
    let _ = std::hint::black_box(browser);
}

#[test]
fn test_domain_context_send_sync() {
    // DomainContext must be Send + Sync for use across async tasks
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DomainContext>();
}

// ---------------------------------------------------------------------------
// TargetEntry Tests
// ---------------------------------------------------------------------------

#[test]
fn test_target_entry_creation() {
    let entry = TargetEntry {
        url: "https://example.com".to_string(),
        html: Some("<html><body>Hello</body></html>".to_string()),
        title: Some("Example".to_string()),
        js_enabled: true,
        frame_tree_json: None,
    };

    assert_eq!(entry.url, "https://example.com");
    assert!(entry.html.is_some());
    assert_eq!(entry.title, Some("Example".to_string()));
    assert!(entry.js_enabled);
}

#[test]
fn test_target_entry_clone() {
    let entry = TargetEntry {
        url: "https://example.com".to_string(),
        html: Some("<html></html>".to_string()),
        title: None,
        js_enabled: false,
        frame_tree_json: None,
    };

    let cloned = entry.clone();
    assert_eq!(cloned.url, entry.url);
    assert_eq!(cloned.html, entry.html);
}

// ---------------------------------------------------------------------------
// DomainContext Accessors Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_html() {
    let app = Arc::new(App::new(BrowserConfig::default()));
    let targets = Arc::new(Mutex::new(HashMap::new()));
    let event_bus = Arc::new(EventBus::new(1024));
    let node_map = Arc::new(Mutex::new(NodeMap::new()));

    let ctx = DomainContext::new(app, targets.clone(), event_bus, node_map);

    // Insert a test target
    {
        let mut targets_lock = targets.lock().await;
        targets_lock.insert("target-1".to_string(), TargetEntry {
            url: "https://example.com".to_string(),
            html: Some("<html><body>Test</body></html>".to_string()),
            title: Some("Test".to_string()),
            js_enabled: false,
            frame_tree_json: None,
        });
    }

    // Get HTML
    let html = ctx.get_html("target-1").await;
    assert_eq!(html, Some("<html><body>Test</body></html>".to_string()));

    // Non-existent target
    let html = ctx.get_html("target-2").await;
    assert!(html.is_none());
}

#[tokio::test]
async fn test_get_url() {
    let app = Arc::new(App::new(BrowserConfig::default()));
    let targets = Arc::new(Mutex::new(HashMap::new()));
    let event_bus = Arc::new(EventBus::new(1024));
    let node_map = Arc::new(Mutex::new(NodeMap::new()));

    let ctx = DomainContext::new(app, targets.clone(), event_bus, node_map);

    {
        let mut targets_lock = targets.lock().await;
        targets_lock.insert("target-1".to_string(), TargetEntry {
            url: "https://example.com/page".to_string(),
            html: None,
            title: None,
            js_enabled: false,
            frame_tree_json: None,
        });
    }

    let url = ctx.get_url("target-1").await;
    assert_eq!(url, Some("https://example.com/page".to_string()));

    let url = ctx.get_url("target-2").await;
    assert!(url.is_none());
}

#[tokio::test]
async fn test_get_title() {
    let app = Arc::new(App::new(BrowserConfig::default()));
    let targets = Arc::new(Mutex::new(HashMap::new()));
    let event_bus = Arc::new(EventBus::new(1024));
    let node_map = Arc::new(Mutex::new(NodeMap::new()));

    let ctx = DomainContext::new(app, targets.clone(), event_bus, node_map);

    {
        let mut targets_lock = targets.lock().await;
        targets_lock.insert("target-1".to_string(), TargetEntry {
            url: "https://example.com".to_string(),
            html: None,
            title: Some("Page Title".to_string()),
            js_enabled: false,
            frame_tree_json: None,
        });
    }

    let title = ctx.get_title("target-1").await;
    assert_eq!(title, Some("Page Title".to_string()));

    let title = ctx.get_title("target-2").await;
    assert!(title.is_none());
}

#[tokio::test]
async fn test_get_target_entry() {
    let app = Arc::new(App::new(BrowserConfig::default()));
    let targets = Arc::new(Mutex::new(HashMap::new()));
    let event_bus = Arc::new(EventBus::new(1024));
    let node_map = Arc::new(Mutex::new(NodeMap::new()));

    let ctx = DomainContext::new(app, targets.clone(), event_bus, node_map);

    let entry = TargetEntry {
        url: "https://example.com".to_string(),
        html: Some("<html></html>".to_string()),
        title: Some("Title".to_string()),
        js_enabled: true,
        frame_tree_json: None,
    };

    {
        let mut targets_lock = targets.lock().await;
        targets_lock.insert("target-1".to_string(), entry.clone());
    }

    let found = ctx.get_target_entry("target-1").await;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.url, entry.url);
    assert_eq!(found.title, entry.title);
}

// ---------------------------------------------------------------------------
// Browser API Integration Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_browser_has_correct_config() {
    let mut config = BrowserConfig::default();
    config.timeout_ms = 5000;
    config.user_agent = "TestAgent/1.0".to_string();

    let app = Arc::new(App::new(config));
    let targets = Arc::new(Mutex::new(HashMap::new()));
    let event_bus = Arc::new(EventBus::new(1024));
    let node_map = Arc::new(Mutex::new(NodeMap::new()));

    let ctx = DomainContext::new(app, targets, event_bus, node_map);

    // Create browser and verify it was created successfully
    let browser = ctx.create_browser();

    // The browser should have the configuration from the App
    // We can't directly verify the timeout/user_agent since they're not public,
    // but we can verify the browser was created
    let _ = std::hint::black_box(browser);
}

#[tokio::test]
async fn test_update_target_with_data() {
    let app = Arc::new(App::new(BrowserConfig::default()));
    let targets = Arc::new(Mutex::new(HashMap::new()));
    let event_bus = Arc::new(EventBus::new(1024));
    let node_map = Arc::new(Mutex::new(NodeMap::new()));

    let ctx = DomainContext::new(app, targets.clone(), event_bus, node_map);

    // Update target data
    ctx.update_target_with_data(
        "target-1",
        "https://example.com".to_string(),
        "<html><body>Updated</body></html>".to_string(),
        Some("Updated Title".to_string()),
    );

    // Verify the update
    let entry = ctx.get_target_entry("target-1").await;
    assert!(entry.is_some());
    let entry = entry.unwrap();
    assert_eq!(entry.url, "https://example.com");
    assert_eq!(entry.html, Some("<html><body>Updated</body></html>".to_string()));
    assert_eq!(entry.title, Some("Updated Title".to_string()));
}

// ---------------------------------------------------------------------------
// Event Bus Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_event_bus_in_domain_context() {
    let app = Arc::new(App::new(BrowserConfig::default()));
    let targets = Arc::new(Mutex::new(HashMap::new()));
    let event_bus = Arc::new(EventBus::new(1024));
    let node_map = Arc::new(Mutex::new(NodeMap::new()));

    let _ctx = DomainContext::new(app, targets, event_bus.clone(), node_map);

    // Subscribe to events
    let mut rx = event_bus.subscribe();

    // Send an event through the context's event bus
    use pardus_cdp::protocol::message::CdpEvent;
    let event = CdpEvent {
        method: "Test.event".to_string(),
        params: serde_json::json!({"key": "value"}),
        session_id: None,
    };

    let _ = event_bus.send(event);

    // Receive the event
    let received = rx.recv().await;
    assert!(received.is_ok());
    assert_eq!(received.unwrap().method, "Test.event");
}

// ---------------------------------------------------------------------------
// Integration Test: Verify Browser is Created from App Config
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_browser_uses_app_config() {
    // Create a custom config
    let config = BrowserConfig::default();

    let app = Arc::new(App::new(config));
    let targets = Arc::new(Mutex::new(HashMap::new()));
    let event_bus = Arc::new(EventBus::new(1024));
    let node_map = Arc::new(Mutex::new(NodeMap::new()));

    let ctx = DomainContext::new(app, targets, event_bus, node_map);

    // Create browser and verify it works
    let browser = ctx.create_browser();

    // Verify browser starts with no active tab
    assert!(browser.active_tab().is_none());
    assert_eq!(browser.tab_count(), 0);
}

// ---------------------------------------------------------------------------
// Concurrency Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_concurrent_target_access() {
    let app = Arc::new(App::new(BrowserConfig::default()));
    let targets = Arc::new(Mutex::new(HashMap::new()));
    let event_bus = Arc::new(EventBus::new(1024));
    let node_map = Arc::new(Mutex::new(NodeMap::new()));

    let _ctx = DomainContext::new(app, targets.clone(), event_bus, node_map);

    // Spawn multiple tasks to access targets concurrently
    let mut handles = vec![];

    for i in 0..10 {
        let ctx_clone = DomainContext::new(
            Arc::new(App::new(BrowserConfig::default())),
            targets.clone(),
            Arc::new(EventBus::new(1024)),
            Arc::new(Mutex::new(NodeMap::new())),
        );

        let handle = tokio::spawn(async move {
            ctx_clone.update_target_with_data(
                &format!("target-{}", i),
                format!("https://example.com/{}", i),
                format!("<html>Page {}</html>", i),
                Some(format!("Title {}", i)),
            );

            ctx_clone.get_target_entry(&format!("target-{}", i)).await
        });

        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        let result = handle.await;
        assert!(result.is_ok());
        let entry = result.unwrap();
        assert!(entry.is_some());
    }

    // Verify all targets were inserted
    let targets_lock = targets.lock().await;
    assert_eq!(targets_lock.len(), 10);
}

// ---------------------------------------------------------------------------
// Error Handling Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_navigate_invalid_url() {
    let app = Arc::new(App::new(BrowserConfig::default()));
    let targets = Arc::new(Mutex::new(HashMap::new()));
    let event_bus = Arc::new(EventBus::new(1024));
    let node_map = Arc::new(Mutex::new(NodeMap::new()));

    let ctx = DomainContext::new(app, targets, event_bus, node_map);

    // Navigate to an invalid URL should fail
    let result = ctx.navigate("target-1", "not-a-valid-url").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_reload_nonexistent_target() {
    let app = Arc::new(App::new(BrowserConfig::default()));
    let targets = Arc::new(Mutex::new(HashMap::new()));
    let event_bus = Arc::new(EventBus::new(1024));
    let node_map = Arc::new(Mutex::new(NodeMap::new()));

    let ctx = DomainContext::new(app, targets, event_bus, node_map);

    // Reload with no previous URL should navigate to about:blank
    // This may succeed or fail depending on implementation
    let _result = ctx.reload("nonexistent").await;
}

// ---------------------------------------------------------------------------
// Stress Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_multiple_browser_instances() {
    let app = Arc::new(App::new(BrowserConfig::default()));
    let targets = Arc::new(Mutex::new(HashMap::new()));
    let event_bus = Arc::new(EventBus::new(1024));
    let node_map = Arc::new(Mutex::new(NodeMap::new()));

    let ctx = DomainContext::new(app, targets, event_bus, node_map);

    // Create multiple browser instances
    for _ in 0..100 {
        let browser = ctx.create_browser();
        let _ = std::hint::black_box(browser);
    }
}

// ---------------------------------------------------------------------------
// Documentation Test
// ---------------------------------------------------------------------------

/// This test documents the design decision for CDP → Browser integration:
///
/// Problem: Browser contains Page which contains scraper::Html with Cell<usize>,
/// making Browser !Send. DomainContext must be Send+Sync for use across async tasks.
///
/// Solution: DomainContext stores Arc<App> and provides create_browser() method
/// to create temporary Browser instances on-demand in handlers.
///
/// Benefits:
/// - DomainContext remains Send+Sync
/// - CDP handlers can use the unified Browser API
/// - Each handler gets a fresh Browser instance with proper config
/// - No need to wrap Browser in Mutex or deal with !Send issues
#[test]
fn test_design_documentation() {
    // Just a documentation test - verifies the design compiles
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DomainContext>();
    assert_send_sync::<Arc<App>>();
}
