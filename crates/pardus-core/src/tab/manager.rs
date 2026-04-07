//! Tab manager for lifecycle and switching
//!
//! The TabManager owns all tabs and manages their lifecycle.
//! It uses a shared App for HTTP resources.

use std::{collections::HashMap, sync::Arc};

use super::{Tab, TabId, TabState, tab::TabConfig};
use crate::{app::App, config::BrowserConfig};

/// Errors that can occur in tab management
#[derive(Debug, thiserror::Error)]
pub enum TabManagerError {
    #[error("Tab not found: {0}")]
    TabNotFound(TabId),
    #[error("No active tab")]
    NoActiveTab,
    #[error("Tab is busy (loading or navigating)")]
    TabBusy,
    #[error("Tab in error state: {0}")]
    TabError(String),
    #[error("Memory limit exceeded for tab {0}: using {1}MB of {2}MB limit")]
    MemoryLimitExceeded(TabId, usize, usize),
}

/// Manages multiple browser tabs with shared App resources
///
/// The TabManager is the entry point for tab-based browser operations.
/// It maintains a set of tabs and tracks which one is currently active.
pub struct TabManager {
    /// All tabs by ID
    tabs: HashMap<TabId, Tab>,
    /// Currently active tab ID
    active_tab: Option<TabId>,
    /// Shared App instance
    app: Arc<App>,
    /// Maximum number of tabs (0 = unlimited)
    pub max_tabs: usize,
}

impl std::fmt::Debug for TabManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabManager")
            .field("tab_count", &self.tabs.len())
            .field("active_tab", &self.active_tab)
            .field("max_tabs", &self.max_tabs)
            .finish()
    }
}

