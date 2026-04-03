//! Internal helpers for Browser: require_active_*, temp_app, apply_navigated_result.

use std::sync::Arc;

use crate::interact::actions::InteractionResult;
use crate::page::Page;
use crate::tab::TabId;

use super::Browser;

impl Browser {
    pub(super) fn require_active_id(&self) -> anyhow::Result<TabId> {
        self.active_tab.ok_or_else(|| anyhow::anyhow!("No active tab"))
    }

    pub(super) fn require_active_page(&self) -> anyhow::Result<&Page> {
        self.current_page().ok_or_else(|| anyhow::anyhow!("No page loaded in active tab"))
    }

    /// Check if the active tab has JS execution enabled.
    #[allow(dead_code)]
    pub(super) fn is_js_enabled(&self) -> bool {
        self.active_tab().map(|t| t.config.js_enabled).unwrap_or(false)
    }

    /// Create a temporary `Arc<App>` that shares pipeline state from the Browser.
    /// This lets us reuse the existing interact functions unchanged.
    pub(super) fn temp_app(&self) -> Arc<crate::app::App> {
        Arc::new(crate::app::App::from_shared(
            self.http_client.clone(),
            self.config.clone(),
            self.network_log.clone(),
            self.interceptors.clone(),
            crate::dedup::RequestDedup::new(self.config.dedup_window_ms),
            self.cookie_jar.clone(),
        ))
    }

    /// If an interaction produced a `Navigated` result, update the active tab.
    /// Clears accumulated form state on navigation.
    pub(super) fn apply_navigated_result(&mut self, result: InteractionResult) -> anyhow::Result<InteractionResult> {
        if let InteractionResult::Navigated(new_page) = result {
            self.form_state = crate::interact::FormState::new();
            let id = self.require_active_id()?;
            let tab = self.tabs.get_mut(&id)
                .ok_or_else(|| anyhow::anyhow!("active tab missing"))?;
            tab.update_page(new_page);
            Ok(InteractionResult::Navigated(
                tab.page.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("page missing after update"))?
                    .clone_shallow()
            ))
        } else if let InteractionResult::Scrolled { url, page: new_page } = result {
            self.form_state = crate::interact::FormState::new();
            let id = self.require_active_id()?;
            let tab = self.tabs.get_mut(&id)
                .ok_or_else(|| anyhow::anyhow!("active tab missing"))?;
            tab.update_page(new_page);
            let url_clone = url.clone();
            Ok(InteractionResult::Scrolled {
                url: url_clone,
                page: tab.page.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("page missing after update"))?
                    .clone_shallow(),
            })
        } else {
            Ok(result)
        }
    }
}
