use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Trait for pluggable screenshot providers.
///
/// Pardus never renders pixels itself — it delegates to an external service
/// that receives page state and returns a base64-encoded image.
#[async_trait::async_trait]
pub trait ScreenshotProvider: Send + Sync {
    async fn capture(&self, request: ScreenshotRequest) -> Result<ScreenshotResult, ScreenshotError>;
}

/// Page state sent to the external screenshot provider.
#[derive(Debug, Serialize)]
pub struct ScreenshotRequest {
    pub url: String,
    pub html: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cookies: Vec<CookieEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<(String, String)>,
    pub viewport: Viewport,
    pub format: ScreenshotFormat,
}

/// Cookie entry for the screenshot request.
#[derive(Debug, Serialize)]
pub struct CookieEntry {
    pub name: String,
    pub value: String,
}

/// Viewport dimensions for the screenshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    #[serde(default = "default_dsf")]
    pub device_scale_factor: f64,
}

fn default_dsf() -> f64 {
    1.0
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            device_scale_factor: 1.0,
        }
    }
}

/// Image format for screenshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScreenshotFormat {
    Png,
    Jpeg,
    Webp,
}

impl Default for ScreenshotFormat {
    fn default() -> Self {
        Self::Png
    }
}

/// Result from the screenshot provider.
#[derive(Debug, Serialize, Deserialize)]
pub struct ScreenshotResult {
    /// Base64-encoded image data.
    pub data: String,
    pub format: ScreenshotFormat,
    pub width: u32,
    pub height: u32,
}

/// Errors from screenshot providers.
#[derive(Debug, thiserror::Error)]
pub enum ScreenshotError {
    #[error("No screenshot provider configured. Use --screenshot-endpoint to enable.")]
    ProviderUnavailable,
    #[error("Screenshot provider request failed: {0}")]
    RequestFailed(String),
    #[error("Screenshot provider returned invalid response: {0}")]
    InvalidResponse(String),
}

// ---------------------------------------------------------------------------
// Built-in providers
// ---------------------------------------------------------------------------

/// Provider that sends page state to an external HTTP endpoint.
pub struct HttpScreenshotProvider {
    endpoint: String,
    client: rquest::Client,
    timeout: Duration,
}

impl HttpScreenshotProvider {
    /// Create a new HttpScreenshotProvider with a shared HTTP client.
    /// The client should be configured with connection pooling settings.
    pub fn new(client: rquest::Client, endpoint: &str, timeout_ms: u64) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            client,
            timeout: Duration::from_millis(timeout_ms),
        }
    }
}

#[async_trait::async_trait]
impl ScreenshotProvider for HttpScreenshotProvider {
    async fn capture(&self, request: ScreenshotRequest) -> Result<ScreenshotResult, ScreenshotError> {
        let response = self.client
            .post(&self.endpoint)
            .json(&request)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| ScreenshotError::RequestFailed(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ScreenshotError::RequestFailed(
                format!("HTTP {}: {}", status, body)
            ));
        }

        let result: ScreenshotResult = response
            .json()
            .await
            .map_err(|e| ScreenshotError::InvalidResponse(e.to_string()))?;

        Ok(result)
    }
}

/// No-op provider that returns an error when screenshots are requested.
pub struct NoopScreenshotProvider;

#[async_trait::async_trait]
impl ScreenshotProvider for NoopScreenshotProvider {
    async fn capture(&self, _request: ScreenshotRequest) -> Result<ScreenshotResult, ScreenshotError> {
        Err(ScreenshotError::ProviderUnavailable)
    }
}
