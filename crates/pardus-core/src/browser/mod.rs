//! Unified browser that combines navigation, interaction, and tab management.
//!
//! `Browser` replaces the separate `App` + `TabManager` + `Page` pattern with
//! a single entry point. All operations work on the active tab.

mod helpers;
mod history;
mod interact;
mod navigate;
mod tabs;
pub mod traits;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use pardus_debug::NetworkLog;

use crate::config::BrowserConfig;
use crate::page::Page;
use crate::push::PushCache;
use crate::tab::{Tab, TabId};

/// Unified headless browser for AI agents.
///
/// Owns the HTTP client, tab state, and provides navigation + interaction
/// as a single cohesive API. Every operation targets the active tab.
pub struct Browser {
    pub http_client: reqwest::Client,
    pub config: BrowserConfig,
    pub network_log: Arc<Mutex<NetworkLog>>,
    pub push_cache: Arc<PushCache>,
    #[cfg(feature = "screenshot")]
    screenshot_handle: crate::screenshot::ScreenshotHandle,
    tabs: HashMap<TabId, Tab>,
    active_tab: Option<TabId>,
}

impl std::fmt::Debug for Browser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Browser")
            .field("tab_count", &self.tabs.len())
            .field("active_tab", &self.active_tab)
            .finish()
    }
}

// -----------------------------------------------------------------------
// Construction
// -----------------------------------------------------------------------

impl Browser {
    /// Create a new Browser with the given configuration.
    pub fn new(config: BrowserConfig) -> anyhow::Result<Self> {
        let http_client = crate::app::build_http_client(&config)?;

        let push_cache = Arc::new(PushCache::new(
            config.push.max_push_resources,
            config.push.push_cache_ttl_secs,
        ));

        #[cfg(feature = "screenshot")]
        let screenshot_handle = crate::screenshot::ScreenshotHandle::new(
            config.screenshot_chrome_path.clone(),
            config.viewport_width,
            config.viewport_height,
        );

        Ok(Self {
            http_client,
            config,
            network_log: Arc::new(Mutex::new(NetworkLog::new())),
            push_cache,
            #[cfg(feature = "screenshot")]
            screenshot_handle,
            tabs: HashMap::new(),
            active_tab: None,
        })
    }

    /// Create a default Browser wrapped in `Arc` for sharing.
    pub fn default_shared() -> anyhow::Result<Arc<Self>> {
        Ok(Arc::new(Self::new(BrowserConfig::default())?))
    }
}

// -----------------------------------------------------------------------
// Accessors
// -----------------------------------------------------------------------

impl Browser {
    /// Get the currently active tab.
    pub fn active_tab(&self) -> Option<&Tab> {
        self.active_tab.and_then(|id| self.tabs.get(&id))
    }

    /// Get the currently active tab (mutable).
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.active_tab.and_then(move |id| self.tabs.get_mut(&id))
    }

    /// Get the active tab's page.
    pub fn current_page(&self) -> Option<&Page> {
        self.active_tab().and_then(|t| t.page.as_ref())
    }

    /// Get the active tab's URL.
    pub fn current_url(&self) -> Option<&str> {
        self.active_tab().map(|t| t.url.as_str())
    }

    /// Capture a full-page screenshot of the given URL.
    #[cfg(feature = "screenshot")]
    pub async fn capture_screenshot(
        &self,
        url: &str,
        opts: &crate::screenshot::ScreenshotOptions,
    ) -> anyhow::Result<Vec<u8>> {
        self.screenshot_handle.capture_page(url, opts).await
    }

    /// Capture a screenshot of a specific element identified by CSS selector.
    #[cfg(feature = "screenshot")]
    pub async fn capture_element_screenshot(
        &self,
        url: &str,
        selector: &str,
        opts: &crate::screenshot::ScreenshotOptions,
    ) -> anyhow::Result<Vec<u8>> {
        self.screenshot_handle.capture_element(url, selector, opts).await
    }

    /// Get a reference to the screenshot handle (for CDP integration).
    #[cfg(feature = "screenshot")]
    pub fn screenshot_handle(&self) -> &crate::screenshot::ScreenshotHandle {
        &self.screenshot_handle
    }
}

impl Default for Browser {
    fn default() -> Self {
        Self::new(BrowserConfig::default())
            .expect("failed to create default Browser: TLS backend may be unavailable")
    }
}
