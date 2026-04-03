//! Request deduplication — avoids parallel fetches of the same URL within a time window.

use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::Notify;

/// Cached result of a completed fetch, shared among deduplicated callers.
#[derive(Debug, Clone)]
pub struct DedupResult {
    pub url: String,
    pub status: u16,
    pub body: Vec<u8>,
    pub content_type: Option<String>,
    pub headers: Vec<(String, String)>,
    pub http_version: String,
}

#[derive(Debug)]
enum Entry {
    /// A fetch for this URL is currently in-flight.
    InFlight(Arc<Notify>),
    /// A previous fetch completed within the dedup window.
    Completed {
        result: Arc<DedupResult>,
        completed_at: std::time::Instant,
    },
}

/// Manages in-flight request deduplication.
///
/// When enabled, if two fetches for the same URL happen concurrently,
/// the second caller waits for and reuses the first caller's result.
#[derive(Debug)]
pub struct RequestDedup {
    inflight: DashMap<String, Entry>,
    /// Window in ms after completion during which the cached result is returned.
    window_ms: u64,
}

impl Clone for RequestDedup {
    fn clone(&self) -> Self {
        Self {
            inflight: DashMap::new(),
            window_ms: self.window_ms,
        }
    }
}

/// Outcome of calling `RequestDedup::enter()`.
pub enum DedupEntry {
    /// No existing request — caller should proceed with the fetch.
    Proceed,
    /// A previous fetch completed within the window — reuse this result.
    Cached(Arc<DedupResult>),
    /// A fetch is in-flight — await the notify, then check for a completed result.
    Wait(Arc<Notify>),
}

impl RequestDedup {
    /// Create a new dedup manager. `window_ms` of 0 disables dedup.
    pub fn new(window_ms: u64) -> Self {
        Self {
            inflight: DashMap::new(),
            window_ms,
        }
    }

    /// Whether deduplication is enabled.
    pub fn is_enabled(&self) -> bool {
        self.window_ms > 0
    }

    /// Enter the dedup for a URL key.
    ///
    /// - Returns `Proceed` if this is the first request (caller should fetch).
    /// - Returns `Cached(result)` if a completed result is available within the window.
    /// - Returns `Wait(notify)` if a request is in-flight (caller should await the notify).
    pub async fn enter(&self, url_key: &str) -> DedupEntry {
        // First, check for a completed result within the window.
        if let Some(entry) = self.inflight.get(url_key) {
            match &*entry {
                Entry::Completed { result, completed_at } => {
                    let elapsed = completed_at.elapsed().as_millis() as u64;
                    if elapsed < self.window_ms {
                        return DedupEntry::Cached(result.clone());
                    }
                    // Window expired — fall through to start a new fetch.
                }
                Entry::InFlight(notify) => {
                    let notify = notify.clone();
                    // Drop the dashmap reference before awaiting.
                    drop(entry);
                    notify.notified().await;
                    // After being notified, check for a completed result.
                    if let Some(entry) = self.inflight.get(url_key) {
                        if let Entry::Completed { result, .. } = &*entry {
                            return DedupEntry::Cached(result.clone());
                        }
                    }
                    // Result was removed (error path) — proceed with own fetch.
                    return DedupEntry::Proceed;
                }
            }
        }

        // No valid entry — register as in-flight.
        let notify = Arc::new(Notify::new());
        self.inflight
            .insert(url_key.to_string(), Entry::InFlight(notify));

        DedupEntry::Proceed
    }

    /// Mark a URL as completed with the given result.
    /// Notifies any waiting callers.
    pub fn complete(&self, url_key: &str, result: DedupResult) {
        let result = Arc::new(result);
        let notify = {
            let mut entry = self.inflight.entry(url_key.to_string()).or_insert_with(|| {
                Entry::InFlight(Arc::new(Notify::new()))
            });
            match &mut *entry {
                Entry::InFlight(notify) => {
                    let notify = notify.clone();
                    *entry = Entry::Completed {
                        result,
                        completed_at: std::time::Instant::now(),
                    };
                    Some(notify)
                }
                Entry::Completed { .. } => {
                    // Overwrite with new result.
                    *entry = Entry::Completed {
                        result,
                        completed_at: std::time::Instant::now(),
                    };
                    None
                }
            }
        };
        if let Some(notify) = notify {
            notify.notify_waiters();
        }
    }

