use serde::Serialize;

/// Events pushed over the WebSocket to connected UI clients.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerEvent {
    #[serde(rename = "navigation.started")]
    NavigationStarted { tab_id: u64, url: String },
    #[serde(rename = "navigation.completed")]
    NavigationCompleted { tab_id: u64, status: u16, url: String },
    #[serde(rename = "navigation.failed")]
    NavigationFailed { tab_id: u64, error: String },
    #[serde(rename = "reloaded")]
    Reloaded,
    #[serde(rename = "tab.opened")]
    TabOpened { url: String },
    #[serde(rename = "tab.closed")]
    TabClosed { id: u64 },
    #[serde(rename = "tab.activated")]
    TabActivated { id: u64 },
}
