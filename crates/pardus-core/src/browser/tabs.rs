//! Tab lifecycle management: create, switch, close, list.

use super::Browser;
use crate::tab::{Tab, TabId, TabState};

impl Browser {
    /// Create a new tab with the given URL (does not load it).
    pub fn create_tab(&mut self, url: impl Into<String>) -> TabId {
        let tab = Tab::new(url);
        let id = tab.id;
        self.tabs.insert(id, tab);
        id
    }

    /// Create a tab with custom configuration.
    pub fn create_tab_with_config(
        &mut self,
        url: impl Into<String>,
        config: crate::tab::tab::TabConfig,
    ) -> TabId {
        let tab = Tab::with_config(url, config);
        let id = tab.id;
        self.tabs.insert(id, tab);
        id
    }

    /// Create, activate, and load a tab.
    pub async fn open_tab(&mut self, url: impl Into<String>) -> anyhow::Result<&Tab> {
        let id = self.create_tab(url);
        self.switch_to(id).await
    }

    /// Switch to a tab by ID, loading it if needed.
    pub async fn switch_to(&mut self, id: TabId) -> anyhow::Result<&Tab> {
        if !self.tabs.contains_key(&id) {
            return Err(anyhow::anyhow!("Tab not found: {}", id));
        }
        self.active_tab = Some(id);
        let tab = self
            .tabs
            .get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("tab missing after verification"))?;
        tab.activate();

        let needs_load = tab.page.is_none() && matches!(tab.state, TabState::Loading);
        if needs_load {
            tab.load_with_client(
                &self.http_client,
                &self.network_log,
                &self.config,
                tab.config.js_enabled,
                tab.config.wait_ms,
            )
            .await?;
        }

        Ok(self
            .tabs
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("tab missing after verification"))?)
    }

    /// Close a tab. Returns true if it was the active tab.
    pub fn close_tab(&mut self, id: TabId) -> bool {
        if self.tabs.remove(&id).is_none() {
            return false;
        }
        let was_active = self.active_tab == Some(id);
        if was_active {
            self.active_tab = self.tabs.keys().next().copied();
            if let Some(new_id) = self.active_tab {
                if let Some(tab) = self.tabs.get_mut(&new_id) {
                    tab.activate();
                }
            }
        }
        was_active
    }

    /// Close all tabs.
    pub fn close_all(&mut self) {
        self.tabs.clear();
        self.active_tab = None;
    }

    /// Close all tabs except the active one.
    pub fn close_others(&mut self) {
        if let Some(active) = self.active_tab {
            self.tabs.retain(|id, _| *id == active);
        }
    }

    /// List all tabs.
    pub fn list_tabs(&self) -> Vec<&Tab> { self.tabs.values().collect() }

    /// Number of open tabs.
    pub fn tab_count(&self) -> usize { self.tabs.len() }
}
