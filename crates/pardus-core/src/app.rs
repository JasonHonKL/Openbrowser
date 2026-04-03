use crate::config::BrowserConfig;
use pardus_debug::NetworkLog;
use parking_lot::RwLock;
use std::sync::Arc;
use std::sync::Mutex;
use url::Url;

/// Build an HTTP client from the given browser configuration.
///
/// Extracted as a standalone function so that both `App` and `Browser`
/// can reuse the same client-building logic.
pub fn build_http_client(config: &BrowserConfig) -> anyhow::Result<reqwest::Client> {
    let mut client_builder = reqwest::Client::builder()
        .user_agent(&config.user_agent)
        .timeout(std::time::Duration::from_millis(config.timeout_ms as u64));

    // Sandbox: disable cookie store for ephemeral sessions
    if !config.sandbox.ephemeral_session {
        client_builder = client_builder.cookie_store(true);
    }

    // Certificate pinning: use custom TLS connector when pins are configured
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
                    let mut new_builder = reqwest::Client::builder()
                        .user_agent(&config.user_agent)
                        .timeout(std::time::Duration::from_millis(config.timeout_ms as u64));
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
    pub http_client: reqwest::Client,
    pub config: RwLock<BrowserConfig>,
    pub network_log: Arc<Mutex<NetworkLog>>,
}

impl App {
    pub fn new(config: BrowserConfig) -> Self {
        let http_client = build_http_client(&config)
            .expect("failed to build HTTP client");

        Self {
            http_client,
            config: RwLock::new(config),
            network_log: Arc::new(Mutex::new(NetworkLog::new())),
        }
    }

    /// Validate a URL against the configured security policy.
    ///
    /// Returns an parsed URL if valid, or an error if the URL violates the policy.
    pub fn validate_url(&self, url: &str) -> anyhow::Result<Url> {
        self.config.read().url_policy.validate(url)
    }

    /// Get a snapshot of the current configuration.
    pub fn config_snapshot(&self) -> BrowserConfig {
        self.config.read().clone()
    }
}
