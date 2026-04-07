//! Fetch operation for deno_core.
//!
//! Provides JavaScript fetch API via rquest with timeout, body size limits,
//! and HTTP cache compliance (ETag, Last-Modified, conditional requests).

use deno_core::*;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::OnceLock;

use crate::cache::ResourceCache;
use crate::url_policy::UrlPolicy;

const OP_FETCH_MAX_BODY_SIZE: usize = 1_048_576;

/// Per-runtime fetch policy, stored in OpState.
pub struct FetchPolicy {
    pub blocked: bool,
}

fn get_fetch_client() -> &'static rquest::Client {
    crate::http::client::fetch_client()
}

fn get_fetch_cache() -> &'static Arc<ResourceCache> {
    static CACHE: OnceLock<Arc<ResourceCache>> = OnceLock::new();
    CACHE.get_or_init(|| Arc::new(ResourceCache::new(100 * 1024 * 1024)))
}

/// Get the URL policy for JS fetch operations.
/// Uses a strict default policy that blocks SSRF attacks.
fn get_fetch_url_policy() -> &'static UrlPolicy {
    static POLICY: OnceLock<UrlPolicy> = OnceLock::new();
    POLICY.get_or_init(UrlPolicy::default)
}

/// Validate a URL for safe fetching.
/// Blocks SSRF attacks by rejecting:
/// - Non-HTTP(S) schemes (file://, ftp://, etc.)
/// - Localhost and loopback addresses
/// - Private IP ranges (10.x, 172.16-31.x, 192.168.x)
/// - Link-local addresses (169.254.x.x)
/// - Cloud metadata endpoints (169.254.169.254, metadata.google.internal)
fn is_url_safe(url: &str) -> bool {
    get_fetch_url_policy().validate(url).is_ok()
}

fn build_request(
    client: &rquest::Client,
    method: &str,
    url: &str,
    headers: &HashMap<String, String>,
    body: &Option<String>,
) -> rquest::RequestBuilder {
    let req = match method {
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        "PATCH" => client.patch(url),
        "HEAD" => client.head(url),
        _ => client.get(url),
    };

    let mut req = req;
    for (k, v) in headers {
        req = req.header(k, v);
    }
    if let Some(body) = body {
        req = req.body(body.clone());
    }
    req
}

fn extract_response_headers(resp: &rquest::Response) -> (u16, String, HashMap<String, String>) {
    let status = resp.status().as_u16();
    let status_text = resp
        .status()
        .canonical_reason()
        .unwrap_or("")
        .to_string();
    let headers: HashMap<String, String> = resp
        .headers()
        .iter()
        .filter_map(|(k, v)| Some((k.to_string(), v.to_str().ok()?.to_string())))
        .collect();
    (status, status_text, headers)
}

async fn read_body_with_limit(resp: rquest::Response, max_size: usize) -> String {
    let mut bytes = Vec::with_capacity(1024.min(max_size));
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(data) => {
                if bytes.len() + data.len() > max_size {
                    bytes.truncate(max_size);
                    break;
                }
                bytes.extend_from_slice(&data);
            }
            Err(_) => break,
        }
    }

    String::from_utf8_lossy(&bytes).to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FetchCacheMode {
    Default,
    NoStore,
    ForceCache,
    OnlyIfCached,
}

impl FetchCacheMode {
    fn from_str(s: &str) -> Self {
        match s {
            "no-store" => Self::NoStore,
            "force-cache" => Self::ForceCache,
            "only-if-cached" => Self::OnlyIfCached,
            _ => Self::Default,
        }
    }
}