impl TabManager {
    /// Create a new TabManager with default App configuration
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self::with_app(Arc::new(App::new(BrowserConfig::default())?)))
    }

    /// Create a new TabManager with the given App
    pub fn with_app(app: Arc<App>) -> Self {
        Self {
            tabs: HashMap::new(),
            active_tab: None,
            app,
            max_tabs: 0,
        }
    }

    /// Create a new tab with the given URL
    ///
    /// Returns the new tab's ID. The tab is created but not loaded.
    /// Use `load_tab()` or `switch_to()` to activate and load.
    pub fn create_tab(&mut self, url: impl Into<String>) -> TabId {
        let tab = Tab::new(url);
        let id = tab.id;
        self.tabs.insert(id, tab);
        id
    }

    /// Create a new tab with a memory limit
    ///
    /// Convenience method that creates a tab with the specified memory limit in MB.
    /// A limit of 0 means unlimited.
    pub fn create_tab_with_memory_limit(
        &mut self,
        url: impl Into<String>,
        memory_limit_mb: usize,
    ) -> TabId {
        let mut config = TabConfig::default();
        config.memory_limit_mb = memory_limit_mb;
        self.create_tab_with_config(url, config)
    }

    /// Create a new tab with custom configuration
    pub fn create_tab_with_config(&mut self, url: impl Into<String>, config: TabConfig) -> TabId {
        let tab = Tab::with_config(url, config);
        let id = tab.id;
        self.tabs.insert(id, tab);
        id
    }

    /// Create a tab, activate it, and load the page
    ///
    /// This is a convenience method for the common case.
    pub async fn open_tab(&mut self, url: impl Into<String>) -> anyhow::Result<&Tab> {
        let id = self.create_tab(url);
        self.switch_to(id).await
    }

    /// Get a reference to a tab by ID
    pub fn get(&self, id: TabId) -> Option<&Tab> { self.tabs.get(&id) }

    /// Get a mutable reference to a tab by ID
    pub fn get_mut(&mut self, id: TabId) -> Option<&mut Tab> { self.tabs.get_mut(&id) }

    /// Check if a tab has exceeded its memory limit
    pub fn check_memory_limit(&self, id: TabId) -> Result<bool, TabManagerError> {
        let tab = self.tabs.get(&id).ok_or(TabManagerError::TabNotFound(id))?;
        Ok(tab.is_memory_limit_exceeded())
    }

    /// Get memory usage for a tab in MB
    pub fn get_memory_usage_mb(&self, id: TabId) -> Result<usize, TabManagerError> {
        let tab = self.tabs.get(&id).ok_or(TabManagerError::TabNotFound(id))?;
        Ok(tab.memory_usage_mb())
    }

    /// Get total memory usage across all tabs
    pub fn total_memory_usage_mb(&self) -> usize {
        self.tabs.values().map(|t| t.memory_usage_mb()).sum()
    }

    /// Get the number of tabs that have exceeded their memory limits
    pub fn tabs_exceeding_memory_limit(&self) -> Vec<TabId> {
        self.tabs
            .values()
            .filter(|t| t.is_memory_limit_exceeded())
            .map(|t| t.id)
            .collect()
    }

    /// Free memory for tabs that have exceeded their limit
    /// Returns the number of tabs that were freed
    pub fn free_excess_memory(&mut self) -> usize {
        let exceeded: Vec<TabId> = self.tabs_exceeding_memory_limit();
        let count = exceeded.len();
        for id in exceeded {
            if let Some(tab) = self.tabs.get_mut(&id) {
                tab.free_memory();
            }
        }
        count
    }

    /// Switch to a different tab (activate it and load if needed)
    ///
    /// Returns the active tab. Loads the page if it hasn't been loaded yet.
    pub async fn switch_to(&mut self, id: TabId) -> anyhow::Result<&Tab> {
        // Check if tab exists
        if !self.tabs.contains_key(&id) {
            return Err(TabManagerError::TabNotFound(id).into());
        }

        // Activate new
        self.active_tab = Some(id);
        let tab = self
            .tabs
            .get_mut(&id)
            .ok_or_else(|| TabManagerError::TabNotFound(id))?;
        tab.activate();

        // Load if not already loaded
        let needs_load = tab.page.is_none() && matches!(tab.state, TabState::Loading);
        if needs_load {
            tab.load(&self.app).await?;
        }

        Ok(self
            .tabs
            .get(&id)
            .ok_or_else(|| TabManagerError::TabNotFound(id))?)
    }

    /// Get the currently active tab
    pub fn active(&self) -> Option<&Tab> { self.active_tab.and_then(|id| self.tabs.get(&id)) }

    /// Get the currently active tab (mutable)
    pub fn active_mut(&mut self) -> Option<&mut Tab> {
        self.active_tab.and_then(move |id| self.tabs.get_mut(&id))
    }

    /// Require an active tab or return error
    pub fn require_active(&self) -> Result<&Tab, TabManagerError> {
        self.active().ok_or(TabManagerError::NoActiveTab)
    }

    /// Close a tab by ID
    ///
    /// Returns true if the closed tab was the active one.
    /// Returns error if tab not found.
    pub fn close_tab(&mut self, id: TabId) -> Result<bool, TabManagerError> {
        if self.tabs.remove(&id).is_none() {
            return Err(TabManagerError::TabNotFound(id));
        }

        let was_active = self.active_tab == Some(id);
        if was_active {
            // Switch to another tab if available
            self.active_tab = self.tabs.keys().next().copied();
            if let Some(new_id) = self.active_tab {
                if let Some(tab) = self.tabs.get_mut(&new_id) {
                    tab.activate();
                }
            }
        }

        Ok(was_active)
    }

    /// Close all tabs
    pub fn close_all(&mut self) {
        self.tabs.clear();
        self.active_tab = None;
    }

    /// Close all tabs except the active one
    pub fn close_others(&mut self) {
        if let Some(active) = self.active_tab {
            self.tabs.retain(|id, _| *id == active);
        }
    }

    /// List all tabs
    pub fn list(&self) -> Vec<&Tab> { self.tabs.values().collect() }

    /// Get tab count
    pub fn len(&self) -> usize { self.tabs.len() }

    /// Check if there are any tabs
    pub fn is_empty(&self) -> bool { self.tabs.is_empty() }

    /// Check if a tab exists
    pub fn contains(&self, id: TabId) -> bool { self.tabs.contains_key(&id) }

    /// Navigate the active tab to a new URL
    pub async fn navigate_active(&mut self, url: &str) -> anyhow::Result<&Tab> {
        let id = self.require_active()?.id;
        let tab = self
            .tabs
            .get_mut(&id)
            .ok_or_else(|| TabManagerError::TabNotFound(id))?;
        tab.navigate(&self.app, url).await?;
        Ok(self
            .tabs
            .get(&id)
            .ok_or_else(|| TabManagerError::TabNotFound(id))?)
    }

    /// Reload the active tab
    pub async fn reload_active(&mut self) -> anyhow::Result<&Tab> {
        let id = self.require_active()?.id;
        let tab = self
            .tabs
            .get_mut(&id)
            .ok_or_else(|| TabManagerError::TabNotFound(id))?;
        tab.reload(&self.app).await?;
        Ok(self
            .tabs
            .get(&id)
            .ok_or_else(|| TabManagerError::TabNotFound(id))?)
    }

    /// Go back in the active tab's history
    pub async fn go_back(&mut self) -> anyhow::Result<Option<&Tab>> {
        let id = self.require_active()?.id;
        let tab = self
            .tabs
            .get_mut(&id)
            .ok_or_else(|| TabManagerError::TabNotFound(id))?;
        tab.go_back(&self.app).await?;
        Ok(self.active())
    }

    /// Go forward in the active tab's history
    pub async fn go_forward(&mut self) -> anyhow::Result<Option<&Tab>> {
        let id = self.require_active()?.id;
        let tab = self
            .tabs
            .get_mut(&id)
            .ok_or_else(|| TabManagerError::TabNotFound(id))?;
        tab.go_forward(&self.app).await?;
        Ok(self.active())
    }

    /// Get the shared App instance
    pub fn app(&self) -> &Arc<App> { &self.app }
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new().unwrap_or_else(|e| panic!("failed to create default TabManager: {e}"))
    }
}

/// Summary information about the tab manager state
#[derive(Debug, Clone, serde::Serialize)]
pub struct TabManagerSummary {
    pub tab_count: usize,
    pub active_tab: Option<TabId>,
}

impl TabManager {
    /// Get a summary of the manager state
    pub fn summary(&self) -> TabManagerSummary {
        TabManagerSummary {
            tab_count: self.len(),
            active_tab: self.active_tab,
        }
    }
}
