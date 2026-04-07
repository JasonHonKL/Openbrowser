use std::sync::{Arc, Mutex};

use pardus_debug::NetworkLog;
use parking_lot::RwLock;
use rquest_util::Emulation;
use url::Url;

use crate::{
    config::BrowserConfig, dedup::RequestDedup, intercept::InterceptorManager,
    session::SessionStore,
};

fn chrome_default_headers() -> rquest::header::HeaderMap {
    let mut headers = rquest::header::HeaderMap::new();

    headers.insert(
        rquest::header::ACCEPT,
        "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/\
         *;q=0.8"
            .parse()
            .unwrap(),
    );

    headers.insert(
        rquest::header::ACCEPT_LANGUAGE,
        "en-US,en;q=0.9".parse().unwrap(),
    );

    headers.insert(
        rquest::header::ACCEPT_ENCODING,
        "gzip, deflate, br".parse().unwrap(),
    );

    headers.insert(
        "sec-ch-ua",
        r#""Google Chrome";v="131", "Chromium";v="131", "Not_A Brand";v="24""#
            .parse()
            .unwrap(),
    );
    headers.insert("sec-ch-ua-mobile", "?0".parse().unwrap());
    headers.insert("sec-ch-ua-platform", r#""macOS""#.parse().unwrap());

    headers.insert("sec-fetch-dest", "document".parse().unwrap());
    headers.insert("sec-fetch-mode", "navigate".parse().unwrap());
    headers.insert("sec-fetch-site", "none".parse().unwrap());
    headers.insert("sec-fetch-user", "?1".parse().unwrap());

    headers.insert("upgrade-insecure-requests", "1".parse().unwrap());

    headers
}

fn base_client_builder(config: &BrowserConfig) -> rquest::ClientBuilder {
    let mut builder = rquest::Client::builder()
        .emulation(Emulation::Chrome131)
        .timeout(std::time::Duration::from_millis(config.timeout_ms as u64))
        .default_headers(chrome_default_headers())
        .user_agent(&config.user_agent)
        .cert_verification(config.tls_verify_certificates)
        .pool_max_idle_per_host(config.connection_pool.max_idle_per_host)
        .pool_idle_timeout(std::time::Duration::from_secs(
            config.connection_pool.idle_timeout_secs,
        ))
        .tcp_keepalive(std::time::Duration::from_secs(
            config.connection_pool.tcp_keepalive_secs,
        ))
        .http2_max_retry_count(2);

    if !config.sandbox.ephemeral_session {
        builder = builder.cookie_store(true);
    }

    builder
}

pub fn build_http_client(config: &BrowserConfig) -> anyhow::Result<rquest::Client> {
    let client_builder = base_client_builder(config);

    #[cfg(feature = "tls-pinning")]
    let client_builder = if let Some(pinning) = &config.cert_pinning {
        if !pinning.pins.is_empty() || !pinning.default_pins.is_empty() {
            match crate::tls::pinned_client_builder(client_builder, pinning) {
                Ok(builder) => builder,
                Err(e) => {
                    tracing::warn!("certificate pinning setup failed, using default TLS: {}", e);
                    base_client_builder(config)
                }
            }
        } else {
            client_builder
        }
    } else {
        client_builder
    };

    #[cfg(not(feature = "tls-pinning"))]
    let client_builder = client_builder;

    Ok(client_builder.build()?)
}

pub struct App {
    pub http_client: rquest::Client,
    pub config: RwLock<BrowserConfig>,
    pub network_log: Arc<Mutex<NetworkLog>>,
    pub interceptors: InterceptorManager,
    pub dedup: RequestDedup,
    pub cookie_jar: Arc<SessionStore>,
}

impl App {
    pub fn new(config: BrowserConfig) -> anyhow::Result<Self> {
        let http_client = build_http_client(&config)?;

        let dedup_window = config.dedup_window_ms;
        let cookie_jar = Arc::new(SessionStore::ephemeral("app", &config.cache_dir)?);

        Ok(Self {
            http_client,
            config: RwLock::new(config),
            network_log: Arc::new(Mutex::new(NetworkLog::new())),
            interceptors: InterceptorManager::new(),
            dedup: RequestDedup::new(dedup_window),
            cookie_jar,
        })
    }

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

    pub fn from_client_and_log(
        http_client: rquest::Client,
        config: BrowserConfig,
        network_log: Arc<Mutex<NetworkLog>>,
    ) -> anyhow::Result<Self> {
        let cookie_jar = Arc::new(SessionStore::ephemeral("app", &config.cache_dir)?);
        Ok(Self {
            http_client,
            config: RwLock::new(config),
            network_log,
            interceptors: InterceptorManager::new(),
            dedup: RequestDedup::new(0),
            cookie_jar,
        })
    }

    pub fn validate_url(&self, url: &str) -> anyhow::Result<Url> {
        self.config.read().url_policy.validate(url)
    }

    pub fn config_snapshot(&self) -> BrowserConfig { self.config.read().clone() }
}