#[op2]
#[serde]
pub async fn op_fetch(
    op_state: Rc<RefCell<OpState>>,
    #[serde] args: FetchArgs,
) -> FetchResult {
    // Sandbox: block JS fetch if this runtime has fetch disabled
    let blocked = op_state.borrow().try_borrow::<FetchPolicy>()
        .map(|p| p.blocked)
        .unwrap_or(false);
    if blocked {
        return FetchResult {
            ok: false,
            status: 403,
            status_text: "Blocked: fetch is disabled by sandbox policy".to_string(),
            headers: HashMap::new(),
            body: String::new(),
        };
    }

    // Validate URL against SSRF protection policy
    if !is_url_safe(&args.url) {
        return FetchResult {
            ok: false,
            status: 0,
            status_text: "Blocked: URL blocked by security policy (SSRF protection)".to_string(),
            headers: HashMap::new(),
            body: String::new(),
        };
    }

    let client = get_fetch_client();
    let cache_mode = args.cache.as_deref()
        .map(FetchCacheMode::from_str)
        .unwrap_or(FetchCacheMode::Default);

    let is_get = args.method.eq_ignore_ascii_case("get");
    let cache = get_fetch_cache();

    if is_get && cache_mode != FetchCacheMode::NoStore {
        if let Some(entry) = cache.get(&args.url) {
            let guard = entry.read().unwrap();

            match cache_mode {
                FetchCacheMode::ForceCache | FetchCacheMode::OnlyIfCached => {
                    if guard.is_fresh() || cache_mode == FetchCacheMode::ForceCache {
                        let body = String::from_utf8_lossy(&guard.content).to_string();
                        let status = 200u16;
                        let mut headers: HashMap<String, String> = guard.content_type
                            .as_ref()
                            .map(|ct| vec![("content-type".to_string(), ct.clone())])
                            .unwrap_or_default()
                            .into_iter()
                            .collect();
                        headers.insert("x-cache".to_string(), "hit".to_string());
                        drop(guard);
                        return FetchResult {
                            ok: true,
                            status,
                            status_text: "OK".to_string(),
                            headers,
                            body,
                        };
                    }
                    if cache_mode == FetchCacheMode::OnlyIfCached {
                        drop(guard);
                        return FetchResult {
                            ok: false,
                            status: 504,
                            status_text: "Gateway Timeout (cache miss)".to_string(),
                            headers: HashMap::new(),
                            body: String::new(),
                        };
                    }
                }
                FetchCacheMode::Default => {
                    if guard.is_fresh() {
                        let body = String::from_utf8_lossy(&guard.content).to_string();
                        let mut headers: HashMap<String, String> = guard.content_type
                            .as_ref()
                            .map(|ct| vec![("content-type".to_string(), ct.clone())])
                            .unwrap_or_default()
                            .into_iter()
                            .collect();
                        headers.insert("x-cache".to_string(), "hit".to_string());
                        drop(guard);
                        return FetchResult {
                            ok: true,
                            status: 200,
                            status_text: "OK".to_string(),
                            headers,
                            body,
                        };
                    }

                    if guard.cache_policy.has_validator {
                        let cond_headers = guard.conditional_headers();
                        drop(guard);

                        let mut request = client.get(&args.url);
                        for (name, value) in cond_headers.iter() {
                            request = request.header(name, value);
                        }
                        for (k, v) in &args.headers {
                            let k_lower = k.to_lowercase();
                            if k_lower != "if-none-match" && k_lower != "if-modified-since" {
                                request = request.header(k.as_str(), v.as_str());
                            }
                        }

                        match request.send().await {
                            Ok(resp) => {
                                let status = resp.status().as_u16();
                                let status_text = resp.status().canonical_reason().unwrap_or("").to_string();
                                let mut headers: HashMap<String, String> = resp.headers()
                                    .iter()
                                    .filter_map(|(k, v)| Some((k.to_string(), v.to_str().ok()?.to_string())))
                                    .collect();

                                if status == 304 {
                                    cache.update_from_304(&args.url, resp.headers());
                                    if let Some(entry) = cache.get(&args.url) {
                                        let guard = entry.read().unwrap();
                                        let body = String::from_utf8_lossy(&guard.content).to_string();
                                        headers.insert("x-cache".to_string(), "hit (304)".to_string());
                                        drop(guard);
                                        return FetchResult {
                                            ok: true,
                                            status: 200,
                                            status_text: "OK".to_string(),
                                            headers,
                                            body,
                                        };
                                    }
                                }

                                let body = read_body_with_limit(resp, OP_FETCH_MAX_BODY_SIZE).await;
                                if (200..300).contains(&status) {
                                    cache.insert(&args.url, bytes::Bytes::from(body.clone()), None, &rquest::header::HeaderMap::new());
                                }
                                headers.insert("x-cache".to_string(), "miss".to_string());
                                return FetchResult {
                                    ok: (200..300).contains(&status),
                                    status,
                                    status_text,
                                    headers,
                                    body,
                                };
                            }
                            Err(_) => {}
                        }
                    }
                    // Fall through to full fetch
                }
                FetchCacheMode::NoStore => {}
            }
        }
    }

    let req = build_request(&client, &args.method, &args.url, &args.headers, &args.body);

    match req.send().await {
        Ok(resp) => {
            let (status, status_text, mut headers) = extract_response_headers(&resp);

            let content_length: Option<usize> = resp
                .headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok());

            if content_length.is_some_and(|len| len > OP_FETCH_MAX_BODY_SIZE) {
                return FetchResult {
                    ok: status >= 200 && status < 300,
                    status,
                    status_text,
                    headers,
                    body: String::new(),
                };
            }

            let body = read_body_with_limit(resp, OP_FETCH_MAX_BODY_SIZE).await;

            if is_get && cache_mode != FetchCacheMode::NoStore && (200..300).contains(&status) {
                cache.insert(
                    &args.url,
                    bytes::Bytes::from(body.clone()),
                    headers.get("content-type").cloned(),
                    &rquest::header::HeaderMap::new(),
                );
            }

            headers.insert("x-cache".to_string(), "miss".to_string());

            FetchResult {
                ok: status >= 200 && status < 300,
                status,
                status_text,
                headers,
                body,
            }
        }
        Err(_) => FetchResult {
            ok: false,
            status: 0,
            status_text: "Network Error".to_string(),
            headers: HashMap::new(),
            body: String::new(),
        },
    }
}

