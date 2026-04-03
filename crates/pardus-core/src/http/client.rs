//! Shared HTTP client factory.

use std::sync::OnceLock;
use std::time::Duration;

/// Lightweight client for JS fetch ops (long-lived, does not depend on BrowserConfig).
pub fn fetch_client() -> &'static rquest::Client {
    static CLIENT: OnceLock<rquest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        rquest::Client::builder()
            .timeout(Duration::from_millis(10_000))
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(Duration::from_secs(60))
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| rquest::Client::new())
    })
}
