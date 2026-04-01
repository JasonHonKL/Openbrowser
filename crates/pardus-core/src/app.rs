use crate::config::BrowserConfig;
use pardus_debug::NetworkLog;
use std::sync::Arc;
use std::sync::Mutex;

pub struct App {
    pub http_client: reqwest::Client,
    pub config: BrowserConfig,
    pub network_log: Arc<Mutex<NetworkLog>>,
}

impl App {
    pub fn new(config: BrowserConfig) -> Self {
        let http_client = reqwest::Client::builder()
            .user_agent(&config.user_agent)
            .timeout(std::time::Duration::from_millis(config.timeout_ms as u64))
            .cookie_store(true)
            .build()
            .expect("failed to build HTTP client");

        Self {
            http_client,
            config,
            network_log: Arc::new(Mutex::new(NetworkLog::new())),
        }
    }

    pub fn default() -> Arc<Self> {
        Arc::new(Self::new(BrowserConfig::default()))
    }
}
