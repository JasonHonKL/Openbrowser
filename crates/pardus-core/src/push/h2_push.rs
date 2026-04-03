//! Low-level HTTP/2 PUSH_PROMISE reception.
//!
//! Provides [`H2PushReceiver`] which can intercept PUSH_PROMISE frames on an
//! HTTP/2 connection and buffer the pushed response data into a [`PushCache`].
//!
//! This module is behind the `h2-push` feature flag. It uses the `h2` crate
//! directly to handle PUSH_PROMISE frames, since `rquest` does not expose this
//! functionality.
//!
//! ## Important
//!
//! HTTP/2 server push was deprecated and removed from Chrome (2022) and Firefox
//! (2023). Most modern servers no longer send PUSH_PROMISE frames. This module
//! exists for compatibility with servers that still support push.

use bytes::Bytes;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, trace, warn};

use crate::push::push_cache::{PushCache, PushSource};

/// Configuration for H2 push reception.
#[derive(Debug, Clone)]
pub struct H2PushConfig {
    /// Maximum number of concurrent push streams to accept per connection.
    pub max_pushed_streams: usize,
    /// Maximum size per pushed resource in bytes.
    pub max_push_size: usize,
    /// Timeout for receiving pushed stream data.
    pub push_timeout: Duration,
}

impl Default for H2PushConfig {
    fn default() -> Self {
        Self {
            max_pushed_streams: 16,
            max_push_size: 5 * 1024 * 1024, // 5 MB
            push_timeout: Duration::from_secs(10),
        }
    }
}

/// Information about a received push promise.
#[derive(Debug, Clone)]
pub struct PushPromiseInfo {
    /// The stream ID assigned by the server.
    pub promised_stream_id: u32,
    /// The request headers from the PUSH_PROMISE frame.
    pub request_url: String,
    /// The authority (host:port) from the :authority pseudo-header.
    pub authority: Option<String>,
    /// The scheme from the :scheme pseudo-header.
    pub scheme: Option<String>,
    /// When the push promise was received.
    pub received_at: std::time::Instant,
}

impl PushPromiseInfo {
    /// Extract the URL from h2 pseudo-headers.
    pub fn from_headers(headers: &[(String, String)], promised_stream_id: u32) -> Option<Self> {
        let mut scheme = None;
        let mut authority = None;
        let mut path = None;

        for (name, value) in headers {
            match name.as_str() {
                ":scheme" => scheme = Some(value.clone()),
                ":authority" => authority = Some(value.clone()),
                ":path" => path = Some(value.clone()),
                _ => {}
            }
        }

        let path = path?;

        let request_url = if let (Some(s), Some(a)) = (&scheme, &authority) {
            format!("{}://{}{}", s, a, path)
        } else {
            path.clone()
        };

        Some(Self {
            promised_stream_id,
            request_url,
            authority,
            scheme,
            received_at: std::time::Instant::now(),
        })
    }
}

/// Receiver for HTTP/2 PUSH_PROMISE frames.
///
/// Stores pushed resource data into a [`PushCache`] so that later explicit
/// fetches can use the pre-loaded data.
pub struct H2PushReceiver {
    config: H2PushConfig,
    push_cache: Arc<PushCache>,
    active_pushes: parking_lot::Mutex<std::collections::HashSet<String>>,
    pushed_count: std::sync::atomic::AtomicUsize,
    rejected_count: std::sync::atomic::AtomicUsize,
}

