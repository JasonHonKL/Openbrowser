use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::protocol::event_bus::EventBus;
use crate::protocol::node_map::NodeMap;
use crate::protocol::target::CdpSession;
use crate::error::{CdpError, CdpErrorBody};
use crate::protocol::message::CdpErrorResponse;

/// A Send+Sync entry for a CDP target (tab).
/// Stores raw HTML rather than parsed Page to avoid !Send types from scraper.
#[derive(Debug, Clone)]
pub struct TargetEntry {
    pub url: String,
    pub html: Option<String>,
    pub title: Option<String>,
    pub js_enabled: bool,
    /// Serialized `FrameTree` JSON. Populated when iframe parsing is enabled.
    pub frame_tree_json: Option<String>,
}

/// Shared state available to all domain handlers. All fields are Send+Sync.
pub struct DomainContext {
    /// The App instance (HTTP client, config, network log).
    pub app: Arc<pardus_core::App>,
    /// Target store: target_id -> TargetEntry.
    /// Stores raw HTML rather than parsed Page to avoid !Send types from scraper.
    pub targets: Arc<Mutex<HashMap<String, TargetEntry>>>,
    /// Event bus sender for pushing events to clients.
    pub event_bus: Arc<EventBus>,
    /// Node map for this session (backendNodeId <-> selector).
    pub node_map: Arc<Mutex<NodeMap>>,
    /// Screenshot handle for captureScreenshot support (feature-gated).
    #[cfg(feature = "screenshot")]
    pub screenshot_handle: pardus_core::screenshot::ScreenshotHandle,
}

impl DomainContext {
    /// Create a new DomainContext with the given App.
    pub fn new(
        app: Arc<pardus_core::App>,
        targets: Arc<Mutex<HashMap<String, TargetEntry>>>,
        event_bus: Arc<EventBus>,
        node_map: Arc<Mutex<NodeMap>>,
    ) -> Self {
        #[cfg(feature = "screenshot")]
        let screenshot_handle = {
            let config = app.config.read();
            pardus_core::screenshot::ScreenshotHandle::new(
                config.screenshot_chrome_path.clone(),
                config.viewport_width,
                config.viewport_height,
            )
        };
        Self {
            app,
            targets,
            event_bus,
            node_map,
            #[cfg(feature = "screenshot")]
            screenshot_handle,
        }
    }

    /// Create a temporary Browser from the App configuration.
    /// 
    /// This allows using the unified Browser API while keeping DomainContext Send+Sync.
    /// The Browser is created on-demand and not stored in DomainContext.
    pub fn create_browser(&self) -> pardus_core::Browser {
        let config = self.app.config_snapshot();
        pardus_core::Browser::new(config)
            .expect("failed to create Browser")
    }
}

impl DomainContext {
    pub async fn get_html(&self, target_id: &str) -> Option<String> {
        let targets = self.targets.lock().await;
        targets.get(target_id).and_then(|e| e.html.clone())
    }

    pub async fn get_url(&self, target_id: &str) -> Option<String> {
        let targets = self.targets.lock().await;
        targets.get(target_id).map(|e| e.url.clone())
    }

    pub async fn get_title(&self, target_id: &str) -> Option<String> {
        let targets = self.targets.lock().await;
        targets.get(target_id).and_then(|e| e.title.clone())
    }

    pub async fn get_target_entry(&self, target_id: &str) -> Option<TargetEntry> {
        let targets = self.targets.lock().await;
        targets.get(target_id).cloned()
    }

    /// Navigate to a URL using the App API.
    /// 
    /// Note: Uses App directly rather than Browser because Browser contains
    /// !Send types (scraper::Html in Page) that cannot be held across await
    /// points in CDP handlers which must be Send.
    pub async fn navigate(&self, target_id: &str, url: &str) -> anyhow::Result<()> {
        let page = match pardus_core::Page::from_url_with_js(&self.app, url, 3000).await {
            Ok(p) => p,
            Err(_) => pardus_core::Page::from_url(&self.app, url).await?,
        };
        let frame_tree_json = page.frame_tree.as_ref()
            .and_then(|ft| serde_json::to_string(ft).ok());
        let final_url = page.url.clone();
        let html_str = page.html.html().to_string();
        let title = page.title();
        let mut targets = self.targets.lock().await;
        targets.insert(target_id.to_string(), TargetEntry {
            url: final_url,
            html: Some(html_str),
            title,
            js_enabled: true,
            frame_tree_json,
        });
        Ok(())
    }
    
    /// Reload a target using the App API.
    pub async fn reload(&self, target_id: &str) -> anyhow::Result<()> {
        let url = {
            let targets = self.targets.lock().await;
            targets.get(target_id)
                .map(|t| t.url.clone())
                .unwrap_or_else(|| "about:blank".to_string())
        };
        
        self.navigate(target_id, &url).await
    }

    pub fn update_target_with_data(&self, target_id: &str, url: String, html: String, title: Option<String>) {
        let mut targets = self.targets.blocking_lock();
        targets.insert(target_id.to_string(), TargetEntry {
            url,
            html: Some(html),
            title,
            js_enabled: false,
            frame_tree_json: None,
        });
    }

    pub async fn get_frame_tree_json(&self, target_id: &str) -> Option<String> {
        let targets = self.targets.lock().await;
        targets.get(target_id).and_then(|e| e.frame_tree_json.clone())
    }
}

pub enum HandleResult {
    Success(Value),
    Error(CdpErrorResponse),
    Ack,
}

impl HandleResult {
    pub fn with_request_id(self, id: u64) -> Self {
        match self {
            HandleResult::Success(v) => HandleResult::Success(v),
            HandleResult::Error(err) => HandleResult::Error(CdpErrorResponse {
                id,
                ..err
            }),
            HandleResult::Ack => HandleResult::Ack,
        }
    }
}

#[async_trait(?Send)]
pub trait CdpDomainHandler: Send + Sync {
    fn domain_name(&self) -> &'static str;

    async fn handle(
        &self,
        method: &str,
        params: Value,
        session: &mut CdpSession,
        ctx: &DomainContext,
    ) -> HandleResult;
}

pub fn method_not_found(domain: &str, method: &str) -> HandleResult {
    HandleResult::Error(CdpErrorResponse {
        id: 0,
        error: CdpErrorBody::from(&CdpError::MethodNotFound(format!("{}.{} not found", domain, method))),
        session_id: None,
    })
}

pub mod browser;
pub mod console;
pub mod css;
pub mod dom;
pub mod emulation;
pub mod input;
pub mod log;
pub mod network;
pub mod pardus_ext;
pub mod page;
pub mod performance;
pub mod runtime;
pub mod security;
pub mod target;
