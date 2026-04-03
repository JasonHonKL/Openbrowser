//! Core operation traits for Browser.
//!
//! These traits enable:
//! - Mocking for tests (no network needed)
//! - Future alternative implementations (remote browser via CDP, replay from recording)
//! - Generic code that works with any implementation
//!
//! `Browser` implements all traits directly (zero-cost — no `Box<dyn Trait>` overhead).
//! Consumers can use `Browser` concretely or generically via `&mut dyn Interactor`.

use crate::interact::actions::InteractionResult;
use crate::interact::FormState;
use crate::interact::ScrollDirection;
use crate::tab::{Tab, TabId};
use crate::tab::tab::TabConfig;

/// Navigation operations.
#[async_trait::async_trait]
pub trait Navigator: Send + Sync {
    /// Navigate to a URL (HTTP-only, no JS execution).
    async fn navigate(&mut self, url: &str) -> anyhow::Result<&Tab>;

    /// Navigate with JS execution enabled.
    async fn navigate_with_js(&mut self, url: &str, wait_ms: u32) -> anyhow::Result<&Tab>;

    /// Reload the active tab.
    async fn reload(&mut self) -> anyhow::Result<&Tab>;

    /// Go back in the active tab's history.
    async fn go_back(&mut self) -> anyhow::Result<Option<&Tab>>;

    /// Go forward in the active tab's history.
    async fn go_forward(&mut self) -> anyhow::Result<Option<&Tab>>;
}

/// Page interaction operations.
#[async_trait::async_trait]
pub trait Interactor: Send + Sync {
    /// Click an element by CSS selector.
    async fn click(&mut self, selector: &str) -> anyhow::Result<InteractionResult>;

    /// Click an element by its numeric element ID.
    async fn click_by_id(&mut self, id: usize) -> anyhow::Result<InteractionResult>;

    /// Type text into a form field.
    async fn type_text(&mut self, selector: &str, value: &str) -> anyhow::Result<InteractionResult>;

    /// Type text into a form field by its numeric element ID.
    async fn type_by_id(&mut self, id: usize, value: &str) -> anyhow::Result<InteractionResult>;

    /// Submit a form with the given field values.
    async fn submit(
        &mut self,
        form_selector: &str,
        state: &FormState,
    ) -> anyhow::Result<InteractionResult>;

    /// Wait for a CSS selector to appear.
    async fn wait_for(
        &mut self,
        selector: &str,
        timeout_ms: u32,
    ) -> anyhow::Result<InteractionResult>;

    /// Scroll the page.
    async fn scroll(&mut self, direction: ScrollDirection) -> anyhow::Result<InteractionResult>;

    /// Toggle a checkbox or radio button.
    fn toggle(&mut self, selector: &str) -> anyhow::Result<InteractionResult>;

    /// Select an option in a `<select>` element.
    fn select_option(&mut self, selector: &str, value: &str) -> anyhow::Result<InteractionResult>;
}

/// Tab lifecycle management.
#[async_trait::async_trait]
pub trait TabLifecycle: Send + Sync {
    /// Create a new tab (does not load it).
    fn create_tab(&mut self, url: &str) -> TabId;

    /// Create a tab with custom configuration.
    fn create_tab_with_config(&mut self, url: &str, config: TabConfig) -> TabId;

    /// Create, activate, and load a tab.
    async fn open_tab(&mut self, url: &str) -> anyhow::Result<&Tab>;

    /// Switch to a tab by ID.
    async fn switch_to(&mut self, id: TabId) -> anyhow::Result<&Tab>;

    /// Close a tab. Returns true if it was the active tab.
    fn close_tab(&mut self, id: TabId) -> bool;

    /// Close all tabs.
    fn close_all(&mut self);

    /// Close all tabs except the active one.
    fn close_others(&mut self);

    /// List all tabs.
    fn list_tabs(&self) -> Vec<&Tab>;

    /// Number of open tabs.
    fn tab_count(&self) -> usize;

    /// Get the currently active tab.
    fn active_tab(&self) -> Option<&Tab>;

    /// Get the active tab's URL.
    fn current_url(&self) -> Option<&str>;
}
