//! Page interaction operations: click, type, submit, scroll, etc.

use crate::interact::actions::InteractionResult;
use crate::interact::{FormState, ScrollDirection};

use super::Browser;

impl Browser {
    /// Click an element. If JS is enabled, dispatches click event in V8 DOM first.
    /// If the click produces navigation, the tab is updated.
    pub async fn click(&mut self, selector: &str) -> anyhow::Result<InteractionResult> {
        #[cfg(feature = "js")]
        if self.is_js_enabled() {
            let page = self.require_active_page()?;
            let app = self.temp_app();
            let result = crate::interact::js_interact::js_click(&app, page, selector).await?;
            drop(app);
            return self.apply_navigated_result(result);
        }

        let page = self.require_active_page()?;
        let handle = page.query(selector).ok_or_else(|| {
            anyhow::anyhow!("Element not found: {}", selector)
        })?;
        let app = self.temp_app();
        let result = crate::interact::actions::click(&app, page, &handle, &self.form_state).await?;
        drop(app);
        self.apply_navigated_result(result)
    }

    /// Click an element by its element ID (shown in semantic tree as [#1], [#2], etc.)
    /// This is the preferred way for AI agents to click elements.
    pub async fn click_by_id(&mut self, id: usize) -> anyhow::Result<InteractionResult> {
        let page = self.require_active_page()?;
        let handle = page.find_by_element_id(id).ok_or_else(|| {
            anyhow::anyhow!("Element with ID {} not found", id)
        })?;

        #[cfg(feature = "js")]
        if self.is_js_enabled() {
            let selector = handle.selector.clone();
            let app = self.temp_app();
            let result = crate::interact::js_interact::js_click(&app, page, &selector).await?;
            drop(app);
            return self.apply_navigated_result(result);
        }

        let app = self.temp_app();
        let result = crate::interact::actions::click(&app, page, &handle, &self.form_state).await?;
        drop(app);
        self.apply_navigated_result(result)
    }

    /// Type text into a form field.
    /// If JS is enabled, dispatches input/change events in V8 DOM.
    pub async fn type_text(&mut self, selector: &str, value: &str) -> anyhow::Result<InteractionResult> {
        #[cfg(feature = "js")]
        if self.is_js_enabled() {
            let page = self.require_active_page()?;
            let result = crate::interact::js_interact::js_type(page, selector, value).await?;
            return self.apply_navigated_result(result);
        }

        let page = self.require_active_page()?;
        let handle = page.query(selector).ok_or_else(|| {
            anyhow::anyhow!("Element not found: {}", selector)
        })?;
        let (result, field_name) = {
            let name = handle.name.clone();
            let result = crate::interact::actions::type_text(page, &handle, value)?;
            (result, name)
        };
        if let Some(name) = field_name {
            self.form_state.set(&name, value);
        }
        Ok(result)
    }

    /// Type text into a form field by its element ID (shown in semantic tree as [#1], [#2], etc.)
    /// This is the preferred way for AI agents to fill form fields.
    pub async fn type_by_id(&mut self, id: usize, value: &str) -> anyhow::Result<InteractionResult> {
        #[cfg(feature = "js")]
        if self.is_js_enabled() {
            let page = self.require_active_page()?;
            let handle = page.find_by_element_id(id).ok_or_else(|| {
                anyhow::anyhow!("Element with ID {} not found", id)
            })?;
            let name = handle.name.clone();
            let selector = handle.selector.clone();
            let result = crate::interact::js_interact::js_type(page, &selector, value).await?;
            if let Some(n) = name {
                self.form_state.set(&n, value);
            }
            return Ok(result);
        }

        let page = self.require_active_page()?;
        let handle = page.find_by_element_id(id).ok_or_else(|| {
            anyhow::anyhow!("Element with ID {} not found", id)
        })?;
        let (result, field_name) = {
            let name = handle.name.clone();
            let result = crate::interact::actions::type_text(page, &handle, value)?;
            (result, name)
        };
        if let Some(name) = field_name {
            self.form_state.set(&name, value);
        }
        Ok(result)
    }

    /// Submit a form with the given field values.
    /// If JS is enabled, dispatches submit event first and respects preventDefault.
    pub async fn submit(
        &mut self,
        form_selector: &str,
        state: &FormState,
    ) -> anyhow::Result<InteractionResult> {
        #[cfg(feature = "js")]
        if self.is_js_enabled() {
            let page = self.require_active_page()?;
            let app = self.temp_app();
            let result = crate::interact::js_interact::js_submit(&app, page, form_selector, state).await?;
            drop(app);
            return self.apply_navigated_result(result);
        }

        let page = self.require_active_page()?;
        let app = self.temp_app();
        let result = crate::interact::form::submit_form(&app, page, form_selector, state).await?;
        drop(app);
        self.apply_navigated_result(result)
    }

    /// Wait for a CSS selector to appear.
    pub async fn wait_for(
        &mut self,
        selector: &str,
        timeout_ms: u32,
    ) -> anyhow::Result<InteractionResult> {
        let page = self.require_active_page()?;
        let app = self.temp_app();
        let result = crate::interact::wait::wait_for_selector(
            &app, page, selector, timeout_ms, 500,
        ).await?;
        drop(app);
        Ok(result)
    }

    /// Scroll. If JS is enabled, dispatches scroll/wheel events in V8 DOM.
    /// Otherwise uses URL-based pagination detection.
    pub async fn scroll(&mut self, direction: ScrollDirection) -> anyhow::Result<InteractionResult> {
        #[cfg(feature = "js")]
        if self.is_js_enabled() {
            let page = self.require_active_page()?;
            let result = crate::interact::js_interact::js_scroll(page, direction).await?;
            return self.apply_navigated_result(result);
        }

        let page = self.require_active_page()?;
        let app = self.temp_app();
        let result = crate::interact::scroll::scroll(&app, page, direction).await?;
        drop(app);
        self.apply_navigated_result(result)
    }

    /// Toggle a checkbox or radio.
    pub fn toggle(&mut self, selector: &str) -> anyhow::Result<InteractionResult> {
        let page = self.require_active_page()?;
        let handle = page.query(selector).ok_or_else(|| {
            anyhow::anyhow!("Element not found: {}", selector)
        })?;
        let result = crate::interact::actions::toggle(page, &handle)?;
        if let Some(ref name) = handle.name {
            let value = handle.value.as_deref().unwrap_or("on");
            if let InteractionResult::Toggled { checked, .. } = &result {
                self.form_state.apply_toggle(name, value, *checked);
            }
        }
        Ok(result)
    }

    /// Select an option in a `<select>` element.
    pub fn select_option(&mut self, selector: &str, value: &str) -> anyhow::Result<InteractionResult> {
        let page = self.require_active_page()?;
        let handle = page.query(selector).ok_or_else(|| {
            anyhow::anyhow!("Element not found: {}", selector)
        })?;
        let result = crate::interact::actions::select_option(page, &handle, value)?;
        if let Some(ref name) = handle.name {
            self.form_state.set(name, value);
        }
        Ok(result)
    }
}
