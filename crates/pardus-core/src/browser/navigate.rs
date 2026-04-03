//! Navigation operations: navigate, navigate_with_js, reload.

use crate::tab::Tab;
use super::Browser;

impl Browser {
    /// Navigate to a URL. Creates a tab if none exists.
    ///
    /// Fetches the page, builds the parsed HTML, updates tab history.
    pub async fn navigate(&mut self, url: &str) -> anyhow::Result<&Tab> {
        if !self.config.sandbox.is_navigation_allowed(url) {
            anyhow::bail!("Navigation to '{}' blocked by sandbox policy", url);
        }

        if self.active_tab.is_none() {
            let id = self.create_tab(url);
            let tab = self.tabs.get_mut(&id)
                .ok_or_else(|| anyhow::anyhow!("tab missing after creation"))?;
            tab.load_with_client(&self.http_client, &self.network_log, &self.config, false, 0).await?;
            self.active_tab = Some(id);
            return Ok(self.tabs.get(&id)
                .ok_or_else(|| anyhow::anyhow!("tab missing after creation"))?);
        }

        let id = self.require_active_id()?;
        let tab = self.tabs.get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("active tab missing"))?;
        tab.navigate_with_client(&self.http_client, &self.network_log, &self.config, url, false, 0).await?;
        Ok(self.tabs.get(&id)
            .ok_or_else(|| anyhow::anyhow!("active tab missing"))?)
    }

    /// Navigate with JS execution enabled.
    pub async fn navigate_with_js(&mut self, url: &str, wait_ms: u32) -> anyhow::Result<&Tab> {
        if !self.config.sandbox.is_navigation_allowed(url) {
            anyhow::bail!("Navigation to '{}' blocked by sandbox policy", url);
        }

        if self.active_tab.is_none() {
            let id = self.create_tab(url);
            let tab = self.tabs.get_mut(&id)
                .ok_or_else(|| anyhow::anyhow!("tab missing after creation"))?;
            tab.load_with_client(&self.http_client, &self.network_log, &self.config, true, wait_ms).await?;
            self.active_tab = Some(id);
            return Ok(self.tabs.get(&id)
                .ok_or_else(|| anyhow::anyhow!("tab missing after creation"))?);
        }

        let id = self.require_active_id()?;
        let tab = self.tabs.get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("active tab missing"))?;
        tab.navigate_with_client(&self.http_client, &self.network_log, &self.config, url, true, wait_ms).await?;
        Ok(self.tabs.get(&id)
            .ok_or_else(|| anyhow::anyhow!("active tab missing"))?)
    }

    /// Reload the active tab.
    pub async fn reload(&mut self) -> anyhow::Result<&Tab> {
        let id = self.require_active_id()?;
        let tab = self.tabs.get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("active tab missing"))?;
        tab.reload_with_client(&self.http_client, &self.network_log, &self.config).await?;
        Ok(self.tabs.get(&id)
            .ok_or_else(|| anyhow::anyhow!("active tab missing"))?)
    }
}
