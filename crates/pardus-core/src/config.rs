use std::path::PathBuf;

use crate::csp::CspPolicySet;
use crate::sandbox::SandboxPolicy;
#[cfg(feature = "tls-pinning")]
use crate::tls::CertificatePinningConfig;
use crate::url_policy::UrlPolicy;

/// Proxy configuration for HTTP/HTTPS/SOCKS5 traffic.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ProxyConfig {
    /// HTTP proxy URL (e.g., "http://proxy.example.com:8080")
    pub http_proxy: Option<String>,
    /// HTTPS proxy URL (e.g., "http://proxy.example.com:8080")
    pub https_proxy: Option<String>,
    /// All traffic proxy (e.g., "socks5://127.0.0.1:1080" or "http://proxy.example.com:8080")
    pub all_proxy: Option<String>,
    /// Comma-separated list of hosts to bypass proxy
    pub no_proxy: Option<String>,
}

impl ProxyConfig {
    /// Create a new empty proxy config
    pub fn new() -> Self {
        Self::default()
    }

    /// Set HTTP proxy URL
    pub fn with_http_proxy(mut self, url: impl Into<String>) -> Self {
        self.http_proxy = Some(url.into());
        self
    }

    /// Set HTTPS proxy URL
    pub fn with_https_proxy(mut self, url: impl Into<String>) -> Self {
        self.https_proxy = Some(url.into());
        self
    }

    /// Set all traffic proxy URL
    pub fn with_all_proxy(mut self, url: impl Into<String>) -> Self {
        self.all_proxy = Some(url.into());
        self
    }

    /// Set no_proxy list (comma-separated)
    pub fn with_no_proxy(mut self, hosts: impl Into<String>) -> Self {
        self.no_proxy = Some(hosts.into());
        self
    }

    /// Check if any proxy is configured
    pub fn is_configured(&self) -> bool {
        self.http_proxy.is_some() || self.https_proxy.is_some() || self.all_proxy.is_some()
    }

    /// Load proxy configuration from environment variables
    /// Respects: HTTP_PROXY, http_proxy, HTTPS_PROXY, https_proxy,
    ///           ALL_PROXY, all_proxy, NO_PROXY, no_proxy
    pub fn from_env() -> Self {
        Self {
            http_proxy: std::env::var("HTTP_PROXY")
                .or_else(|_| std::env::var("http_proxy"))
                .ok(),
            https_proxy: std::env::var("HTTPS_PROXY")
                .or_else(|_| std::env::var("https_proxy"))
                .ok(),
            all_proxy: std::env::var("ALL_PROXY")
                .or_else(|_| std::env::var("all_proxy"))
                .ok(),
            no_proxy: std::env::var("NO_PROXY")
                .or_else(|_| std::env::var("no_proxy"))
                .ok(),
        }
    }

    /// Merge environment variables with explicit config (explicit takes precedence)
    pub fn merge_env(mut self) -> Self {
        let env = Self::from_env();
        if self.http_proxy.is_none() && env.http_proxy.is_some() {
            self.http_proxy = env.http_proxy;
        }
        if self.https_proxy.is_none() && env.https_proxy.is_some() {
            self.https_proxy = env.https_proxy;
        }
        if self.all_proxy.is_none() && env.all_proxy.is_some() {
            self.all_proxy = env.all_proxy;
        }
        if self.no_proxy.is_none() && env.no_proxy.is_some() {
            self.no_proxy = env.no_proxy;
        }
        self
    }
}

/// Connection pool configuration for HTTP client.
#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    /// Maximum idle connections per host (default: 32)
    pub max_idle_per_host: usize,
    /// Idle connection timeout in seconds (default: 90)
    pub idle_timeout_secs: u64,
    /// TCP keepalive interval in seconds (default: 60)
    pub tcp_keepalive_secs: u64,
    /// Enable HTTP/2 (default: true)
    pub enable_http2: bool,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_idle_per_host: 32,
            idle_timeout_secs: 90,
            tcp_keepalive_secs: 60,
            enable_http2: true,
        }
    }
}

fn default_cache_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let p = PathBuf::from(home).join("Library/Caches/pardus-browser");
            if p.parent().map_or(false, |d| d.exists()) {
                return p;
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
            return PathBuf::from(xdg).join("pardus-browser");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(".cache/pardus-browser");
        }
    }
    PathBuf::from("/tmp/pardus-browser")
}

/// Configuration for HTTP/2 push simulation.
#[derive(Debug, Clone)]
pub struct PushConfig {
    /// Enable client-side push simulation (default: true).
    pub enable_push: bool,
    /// Maximum number of resources in the push cache (default: 32).
    pub max_push_resources: usize,
    /// Push cache TTL in seconds (default: 30).
    pub push_cache_ttl_secs: u64,
}

impl Default for PushConfig {
    fn default() -> Self {
        Self {
            enable_push: true,
            max_push_resources: 32,
            push_cache_ttl_secs: 30,
        }
    }
}

/// CSP enforcement configuration.
#[derive(Debug, Clone)]
pub struct CspConfig {
    /// Enable CSP enforcement from server headers.
    /// When `false` (default), CSP headers are ignored entirely.
    pub enforce_csp: bool,
    /// Log report-only violations even when enforce mode is active.
    pub log_report_only: bool,
    /// Override: ignore server CSP headers and use this raw policy string instead.
    /// Useful for AI agents that want to enforce their own CSP.
    pub override_policy: Option<String>,
}