    /// Remove the entry (e.g., on fetch error).
    /// Notifies any waiting callers so they can proceed with their own fetch.
    pub fn remove(&self, url_key: &str) {
        if let Some((_, entry)) = self.inflight.remove(url_key) {
            if let Entry::InFlight(notify) = entry {
                notify.notify_waiters();
            }
        }
    }

    /// Get a completed result if one exists (used after being notified from Wait state).
    pub fn get_completed(&self, url_key: &str) -> Option<Arc<DedupResult>> {
        if let Some(entry) = self.inflight.get(url_key) {
            if let Entry::Completed { result, .. } = &*entry {
                return Some(result.clone());
            }
        }
        None
    }
}

/// Produce a normalized dedup key from a URL.
///
/// Lowercases the host, sorts query parameters, strips fragments.
pub fn dedup_key(url: &str) -> String {
    use std::fmt::Write;

    let parsed = match url::Url::parse(url) {
        Ok(u) => u,
        Err(_) => return url.to_string(),
    };

    let mut key = String::new();
    let _ = write!(key, "{}://{}", parsed.scheme(), parsed.host_str().unwrap_or(""));

    if let Some(port) = parsed.port() {
        let _ = write!(key, ":{}", port);
    }

    let path = parsed.path();
    if path != "/" {
        key.push_str(path);
    } else if parsed.query().is_none() {
        key.push('/');
    }

    if let Some(query) = parsed.query() {
        // Sort query parameters for stable key.
        let mut pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        key.push('?');
        let mut first = true;
        for (k, v) in &pairs {
            if !first {
                key.push('&');
            }
            first = false;
            key.push_str(k);
            key.push('=');
            key.push_str(v);
        }
    }

    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedup_key_normalizes_query_order() {
        let k1 = dedup_key("https://example.com/api?b=2&a=1");
        let k2 = dedup_key("https://example.com/api?a=1&b=2");
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_dedup_key_strips_fragment() {
        let k1 = dedup_key("https://example.com/page#section");
        let k2 = dedup_key("https://example.com/page");
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_dedup_key_different_paths() {
        let k1 = dedup_key("https://example.com/a");
        let k2 = dedup_key("https://example.com/b");
        assert_ne!(k1, k2);
    }

    #[tokio::test]
    async fn test_dedup_proceed_on_first_request() {
        let dedup = RequestDedup::new(5000);
        let result = dedup.enter("https://example.com/page").await;
        assert!(matches!(result, DedupEntry::Proceed));
    }

    #[tokio::test]
    async fn test_dedup_returns_cached_after_complete() {
        let dedup = RequestDedup::new(5000);
        dedup.enter("https://example.com/page").await;
        dedup.complete(
            "https://example.com/page",
            DedupResult {
                url: "https://example.com/page".to_string(),
                status: 200,
                body: b"hello".to_vec(),
                content_type: Some("text/html".to_string()),
                headers: vec![],
                http_version: "HTTP/1.1".to_string(),
            },
        );
        let result = dedup.enter("https://example.com/page").await;
        assert!(matches!(result, DedupEntry::Cached(_)));
    }

    #[tokio::test]
    async fn test_dedup_removes_on_error() {
        let dedup = RequestDedup::new(5000);
        dedup.enter("https://example.com/page").await;
        dedup.remove("https://example.com/page");
        // After removal, next enter should proceed again.
        let result = dedup.enter("https://example.com/page").await;
        assert!(matches!(result, DedupEntry::Proceed));
    }

    #[test]
    fn test_dedup_disabled_when_window_zero() {
        let dedup = RequestDedup::new(0);
        assert!(!dedup.is_enabled());
    }
}
