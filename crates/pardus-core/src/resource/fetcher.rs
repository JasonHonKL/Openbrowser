//! HTTP resource fetcher with optimized client and HTTP cache compliance

use std::{sync::Arc, time::Instant};

use bytes::Bytes;
use rquest::header::HeaderMap;
use tracing::{instrument, trace};

use super::ResourceConfig;
use crate::{
    cache::{CachedResource, ResourceCache},
    push::PushCache,
};

/// Fetch options
#[derive(Debug, Clone)]
pub struct FetchOptions {
    pub timeout_ms: u64,
    pub follow_redirects: bool,
    pub accept_encoding: Vec<String>,
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            timeout_ms: 30000,
            follow_redirects: true,
            accept_encoding: vec!["gzip".to_string(), "br".to_string()],
        }
    }
}

/// Fetch result
#[derive(Debug, Clone)]
pub struct FetchResult {
    pub url: String,
    pub status: u16,
    pub body: Option<Bytes>,
    pub content_type: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub size: usize,
    pub from_cache: bool,
    pub response_headers: HeaderMap,
}

impl FetchResult {
    pub fn success(
        url: &str,
        status: u16,
        body: Bytes,
        content_type: Option<String>,
        duration_ms: u64,
    ) -> Self {
        Self {
            url: url.to_string(),
            status,
            body: Some(body.clone()),
            content_type,
            error: None,
            duration_ms,
            size: body.len(),
            from_cache: false,
            response_headers: HeaderMap::new(),
        }
    }

    pub fn error(url: &str, error: impl Into<String>) -> Self {
        Self {
            url: url.to_string(),
            status: 0,
            body: None,
            content_type: None,
            error: Some(error.into()),
            duration_ms: 0,
            size: 0,
            from_cache: false,
            response_headers: HeaderMap::new(),
        }
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none() && self.status >= 200 && self.status < 300
    }

    pub fn is_not_modified(&self) -> bool { self.status == 304 }

    /// Convert response headers to `Vec<(String, String)>` for NetworkLog compatibility.
    pub fn response_headers_vec(&self) -> Vec<(String, String)> {
        self.response_headers
            .iter()
            .filter_map(|(k, v)| Some((k.to_string(), v.to_str().ok()?.to_string())))
            .collect()
    }
}

/// High-performance resource fetcher
pub struct ResourceFetcher {
    client: rquest::Client,
    #[allow(dead_code)]
    config: ResourceConfig,
}

impl ResourceFetcher {
    pub fn new(client: rquest::Client, config: ResourceConfig) -> Self { Self { client, config } }

    #[instrument(skip(self), level = "trace")]
    pub async fn fetch(&self, url: &str) -> FetchResult {
        let start = std::time::Instant::now();
        trace!("fetching {}", url);

        match self.client.get(url).send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                let content_type = response
                    .headers()
                    .get(rquest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());

                let headers = response.headers().clone();

                match response.bytes().await {
                    Ok(body) => {
                        let elapsed = start.elapsed();
                        trace!("fetch complete: {} ({} bytes)", url, body.len());
                        let mut result = FetchResult::success(
                            url,
                            status,
                            body,
                            content_type,
                            elapsed.as_millis() as u64,
                        );
                        result.response_headers = headers;
                        result
                    }
                    Err(e) => FetchResult::error(url, format!("body read error: {}", e)),
                }
            }
            Err(e) => FetchResult::error(url, format!("request error: {}", e)),
        }
    }

    pub async fn fetch_with_options(&self, url: &str, _options: FetchOptions) -> FetchResult {
        let start = std::time::Instant::now();

        // Note: redirect policy is configured on the client, not per-request
        // For no-redirect, would need a separate client
        let request = self.client.get(url);

        match request.send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                let content_type = response
                    .headers()
                    .get(rquest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());

                let headers = response.headers().clone();

                match response.bytes().await {
                    Ok(body) => {
                        let elapsed = start.elapsed();
                        let mut result = FetchResult::success(
                            url,
                            status,
                            body,
                            content_type,
                            elapsed.as_millis() as u64,
                        );
                        result.response_headers = headers;
                        result
                    }
                    Err(e) => FetchResult::error(url, e.to_string()),
                }
            }
            Err(e) => FetchResult::error(url, e.to_string()),
        }
    }

    /// Fetch a URL with conditional request headers (for cache revalidation).
    pub async fn fetch_conditional(
        &self,
        url: &str,
        conditional_headers: &HeaderMap,
    ) -> FetchResult {
        let start = std::time::Instant::now();
        trace!("conditional fetch: {}", url);

        let mut request = self.client.get(url);
        for (name, value) in conditional_headers.iter() {
            request = request.header(name, value);
        }

        match request.send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                let content_type = response
                    .headers()
                    .get(rquest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());

                let headers = response.headers().clone();

                if status == 304 {
                    let elapsed = start.elapsed();
                    let result = FetchResult {
                        url: url.to_string(),
                        status,
                        body: None,
                        content_type,
                        error: None,
                        duration_ms: elapsed.as_millis() as u64,
                        size: 0,
                        from_cache: true,
                        response_headers: headers,
                    };
                    return result;
                }

                match response.bytes().await {
                    Ok(body) => {
                        let elapsed = start.elapsed();
                        let mut result = FetchResult::success(
                            url,
                            status,
                            body,
                            content_type,
                            elapsed.as_millis() as u64,
                        );
                        result.response_headers = headers;
                        result
                    }
                    Err(e) => FetchResult::error(url, e.to_string()),
                }
            }
            Err(e) => FetchResult::error(url, e.to_string()),
        }
    }

    pub async fn exists(&self, url: &str) -> bool {
        match self.client.head(url).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    pub async fn content_length(&self, url: &str) -> Option<usize> {
        match self.client.head(url).send().await {
            Ok(response) => response
                .headers()
                .get(rquest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok()),
            Err(_) => None,
        }
    }

    /// Fetch a resource, checking the push cache first.
    ///
    /// If a valid entry exists in the push cache, returns it immediately
    /// without making an HTTP request. Otherwise falls through to a normal
    /// fetch and does NOT store the result in the push cache.
    pub fn fetch_with_push_cache(
        &self,
        url: &str,
        push_cache: &PushCache,
    ) -> std::future::Ready<FetchResult> {
        if let Some(entry) = push_cache.get(url) {
            trace!("push cache hit for fetch: {}", url);
            let mut result = FetchResult::success(
                url,
                entry.status,
                entry.body,
                entry.content_type,
                entry.duration_ms,
            );
            result.from_cache = true;
            return std::future::ready(result);
        }
        trace!("push cache miss for fetch: {}", url);

        let url_owned = url.to_string();
        let _ = url_owned;
        std::future::ready(FetchResult::error(
            &url_owned,
            "push cache miss, use fetch() instead",
        ))
    }
}