#[derive(Deserialize)]
pub struct FetchArgs {
    pub url: String,
    pub method: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
    #[serde(default)]
    pub cache: Option<String>,
}

#[derive(Serialize)]
pub struct FetchResult {
    pub ok: bool,
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FetchRequest {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
}

fn default_method() -> String {
    "GET".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FetchResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub ok: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_url_safe_allows_public_urls() {
        assert!(is_url_safe("https://example.com"));
        assert!(is_url_safe("http://example.com/path?query=1"));
        assert!(is_url_safe("https://api.example.com:8080/v1/data"));
    }

    #[test]
    fn test_is_url_safe_blocks_file_scheme() {
        assert!(!is_url_safe("file:///etc/passwd"));
        assert!(!is_url_safe("file://localhost/Users/test"));
    }

    #[test]
    fn test_is_url_safe_blocks_ftp_scheme() {
        assert!(!is_url_safe("ftp://ftp.example.com/file"));
    }

    #[test]
    fn test_is_url_safe_blocks_data_scheme() {
        assert!(!is_url_safe("data:text/html,<script>alert(1)</script>"));
    }

    #[test]
    fn test_is_url_safe_blocks_javascript_scheme() {
        assert!(!is_url_safe("javascript:alert(1)"));
    }

    #[test]
    fn test_is_url_safe_blocks_localhost() {
        assert!(!is_url_safe("http://localhost/admin"));
        assert!(!is_url_safe("http://LOCALHOST/admin"));
        assert!(!is_url_safe("http://localhost.localdomain/"));
    }

    #[test]
    fn test_is_url_safe_blocks_loopback() {
        assert!(!is_url_safe("http://127.0.0.1/admin"));
        assert!(!is_url_safe("http://127.0.0.1:8080/api"));
        assert!(!is_url_safe("http://[::1]/admin"));
        assert!(!is_url_safe("http://[0:0:0:0:0:0:0:1]/"));
    }

    #[test]
    fn test_is_url_safe_blocks_private_ips() {
        // 10.0.0.0/8
        assert!(!is_url_safe("http://10.0.0.1/"));
        assert!(!is_url_safe("http://10.255.255.255/"));

        // 172.16.0.0/12
        assert!(!is_url_safe("http://172.16.0.1/"));
        assert!(!is_url_safe("http://172.31.255.255/"));
        // 172.15.x.x is public
        assert!(is_url_safe("http://172.15.0.1/"));
        // 172.32.x.x is public
        assert!(is_url_safe("http://172.32.0.1/"));

        // 192.168.0.0/16
        assert!(!is_url_safe("http://192.168.0.1/"));
        assert!(!is_url_safe("http://192.168.1.1/"));
        assert!(!is_url_safe("http://192.168.255.255/"));
    }

    #[test]
    fn test_is_url_safe_allows_public_ips() {
        assert!(is_url_safe("http://8.8.8.8/"));
        assert!(is_url_safe("http://1.1.1.1/"));
        assert!(is_url_safe("http://93.184.216.34/")); // example.com IP
    }

    #[test]
    fn test_is_url_safe_blocks_cloud_metadata() {
        // AWS/GCP/Azure metadata endpoint
        assert!(!is_url_safe("http://169.254.169.254/latest/meta-data/"));
        assert!(!is_url_safe("http://metadata.google.internal/computeMetadata/v1/"));
        assert!(!is_url_safe("http://metadata.azure.internal/"));
        // Alibaba metadata
        assert!(!is_url_safe("http://100.100.100.200/latest/meta-data/"));
    }

    #[test]
    fn test_is_url_safe_blocks_link_local() {
        assert!(!is_url_safe("http://169.254.1.1/"));
        assert!(!is_url_safe("http://169.254.100.50/"));
    }

    #[test]
    fn test_is_url_safe_blocks_invalid_urls() {
        assert!(!is_url_safe("not-a-url"));
        assert!(!is_url_safe("://no-scheme.com"));
        assert!(!is_url_safe(""));
    }

    #[test]
    fn test_is_url_safe_case_insensitive_scheme() {
        assert!(is_url_safe("HTTPS://example.com"));
        assert!(is_url_safe("HtTp://example.com"));
    }

    #[test]
    fn test_is_url_safe_ipv6_addresses() {
        // Public IPv6 should work
        assert!(is_url_safe("http://[2001:4860:4860::8888]/"));

        // Loopback IPv6 blocked
        assert!(!is_url_safe("http://[::1]/"));

        // Link-local IPv6 blocked
        assert!(!is_url_safe("http://[fe80::1]/"));

        // Unique local (private) IPv6 blocked
        assert!(!is_url_safe("http://[fc00::1]/"));
        assert!(!is_url_safe("http://[fd00::1]/"));
    }

    // ==================== FetchCacheMode Tests ====================

    #[test]
    fn test_fetch_cache_mode_from_str() {
        assert_eq!(FetchCacheMode::from_str("no-store"), FetchCacheMode::NoStore);
        assert_eq!(FetchCacheMode::from_str("force-cache"), FetchCacheMode::ForceCache);
        assert_eq!(FetchCacheMode::from_str("only-if-cached"), FetchCacheMode::OnlyIfCached);
        assert_eq!(FetchCacheMode::from_str("default"), FetchCacheMode::Default);
        assert_eq!(FetchCacheMode::from_str(""), FetchCacheMode::Default);
        assert_eq!(FetchCacheMode::from_str("invalid"), FetchCacheMode::Default);
    }

    #[test]
    fn test_fetch_cache_mode_equality() {
        assert_eq!(FetchCacheMode::Default, FetchCacheMode::Default);
        assert_ne!(FetchCacheMode::NoStore, FetchCacheMode::ForceCache);
        assert_ne!(FetchCacheMode::Default, FetchCacheMode::OnlyIfCached);
    }

    // ==================== FetchArgs Deserialization Tests ====================

    #[test]
    fn test_fetch_args_defaults() {
        let json = r#"{"url":"https://example.com","method":""}"#;
        let args: FetchArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.url, "https://example.com");
        assert!(args.headers.is_empty());
        assert!(args.body.is_none());
        assert!(args.cache.is_none());
    }

