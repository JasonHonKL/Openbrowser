use crate::config::BrowserConfig;
use crate::dedup::RequestDedup;
use crate::intercept::InterceptorManager;
use crate::session::SessionStore;
use pardus_debug::NetworkLog;
use parking_lot::RwLock;
use rquest_util::Emulation;
use std::sync::Arc;
use std::sync::Mutex;
use url::Url;

/// Build Chrome-like default headers that anti-bot systems expect.
fn chrome_default_headers() -> rquest::header::HeaderMap {
    let mut headers = rquest::header::HeaderMap::new();

    // Accept header (Chrome navigation request)
    headers.insert(
        rquest::header::ACCEPT,
        "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8"
            .parse().unwrap(),
    );

    // Accept-Language
    headers.insert(
        rquest::header::ACCEPT_LANGUAGE,
        "en-US,en;q=0.9".parse().unwrap(),
    );

    // Accept-Encoding
    headers.insert(
        rquest::header::ACCEPT_ENCODING,
        "gzip, deflate, br".parse().unwrap(),
    );

    // Client Hints — Chrome 131 brand tokens
    headers.insert(
        "sec-ch-ua",
        r#""Google Chrome";v="131", "Chromium";v="131", "Not_A Brand";v="24""#
            .parse().unwrap(),
    );
    headers.insert("sec-ch-ua-mobile", "?0".parse().unwrap());
    headers.insert("sec-ch-ua-platform", r#""macOS""#.parse().unwrap());

    // Fetch metadata
    headers.insert("sec-fetch-dest", "document".parse().unwrap());
    headers.insert("sec-fetch-mode", "navigate".parse().unwrap());
    headers.insert("sec-fetch-site", "none".parse().unwrap());
    headers.insert("sec-fetch-user", "?1".parse().unwrap());

    // Upgrade-Insecure-Requests
    headers.insert("upgrade-insecure-requests", "1".parse().unwrap());

    headers
}

/// Build an HTTP client from the given browser configuration.
///
/// Extracted as a standalone function so that both `App` and `Browser`
/// can reuse the same client-building logic.
pub fn build_http_client(config: &BrowserConfig) -> anyhow::Result<rquest::Client> {
    let mut client_builder = rquest::Client::builder()
        .emulation(Emulation::Chrome131)
        .user_agent(&config.user_agent)
        .timeout(std::time::Duration::from_millis(config.timeout_ms as u64))
        .default_headers(chrome_default_headers());

    // Sandbox: disable cookie store for ephemeral sessions
    if !config.sandbox.ephemeral_session {
        client_builder = client_builder.cookie_store(true);
    }

    // Certificate pinning: use custom TLS connector when pins are configured
    #[cfg(feature = "tls-pinning")]
    if let Some(pinning) = &config.cert_pinning {
        if !pinning.pins.is_empty() || !pinning.default_pins.is_empty() {
            client_builder = match crate::tls::pinned_client_builder(client_builder, pinning) {
                Ok(builder) => builder,
                Err(e) => {
                    tracing::warn!(
                        "certificate pinning setup failed, using default TLS: {}",
                        e
                    );
                    // Rebuild without pinning since builder was moved
                    let mut new_builder = rquest::Client::builder()
                        .emulation(Emulation::Chrome131)
                        .user_agent(&config.user_agent)
                        .timeout(std::time::Duration::from_millis(config.timeout_ms as u64))
                        .default_headers(chrome_default_headers());
                    if !config.sandbox.ephemeral_session {
                        new_builder = new_builder.cookie_store(true);
                    }
                    new_builder
                }
            };
        }
    }

    Ok(client_builder.build()?)
}

pub struct App {
    pub http_client: rquest::Client,
    pub config: RwLock<BrowserConfig>,
    pub network_log: Arc<Mutex<NetworkLog>>,
    /// Request interception pipeline.
    pub interceptors: InterceptorManager,
    /// Request deduplication tracker.
    pub dedup: RequestDedup,
    /// Shared cookie jar for programmatic access.
    pub cookie_jar: Arc<SessionStore>,
}

impl App {
    pub fn new(config: BrowserConfig) -> Self {
        let http_client = build_http_client(&config)
            .expect("failed to build HTTP client");

        let dedup_window = config.dedup_window_ms;
        let cookie_jar = Arc::new(
            SessionStore::ephemeral("app", &config.cache_dir)
                .expect("failed to create cookie jar"),
        );

        Self {
            http_client,
            config: RwLock::new(config),
            network_log: Arc::new(Mutex::new(NetworkLog::new())),
            interceptors: InterceptorManager::new(),
            dedup: RequestDedup::new(dedup_window),
            cookie_jar,
        }
    }

    /// Create an App that shares pipeline state (for Browser temp_app).
    pub fn from_shared(
        http_client: rquest::Client,
        config: BrowserConfig,
        network_log: Arc<Mutex<NetworkLog>>,
        interceptors: InterceptorManager,
        dedup: RequestDedup,
        cookie_jar: Arc<SessionStore>,
    ) -> Self {
        Self {
            http_client,
            config: RwLock::new(config),
            network_log,
            interceptors,
            dedup,
            cookie_jar,
        }
    }

    /// Validate a URL against the configured security policy.
    ///
    /// Returns a parsed URL if valid, or an error if the URL violates the policy.
    pub fn validate_url(&self, url: &str) -> anyhow::Result<Url> {
        self.config.read().url_policy.validate(url)
    }

    /// Get a snapshot of the current configuration.
    pub fn config_snapshot(&self) -> BrowserConfig {
        self.config.read().clone()
    }
}