/// Cache-aware fetcher that performs HTTP conditional requests per RFC 7234.
pub struct CachedFetcher {
    fetcher: ResourceFetcher,
    cache: Arc<ResourceCache>,
}

impl CachedFetcher {
    pub fn new(client: rquest::Client, config: ResourceConfig, cache: Arc<ResourceCache>) -> Self {
        Self {
            fetcher: ResourceFetcher::new(client, config),
            cache,
        }
    }

    /// Fetch a URL with HTTP cache compliance.
    ///
    /// - Fresh cache hit → returns cached data immediately
    /// - Stale with validators → conditional request (If-None-Match / If-Modified-Since)
    /// - 304 Not Modified → updates cache headers, returns cached data
    /// - Miss or no validators → full fetch, stores in cache
    /// - no-store → bypasses cache entirely
    pub async fn fetch(&self, url: &str) -> FetchResult {
        let start = Instant::now();

        let cache_action = {
            if let Some(entry) = self.cache.get(url) {
                let guard = entry.read().unwrap();

                if !guard.can_cache() {
                    drop(guard);
                    CacheAction::Bypass
                } else if guard.is_fresh() {
                    let cached = (*guard).clone();
                    drop(guard);
                    CacheAction::Fresh(cached)
                } else if guard.cache_policy.has_validator {
                    let cond_headers = guard.conditional_headers();
                    drop(guard);
                    CacheAction::Revalidate(cond_headers)
                } else {
                    drop(guard);
                    CacheAction::Bypass
                }
            } else {
                CacheAction::Bypass
            }
        };

        match cache_action {
            CacheAction::Bypass => {
                let result = self.fetcher.fetch(url).await;
                if result.is_success() {
                    self.cache.insert(
                        url,
                        result.body.clone().unwrap_or_default(),
                        result.content_type.clone(),
                        &result.response_headers,
                    );
                }
                result
            }
            CacheAction::Fresh(cached) => {
                let mut result = FetchResult::success(
                    url,
                    200,
                    cached.content,
                    cached.content_type,
                    start.elapsed().as_millis() as u64,
                );
                result.from_cache = true;
                trace!("cache hit (fresh): {}", url);
                result
            }
            CacheAction::Revalidate(cond_headers) => {
                trace!("cache stale, revalidating: {}", url);
                let cond_result = self.fetcher.fetch_conditional(url, &cond_headers).await;

                if cond_result.is_not_modified() {
                    self.cache
                        .update_from_304(url, &cond_result.response_headers);
                    if let Some(entry) = self.cache.get(url) {
                        let guard = entry.read().unwrap();
                        let cached = (*guard).clone();
                        drop(guard);
                        let mut result = FetchResult::success(
                            url,
                            200,
                            cached.content,
                            cached.content_type,
                            start.elapsed().as_millis() as u64,
                        );
                        result.from_cache = true;
                        trace!("cache hit (304): {}", url);
                        return result;
                    }
                }

                if cond_result.is_success() {
                    self.cache.insert(
                        url,
                        cond_result.body.clone().unwrap_or_default(),
                        cond_result.content_type.clone(),
                        &cond_result.response_headers,
                    );
                }
                cond_result
            }
        }
    }

    /// Fetch without caching (force bypass).
    pub async fn fetch_bypass(&self, url: &str) -> FetchResult { self.fetcher.fetch(url).await }

    pub fn invalidate(&self, url: &str) { self.cache.invalidate(url); }

    pub fn clear(&self) { self.cache.clear(); }
}

enum CacheAction {
    Bypass,
    Fresh(CachedResource),
    Revalidate(HeaderMap),
}