    #[test]
    fn test_fetch_args_full() {
        let json = r#"{"url":"https://api.example.com/data","method":"POST","headers":{"content-type":"application/json"},"body":"{\"key\":\"value\"}","cache":"no-store"}"#;
        let args: FetchArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.url, "https://api.example.com/data");
        assert_eq!(args.method, "POST");
        assert_eq!(args.headers.get("content-type").unwrap(), "application/json");
        assert_eq!(args.body, Some("{\"key\":\"value\"}".to_string()));
        assert_eq!(args.cache, Some("no-store".to_string()));
    }

    #[test]
    fn test_fetch_args_empty_body() {
        let json = r#"{"url":"https://example.com","method":"GET","body":null}"#;
        let args: FetchArgs = serde_json::from_str(json).unwrap();
        assert!(args.body.is_none());
    }

    // ==================== FetchResult Serialization Tests ====================

    #[test]
    fn test_fetch_result_serialization() {
        let result = FetchResult {
            ok: true,
            status: 200,
            status_text: "OK".to_string(),
            headers: {
                let mut h = HashMap::new();
                h.insert("content-type".to_string(), "text/html".to_string());
                h
            },
            body: "Hello".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"status\":200"));
        assert!(json.contains("\"body\":\"Hello\""));
    }

    #[test]
    fn test_fetch_result_error() {
        let result = FetchResult {
            ok: false,
            status: 403,
            status_text: "Forbidden".to_string(),
            headers: HashMap::new(),
            body: String::new(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"ok\":false"));
        assert!(json.contains("\"status\":403"));
    }

    #[test]
    fn test_fetch_result_network_error() {
        let result = FetchResult {
            ok: false,
            status: 0,
            status_text: "Network Error".to_string(),
            headers: HashMap::new(),
            body: String::new(),
        };
        assert_eq!(result.status, 0);
        assert!(!result.ok);
    }

    // ==================== FetchRequest Tests ====================

    #[test]
    fn test_fetch_request_default_method() {
        assert_eq!(default_method(), "GET");
    }

    #[test]
    fn test_fetch_request_deserialization() {
        let json = r#"{"url":"https://example.com"}"#;
        let req: FetchRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.url, "https://example.com");
        assert_eq!(req.method, "GET");
        assert!(req.headers.is_empty());
        assert!(req.body.is_none());
    }

    #[test]
    fn test_fetch_request_with_method() {
        let json = r#"{"url":"https://example.com","method":"POST","body":"data"}"#;
        let req: FetchRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.body, Some("data".to_string()));
    }

    // ==================== FetchResponse Tests ====================

    #[test]
    fn test_fetch_response_serialization_roundtrip() {
        let resp = FetchResponse {
            status: 200,
            status_text: "OK".to_string(),
            headers: {
                let mut h = HashMap::new();
                h.insert("content-type".to_string(), "text/plain".to_string());
                h
            },
            body: "Hello World".to_string(),
            ok: true,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: FetchResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.status, 200);
        assert_eq!(back.body, "Hello World");
        assert!(back.ok);
    }

    // ==================== FetchPolicy Tests ====================

    #[test]
    fn test_fetch_policy_default() {
        let policy = FetchPolicy { blocked: false };
        assert!(!policy.blocked);
    }

    #[test]
    fn test_fetch_policy_blocked() {
        let policy = FetchPolicy { blocked: true };
        assert!(policy.blocked);
    }

    // ==================== build_request Method Tests ====================

    #[test]
    fn test_build_request_get() {
        let client = rquest::Client::new();
        let req = build_request(&client, "GET", "https://example.com", &HashMap::new(), &None);
        let built = req.build().unwrap();
        assert_eq!(built.method().as_str(), "GET");
    }

    #[test]
    fn test_build_request_post() {
        let client = rquest::Client::new();
        let req = build_request(&client, "POST", "https://example.com", &HashMap::new(), &Some("body".to_string()));
        let built = req.build().unwrap();
        assert_eq!(built.method().as_str(), "POST");
    }

    #[test]
    fn test_build_request_put() {
        let client = rquest::Client::new();
        let req = build_request(&client, "PUT", "https://example.com", &HashMap::new(), &None);
        let built = req.build().unwrap();
        assert_eq!(built.method().as_str(), "PUT");
    }

    #[test]
    fn test_build_request_delete() {
        let client = rquest::Client::new();
        let req = build_request(&client, "DELETE", "https://example.com/resource", &HashMap::new(), &None);
        let built = req.build().unwrap();
        assert_eq!(built.method().as_str(), "DELETE");
    }

    #[test]
    fn test_build_request_patch() {
        let client = rquest::Client::new();
        let req = build_request(&client, "PATCH", "https://example.com", &HashMap::new(), &None);
        let built = req.build().unwrap();
        assert_eq!(built.method().as_str(), "PATCH");
    }

    #[test]
    fn test_build_request_head() {
        let client = rquest::Client::new();
        let req = build_request(&client, "HEAD", "https://example.com", &HashMap::new(), &None);
        let built = req.build().unwrap();
        assert_eq!(built.method().as_str(), "HEAD");
    }

    #[test]
    fn test_build_request_unknown_defaults_to_get() {
        let client = rquest::Client::new();
        let req = build_request(&client, "OPTIONS", "https://example.com", &HashMap::new(), &None);
        let built = req.build().unwrap();
        assert_eq!(built.method().as_str(), "GET");
    }

    #[test]
    fn test_build_request_with_headers() {
        let client = rquest::Client::new();
        let mut headers = HashMap::new();
        headers.insert("accept".to_string(), "application/json".to_string());
        headers.insert("x-custom".to_string(), "value".to_string());
        let req = build_request(&client, "GET", "https://example.com", &headers, &None);
        let built = req.build().unwrap();
        assert_eq!(built.headers().get("accept").unwrap(), "application/json");
        assert_eq!(built.headers().get("x-custom").unwrap(), "value");
    }

    // ==================== extract_response_headers Tests ====================

    // Note: These would need a real HTTP response. Test the types instead.

    #[test]
    fn test_op_fetch_max_body_size() {
        assert_eq!(OP_FETCH_MAX_BODY_SIZE, 1_048_576);
    }
}