impl H2PushReceiver {
    pub fn new(config: H2PushConfig, push_cache: Arc<PushCache>) -> Self {
        Self {
            config,
            push_cache,
            active_pushes: parking_lot::Mutex::new(std::collections::HashSet::new()),
            pushed_count: std::sync::atomic::AtomicUsize::new(0),
            rejected_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Create with default config.
    pub fn with_cache(push_cache: Arc<PushCache>) -> Self {
        Self::new(H2PushConfig::default(), push_cache)
    }

    /// Handle a received PUSH_PROMISE frame.
    ///
    /// Returns `true` if the push was accepted, `false` if rejected.
    /// Pushes are rejected if:
    /// - The URL is already cached
    /// - Too many active push streams
    /// - The URL is invalid
    pub fn on_push_promise(&self, info: &PushPromiseInfo) -> bool {
        let url = &info.request_url;

        // Skip data: URLs and invalid schemes
        if url.starts_with("data:") || url.starts_with("javascript:") {
            self.rejected_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            trace!("h2 push: rejected data/javascript URL: {}", url);
            return false;
        }

        // Skip if already cached
        if self.push_cache.contains(url) {
            self.rejected_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            trace!("h2 push: already cached, rejecting: {}", url);
            return false;
        }

        // Check concurrent push limit
        {
            let active = self.active_pushes.lock();
            if active.len() >= self.config.max_pushed_streams {
                self.rejected_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                debug!("h2 push: max streams reached, rejecting: {}", url);
                return false;
            }
        }

        // Accept the push
        {
            let mut active = self.active_pushes.lock();
            active.insert(url.clone());
        }
        self.pushed_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        debug!(
            "h2 push: accepted PUSH_PROMISE stream {} for {}",
            info.promised_stream_id, url
        );
        true
    }

    /// Store a successfully received pushed resource.
    pub fn on_push_data(
        &self,
        url: &str,
        status: u16,
        body: Bytes,
        content_type: Option<String>,
        duration_ms: u64,
    ) {
        if body.len() > self.config.max_push_size {
            warn!(
                "h2 push: resource too large ({} bytes), dropping: {}",
                body.len(),
                url,
            );
            self.remove_active(url);
            return;
        }

        let body_len = body.len();
        self.push_cache.insert_success(
            url.to_string(),
            status,
            body,
            content_type,
            duration_ms,
            PushSource::H2PushPromise,
        );
        self.remove_active(url);
        debug!("h2 push: stored {} bytes for {}", body_len, url);
    }

    /// Handle a failed push stream.
    pub fn on_push_error(&self, url: &str, error: &str) {
        warn!("h2 push: failed for {}: {}", url, error);
        self.remove_active(url);
    }

    /// Handle a canceled push stream.
    pub fn on_push_cancel(&self, url: &str) {
        trace!("h2 push: canceled: {}", url);
        self.remove_active(url);
    }

    /// Statistics for this receiver.
    pub fn stats(&self) -> H2PushStats {
        H2PushStats {
            pushed: self.pushed_count.load(std::sync::atomic::Ordering::Relaxed),
            rejected: self
                .rejected_count
                .load(std::sync::atomic::Ordering::Relaxed),
            active: self.active_pushes.lock().len(),
            max_pushed: self.config.max_pushed_streams,
        }
    }

    fn remove_active(&self, url: &str) {
        self.active_pushes.lock().remove(url);
    }
}

/// Statistics for H2 push reception.
#[derive(Debug, Clone)]
pub struct H2PushStats {
    pub pushed: usize,
    pub rejected: usize,
    pub active: usize,
    pub max_pushed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_receiver() -> H2PushReceiver {
        let cache = Arc::new(PushCache::new(32, 30));
        H2PushReceiver::with_cache(cache)
    }

    fn make_push_info(url: &str, stream_id: u32) -> PushPromiseInfo {
        PushPromiseInfo {
            promised_stream_id: stream_id,
            request_url: url.to_string(),
            authority: None,
            scheme: None,
            received_at: std::time::Instant::now(),
        }
    }

    #[test]
    fn test_accept_push() {
        let receiver = make_receiver();
        let info = make_push_info("https://example.com/style.css", 2);
        assert!(receiver.on_push_promise(&info));
        assert_eq!(receiver.stats().pushed, 1);
        assert_eq!(receiver.stats().active, 1);
    }

    #[test]
    fn test_reject_data_url() {
        let receiver = make_receiver();
        let info = make_push_info("data:text/css,body{}", 2);
        assert!(!receiver.on_push_promise(&info));
        assert_eq!(receiver.stats().rejected, 1);
    }

    #[test]
    fn test_reject_javascript_url() {
        let receiver = make_receiver();
        let info = make_push_info("javascript:void(0)", 2);
        assert!(!receiver.on_push_promise(&info));
    }

    #[test]
    fn test_reject_already_cached() {
        let receiver = make_receiver();
        receiver.push_cache.insert_success(
            "https://example.com/style.css".to_string(),
            200,
            Bytes::from_static(b"body{}"),
            Some("text/css".to_string()),
            5,
            PushSource::EarlyScan,
        );
        let info = make_push_info("https://example.com/style.css", 2);
        assert!(!receiver.on_push_promise(&info));
        assert_eq!(receiver.stats().rejected, 1);
    }

    #[test]
    fn test_reject_max_streams() {
        let receiver = H2PushReceiver::new(
            H2PushConfig {
                max_pushed_streams: 1,
                ..Default::default()
            },
            Arc::new(PushCache::new(32, 30)),
        );
        let info1 = make_push_info("https://example.com/a.css", 2);
        let info2 = make_push_info("https://example.com/b.js", 4);
        assert!(receiver.on_push_promise(&info1));
        assert!(!receiver.on_push_promise(&info2));
        assert_eq!(receiver.stats().pushed, 1);
        assert_eq!(receiver.stats().rejected, 1);
    }

    #[test]
    fn test_store_push_data() {
        let receiver = make_receiver();
        let info = make_push_info("https://example.com/style.css", 2);
        receiver.on_push_promise(&info);
        receiver.on_push_data(
            "https://example.com/style.css",
            200,
            Bytes::from_static(b"body{}"),
            Some("text/css".to_string()),
            10,
        );
        assert_eq!(receiver.stats().active, 0);

        let entry = receiver
            .push_cache
            .get("https://example.com/style.css")
            .unwrap();
        assert_eq!(entry.source, PushSource::H2PushPromise);
    }

    #[test]
    fn test_reject_oversized_push() {
        let receiver = H2PushReceiver::new(
            H2PushConfig {
                max_push_size: 10,
                ..Default::default()
            },
            Arc::new(PushCache::new(32, 30)),
        );
        let info = make_push_info("https://example.com/large.js", 2);
        receiver.on_push_promise(&info);
        receiver.on_push_data(
            "https://example.com/large.js",
            200,
            Bytes::from_static(b"this is way too large"),
            Some("application/javascript".to_string()),
            10,
        );
        assert!(receiver
            .push_cache
            .get("https://example.com/large.js")
            .is_none());
    }

    #[test]
    fn test_push_error() {
        let receiver = make_receiver();
        let info = make_push_info("https://example.com/style.css", 2);
        receiver.on_push_promise(&info);
        assert_eq!(receiver.stats().active, 1);
        receiver.on_push_error("https://example.com/style.css", "stream reset");
        assert_eq!(receiver.stats().active, 0);
    }

    #[test]
    fn test_push_cancel() {
        let receiver = make_receiver();
        let info = make_push_info("https://example.com/style.css", 2);
        receiver.on_push_promise(&info);
        receiver.on_push_cancel("https://example.com/style.css");
        assert_eq!(receiver.stats().active, 0);
    }

    #[test]
    fn test_from_headers() {
        let headers = vec![
            (":scheme".to_string(), "https".to_string()),
            (":authority".to_string(), "example.com".to_string()),
            (":path".to_string(), "/style.css".to_string()),
        ];
        let info = PushPromiseInfo::from_headers(&headers, 2).unwrap();
        assert_eq!(info.request_url, "https://example.com/style.css");
        assert_eq!(info.promised_stream_id, 2);
    }

    #[test]
    fn test_from_headers_no_path() {
        let headers = vec![
            (":scheme".to_string(), "https".to_string()),
            (":authority".to_string(), "example.com".to_string()),
        ];
        assert!(PushPromiseInfo::from_headers(&headers, 2).is_none());
    }
}
