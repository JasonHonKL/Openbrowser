//! Shared CLI context — eliminates duplicated proxy/cert config building across commands.

use pardus_core::BrowserConfig;

use crate::config::{self, PinPolicyArg};

/// Shared context built from common CLI arguments.
///
/// Each command variant constructs this from its proxy/cert-pin args,
/// then passes it to the command handler instead of building BrowserConfig inline.
pub struct CliContext {
    pub browser_config: BrowserConfig,
}

impl CliContext {
    /// Build from the standard set of proxy + cert pin CLI arguments.
    #[allow(clippy::too_many_arguments)]
    pub fn from_cli_args(
        proxy: Option<String>,
        proxy_http: Option<String>,
        proxy_https: Option<String>,
        no_proxy: Option<String>,
        no_proxy_env: bool,
        cert_pin: Vec<String>,
        cert_pin_file: Option<std::path::PathBuf>,
        pin_policy: Option<PinPolicyArg>,
    ) -> anyhow::Result<Self> {
        let mut browser_config = BrowserConfig::default();

        // Build proxy configuration
        let mut proxy_config = pardus_core::ProxyConfig::new();
        if let Some(all_proxy) = proxy {
            proxy_config = proxy_config.with_all_proxy(all_proxy);
        }
        if let Some(http) = proxy_http {
            proxy_config = proxy_config.with_http_proxy(http);
        }
        if let Some(https) = proxy_https {
            proxy_config = proxy_config.with_https_proxy(https);
        }
        if let Some(no) = no_proxy {
            proxy_config = proxy_config.with_no_proxy(no);
        }
        // Merge environment variables unless disabled
        if !no_proxy_env {
            proxy_config = proxy_config.merge_env();
        }
        browser_config.proxy = proxy_config;

        // Build certificate pinning configuration
        if !cert_pin.is_empty() || cert_pin_file.is_some() {
            let mut all_pins = cert_pin;
            if let Some(path) = &cert_pin_file {
                let file_pins = config::load_pins_from_file(path)
                    .map_err(|e| anyhow::anyhow!("Failed to load cert pin file '{}': {}", path.display(), e))?;
                all_pins.extend(file_pins);
            }
            let pin_config = config::build_cert_pinning_config(&all_pins, pin_policy, true)
                .map_err(|e| anyhow::anyhow!("Invalid certificate pin config: {}", e))?;
            browser_config.cert_pinning = Some(pin_config);
        }

        Ok(Self { browser_config })
    }

    /// Create a Browser from this context.
    pub fn create_browser(&self) -> anyhow::Result<pardus_core::Browser> {
        pardus_core::Browser::new(self.browser_config.clone())
    }

    /// Get a reference to the browser config.
    pub fn config(&self) -> &BrowserConfig {
        &self.browser_config
    }
}
