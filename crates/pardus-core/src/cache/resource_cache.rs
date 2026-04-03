//! Cache for HTTP resources with RFC 7234 cache compliance

use super::http_cache_policy::CachePolicy;
use bytes::Bytes;
use dashmap::DashMap;
use rquest::header::HeaderMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::debug;

/// Cached resource entry
#[derive(Debug, Clone)]
pub struct CachedResource {
    pub url: String,
    pub content: Bytes,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub created_at: Instant,
    pub cache_policy: CachePolicy,
}

impl CachedResource {
    pub fn conditional_headers(&self) -> HeaderMap {
        self.cache_policy.conditional_headers()
    }

    pub fn needs_validation(&self) -> bool {
        self.cache_policy.needs_validation(self.created_at)
    }

    pub fn is_fresh(&self) -> bool {
        self.cache_policy.is_fresh(self.created_at)
    }

    pub fn can_cache(&self) -> bool {
        self.cache_policy.can_cache()
    }

    pub fn update_from_304(&mut self, new_headers: &HeaderMap) {
        self.cache_policy.update_from_304(new_headers);
        if let Some(etag) = new_headers.get("etag").and_then(|v| v.to_str().ok()) {
            self.etag = Some(etag.to_string());
        }
        if let Some(lm) = new_headers
            .get("last-modified")
            .and_then(|v| v.to_str().ok())
        {
            self.last_modified = Some(lm.to_string());
        }
        if let Some(ct) = new_headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
        {
            self.content_type = Some(ct.to_string());
        }
        self.created_at = Instant::now();
    }
}

/// Resource cache with HTTP cache compliance
pub struct ResourceCache {
    entries: DashMap<String, Arc<std::sync::RwLock<CachedResource>>>,
    max_size: usize,
    current_size: AtomicUsize,
}

impl ResourceCache {
    pub fn new(max_size_bytes: usize) -> Self {
        Self {
            entries: DashMap::new(),
            max_size: max_size_bytes,
            current_size: AtomicUsize::new(0),
        }
    }

    pub fn get(&self, url: &str) -> Option<Arc<std::sync::RwLock<CachedResource>>> {
        self.entries.get(url).map(|e| e.clone())
    }

    pub fn get_fresh(&self, url: &str) -> Option<Arc<CachedResource>> {
        self.entries.get(url).and_then(|e| {
            let entry = e.value().read().unwrap_or_else(|e| e.into_inner());
            if entry.is_fresh() {
                Some(Arc::new(entry.clone()))
            } else {
                None
            }
        })
    }

    pub fn insert(
        &self,
        url: &str,
        content: Bytes,
        content_type: Option<String>,
        headers: &HeaderMap,
    ) {
        let policy = CachePolicy::from_headers(headers);

        if !policy.can_cache() {
            return;
        }

        let size = content.len();

        if let Some(existing) = self.entries.remove(url) {
            let existing_guard = existing.1.read().unwrap_or_else(|e| e.into_inner());
            self.current_size
                .fetch_sub(existing_guard.content.len(), Ordering::SeqCst);
        }

        self.ensure_space(size);

        let etag = headers
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let last_modified = headers
            .get("last-modified")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let resource = Arc::new(std::sync::RwLock::new(CachedResource {
            url: url.to_string(),
            content,
            content_type,
            etag,
            last_modified,
            created_at: Instant::now(),
            cache_policy: policy,
        }));

        self.current_size.fetch_add(size, Ordering::SeqCst);
        self.entries.insert(url.to_string(), resource);

        debug!("cached resource: {} ({} bytes)", url, size);
    }

    pub fn update_from_304(&self, url: &str, new_headers: &HeaderMap) {
        if let Some(entry) = self.entries.get(url) {
            let mut guard = entry.write().unwrap_or_else(|e| e.into_inner());
            guard.update_from_304(new_headers);
        }
    }

    pub fn needs_revalidation(&self, url: &str) -> bool {
        if let Some(entry) = self.entries.get(url) {
            let guard = entry.read().unwrap_or_else(|e| e.into_inner());
            guard.needs_validation()
        } else {
            true
        }
    }

    pub fn invalidate(&self, url: &str) {
        if let Some((_, entry)) = self.entries.remove(url) {
            let guard = entry.read().unwrap_or_else(|e| e.into_inner());
            self.current_size
                .fetch_sub(guard.content.len(), Ordering::SeqCst);
        }
    }

    pub fn clear(&self) {
        self.entries.clear();
        self.current_size.store(0, Ordering::SeqCst);
    }

    fn ensure_space(&self, needed: usize) {
        if needed > self.max_size {
            return;
        }

        let current = self.current_size.load(Ordering::SeqCst);
        if current + needed <= self.max_size {
            return;
        }

        let expired: Vec<String> = self
            .entries
            .iter()
            .filter(|e| {
                let guard = e
                    .value()
                    .read()
                    .unwrap_or_else(|poison| poison.into_inner());
                guard.needs_validation() && !guard.cache_policy.has_validator
            })
            .map(|e| e.key().clone())
            .collect();

        for url in expired {
            if self.current_size.load(Ordering::SeqCst) + needed <= self.max_size {
                break;
            }
            if let Some((_, entry)) = self.entries.remove(&url) {
                let guard = entry.read().unwrap_or_else(|e| e.into_inner());
                self.current_size
                    .fetch_sub(guard.content.len(), Ordering::SeqCst);
            }
        }
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.entries.len(),
            size_bytes: self.current_size.load(Ordering::SeqCst),
            max_size: self.max_size,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub entries: usize,
    pub size_bytes: usize,
    pub max_size: usize,
}
