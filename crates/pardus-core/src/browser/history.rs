//! History navigation: go_back, go_forward.

use crate::tab::Tab;

use super::Browser;

impl Browser {
    /// Go back in the active tab's history.
    pub async fn go_back(&mut self) -> anyhow::Result<Option<&Tab>> {
        let id = self.require_active_id()?;
        let tab = self.tabs.get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("active tab missing"))?;
        if tab.history_index > 0 {
            tab.history_index -= 1;
            tab.url = tab.history[tab.history_index].clone();
            tab.page = None;
            tab.load_with_client(&self.http_client, &self.network_log, &self.config, tab.config.js_enabled, tab.config.wait_ms).await?;
            Ok(Some(self.tabs.get(&id)
                .ok_or_else(|| anyhow::anyhow!("active tab missing"))?))
        } else {
            Ok(None)
        }
    }

    /// Go forward in the active tab's history.
    pub async fn go_forward(&mut self) -> anyhow::Result<Option<&Tab>> {
        let id = self.require_active_id()?;
        let tab = self.tabs.get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("active tab missing"))?;
        if tab.history_index < tab.history.len() - 1 {
            tab.history_index += 1;
            tab.url = tab.history[tab.history_index].clone();
            tab.page = None;
            tab.load_with_client(&self.http_client, &self.network_log, &self.config, tab.config.js_enabled, tab.config.wait_ms).await?;
            Ok(Some(self.tabs.get(&id)
                .ok_or_else(|| anyhow::anyhow!("active tab missing"))?))
        } else {
            Ok(None)
        }
    }
}
