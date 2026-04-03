//! Unit tests for CDP DomainContext.
//!
//! These tests verify that DomainContext correctly maintains Send+Sync properties
//! and integrates with the pardus-core types.

#[cfg(test)]
mod tests {
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
    // Documentation Test
    // ---------------------------------------------------------------------------

    /// This test documents the design decision for CDP integration:
    ///
    /// DomainContext uses Arc<App> directly because:
    /// - Browser contains Page which contains scraper::Html with Cell<usize>
    /// - Browser is !Send and cannot be held across await points in CDP handlers
    /// - CDP handlers must be Send + Sync for use across async tasks
    /// - App is Send + Sync and provides the HTTP client, config, and network log
    ///
    /// The Browser API is used by CLI commands and other sync contexts, while
    /// CDP handlers work directly with App and Page for async operations.
    #[test]
    fn test_design_documentation() {
        // Just a documentation test - verifies the design compiles
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DomainContext>();
        assert_send_sync::<Arc<App>>();
    }
}
