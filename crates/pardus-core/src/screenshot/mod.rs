//! Screenshot capture via headless Chromium.
//!
//! Feature-gated behind `#[cfg(feature = "screenshot")]`. Uses `chromiumoxide`
//! to spawn a headless Chrome process lazily, navigate to pages, and capture
//! PNG/JPEG screenshots of full pages or specific elements.
//!
//! The Chrome process is reused across multiple captures and cleaned up on drop.
//! If no Chrome binary is found on the system, one is auto-downloaded via
//! `chromiumoxide_fetcher` (cached after first download).

mod chrome;
mod capture;

use std::path::PathBuf;
use std::sync::Arc;

pub use chrome::ChromeManager;
pub use capture::{capture_full_page, capture_element};

/// Output format for screenshot captures.
#[derive(Debug, Clone, Default)]
pub enum ScreenshotFormat {
    #[default]
    Png,
    Jpeg { quality: u8 },
}

/// Options for a screenshot capture.
#[derive(Debug, Clone)]
pub struct ScreenshotOptions {
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub format: ScreenshotFormat,
    pub full_page: bool,
    pub timeout_ms: u64,
}

impl Default for ScreenshotOptions {
    fn default() -> Self {
        Self {
            viewport_width: 1280,
            viewport_height: 720,
            format: ScreenshotFormat::Png,
            full_page: false,
            timeout_ms: 10_000,
        }
    }
}

/// Thread-safe, `Send + Sync` handle for screenshot capture.
///
/// Wraps a `ChromeManager` (which manages the Chrome process lifecycle).
/// The Chrome process is launched lazily on first capture and reused thereafter.
#[derive(Clone)]
pub struct ScreenshotHandle {
    inner: Arc<tokio::sync::Mutex<ChromeManager>>,
}

impl ScreenshotHandle {
    /// Create a new handle. Does NOT launch Chrome yet.
    pub fn new(
        chrome_path: Option<PathBuf>,
        viewport_width: u32,
        viewport_height: u32,
    ) -> Self {
        Self {
            inner: Arc::new(tokio::sync::Mutex::new(ChromeManager::new(
                chrome_path,
                viewport_width,
                viewport_height,
            ))),
        }
    }

    /// Capture a full-page screenshot of the given URL.
    pub async fn capture_page(
        &self,
        url: &str,
        opts: &ScreenshotOptions,
    ) -> anyhow::Result<Vec<u8>> {
        let mut manager = self.inner.lock().await;
        manager.ensure_browser().await?;
        let browser = manager.browser().ok_or_else(|| anyhow::anyhow!("Chrome browser not available"))?;
        capture_full_page(browser, url, opts).await
    }

    /// Capture a screenshot of a specific element identified by CSS selector.
    pub async fn capture_element(
        &self,
        url: &str,
        selector: &str,
        opts: &ScreenshotOptions,
    ) -> anyhow::Result<Vec<u8>> {
        let mut manager = self.inner.lock().await;
        manager.ensure_browser().await?;
        let browser = manager.browser().ok_or_else(|| anyhow::anyhow!("Chrome browser not available"))?;
        capture_element(browser, url, selector, opts).await
    }
}
