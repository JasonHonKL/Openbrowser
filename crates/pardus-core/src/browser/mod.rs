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

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use pardus_debug::NetworkLog;

use crate::{
    config::BrowserConfig,
    interact::FormState,
    intercept::InterceptorManager,
    page::Page,
    push::PushCache,
    session::{CookieEntry, SessionStore},
    tab::{Tab, TabId},
};

/// Unified headless browser for AI agents.
///
/// Owns the HTTP client, tab state, and provides navigation + interaction
/// as a single cohesive API. Every operation targets the active tab.
pub struct Browser {
    http_client: rquest::Client,
    config: BrowserConfig,
    network_log: Arc<Mutex<NetworkLog>>,
    push_cache: Arc<PushCache>,
    interceptors: InterceptorManager,
    /// Shared cookie jar for programmatic access.
    cookie_jar: Arc<SessionStore>,
    #[cfg(feature = "screenshot")]
    screenshot_handle: crate::screenshot::ScreenshotHandle,
    tabs: HashMap<TabId, Tab>,
    active_tab: Option<TabId>,
    pub(crate) form_state: FormState,
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
        let cache_dir = config.cache_dir.clone();

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

        let cookie_jar = Arc::new(SessionStore::ephemeral("browser", &cache_dir)?);

        Ok(Self {
            http_client,
            config,
            network_log: Arc::new(Mutex::new(NetworkLog::new())),
            push_cache,
            interceptors: InterceptorManager::new(),
            cookie_jar,
            #[cfg(feature = "screenshot")]
            screenshot_handle,
            tabs: HashMap::new(),
            active_tab: None,
            form_state: FormState::new(),
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
    pub fn http_client(&self) -> &rquest::Client { &self.http_client }

    pub fn config(&self) -> &BrowserConfig { &self.config }

    pub fn network_log(&self) -> &Arc<Mutex<NetworkLog>> { &self.network_log }

    pub fn push_cache(&self) -> &Arc<PushCache> { &self.push_cache }

    pub fn interceptors(&self) -> &InterceptorManager { &self.interceptors }

    pub fn interceptors_mut(&mut self) -> &mut InterceptorManager { &mut self.interceptors }

    /// Get the currently active tab.
    pub fn active_tab(&self) -> Option<&Tab> { self.active_tab.and_then(|id| self.tabs.get(&id)) }

    /// Get the currently active tab (mutable).
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.active_tab.and_then(move |id| self.tabs.get_mut(&id))
    }

    /// Get the active tab's page.
    pub fn current_page(&self) -> Option<&Page> { self.active_tab().and_then(|t| t.page.as_ref()) }

    /// Get the active tab's URL.
    pub fn current_url(&self) -> Option<&str> { self.active_tab().map(|t| t.url.as_str()) }

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
        self.screenshot_handle
            .capture_element(url, selector, opts)
            .await
    }

    /// Get a reference to the screenshot handle (for CDP integration).
    #[cfg(feature = "screenshot")]
    pub fn screenshot_handle(&self) -> &crate::screenshot::ScreenshotHandle {
        &self.screenshot_handle
    }

    // -------------------------------------------------------------------
    // Cookie jar convenience methods
    // -------------------------------------------------------------------

    /// Get a reference to the cookie jar.
    pub fn cookie_jar(&self) -> &Arc<SessionStore> { &self.cookie_jar }

    /// List all cookies in the jar.
    pub fn all_cookies(&self) -> Vec<CookieEntry> { self.cookie_jar.all_cookies() }

    /// Get cookies for a specific URL (as header string value).
    pub fn cookies_for_url(&self, url: &str) -> Option<String> {
        let parsed: url::Url = url.parse().ok()?;
        self.cookie_jar.cookies(&parsed).and_then(|v| {
            let s = v.to_str().ok()?;
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        })
    }

    /// Set a cookie programmatically.
    pub fn set_cookie(&self, name: &str, value: &str, domain: &str, path: &str) {
        self.cookie_jar.set_cookie(name, value, domain, path);
    }

    /// Delete a cookie by name, domain, and path.
    pub fn delete_cookie(&self, name: &str, domain: &str, path: &str) -> bool {
        self.cookie_jar.delete_cookie(name, domain, path)
    }

    /// Clear all cookies.
    pub fn clear_cookies(&self) { self.cookie_jar.clear_cookies(); }
}

impl Default for Browser {
    fn default() -> Self {
        Self::new(BrowserConfig::default())
            .unwrap_or_else(|e| panic!("failed to create default Browser: {e}"))
    }
}