impl Default for CspConfig {
    fn default() -> Self {
        Self {
            enforce_csp: false,
            log_report_only: true,
            override_policy: None,
        }
    }
}

impl CspConfig {
    /// Create a CSP config that enforces server headers.
    pub fn enforcing() -> Self {
        Self {
            enforce_csp: true,
            log_report_only: true,
            override_policy: None,
        }
    }

    /// Create a CSP config with a custom policy string.
    pub fn with_policy(policy: impl Into<String>) -> Self {
        Self {
            enforce_csp: true,
            log_report_only: true,
            override_policy: Some(policy.into()),
        }
    }

    /// Parse the effective policy from headers, respecting override.
    pub fn parse_policy(&self, headers: &[(String, String)]) -> Option<CspPolicySet> {
        if !self.enforce_csp {
            return None;
        }
        if let Some(ref policy_str) = self.override_policy {
            let set = CspPolicySet::from_raw(policy_str);
            if set.is_empty() {
                None
            } else {
                Some(set)
            }
        } else {
            let set = CspPolicySet::from_headers(headers);
            if set.is_empty() {
                None
            } else {
                Some(set)
            }
        }
    }
}

/// Retry policy for transient HTTP failures.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (default: 0 = disabled).
    pub max_retries: u32,
    /// Initial backoff delay in milliseconds (default: 100).
    pub initial_backoff_ms: u64,
    /// Maximum backoff delay in milliseconds (default: 10_000).
    pub max_backoff_ms: u64,
    /// Exponential backoff multiplier (default: 2.0).
    pub backoff_factor: f64,
    /// HTTP status codes that trigger a retry (default: 408, 429, 500, 502, 503, 504).
    pub retry_on_statuses: Vec<u16>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 0,
            initial_backoff_ms: 100,
            max_backoff_ms: 10_000,
            backoff_factor: 2.0,
            retry_on_statuses: vec![408, 429, 500, 502, 503, 504],
        }
    }
}

#[derive(Debug, Clone)]
pub struct BrowserConfig {
    pub cache_dir: PathBuf,
    pub user_agent: String,
    pub timeout_ms: u32,
    pub wait_ms: u32,
    pub screenshot_endpoint: Option<String>,
    pub screenshot_timeout_ms: u64,
    /// Path to Chrome/Chromium binary for screenshot capture.
    /// If None, auto-discovers or downloads Chromium when the `screenshot` feature is enabled.
    pub screenshot_chrome_path: Option<PathBuf>,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub connection_pool: ConnectionPoolConfig,
    pub push: PushConfig,
    /// URL validation policy for SSRF protection.
    pub url_policy: UrlPolicy,
    /// Sandbox policy for restricting untrusted content execution.
    /// Defaults to `SandboxPolicy::off()` (no restrictions).
    pub sandbox: SandboxPolicy,
    /// Certificate pinning configuration.
    /// When set with pins, validates server certificates against known SPKI hashes or CA certs.
    #[cfg(feature = "tls-pinning")]
    pub cert_pinning: Option<CertificatePinningConfig>,
    /// Proxy configuration for HTTP/HTTPS/SOCKS5 traffic.
    /// Supports environment variable loading (HTTP_PROXY, HTTPS_PROXY, ALL_PROXY, NO_PROXY).
    pub proxy: ProxyConfig,
    /// CSP enforcement configuration.
    /// Defaults to disabled — no CSP enforcement unless explicitly enabled.
    pub csp: CspConfig,
    /// Whether to recursively parse and fetch iframe content (default: true).
    pub parse_iframes: bool,
    /// Maximum iframe nesting depth for recursive frame parsing (default: 5).
    /// 0 = root only, 1 = root + immediate iframes, etc.
    pub max_iframe_depth: usize,
    /// Request deduplication window in milliseconds (default: 0 = disabled).
    /// When enabled, parallel fetches of the same URL within this window share results.
    pub dedup_window_ms: u64,
    /// Retry policy for transient HTTP failures (default: disabled).
    pub retry: RetryConfig,
    /// Maximum file upload size in bytes (default: 50MB).
    pub max_upload_size: usize,
    /// Maximum number of HTTP redirects to follow (default: 10).
    /// Set to 0 to disable automatic redirect following.
    pub max_redirects: usize,
}

impl BrowserConfig {
    pub fn effective_user_agent(&self) -> &str {
        &self.user_agent
    }
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            cache_dir: default_cache_dir(),
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".to_string(),
            timeout_ms: 10_000,
            wait_ms: 3_000,
            screenshot_endpoint: None,
            screenshot_timeout_ms: 10_000,
            screenshot_chrome_path: None,
            viewport_width: 1280,
            viewport_height: 720,
            connection_pool: ConnectionPoolConfig::default(),
            push: PushConfig::default(),
            url_policy: UrlPolicy::default(),
            sandbox: SandboxPolicy::default(),
            #[cfg(feature = "tls-pinning")]
            cert_pinning: None,
            proxy: ProxyConfig::default(),
            csp: CspConfig::default(),
            parse_iframes: true,
            max_iframe_depth: 5,
            dedup_window_ms: 0,
            retry: RetryConfig::default(),
            max_upload_size: 50 * 1024 * 1024,
            max_redirects: 10,
        }
    }
}
