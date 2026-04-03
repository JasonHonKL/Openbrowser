//! Chrome process management for screenshot capture.

use std::path::PathBuf;

use chromiumoxide::browser::{Browser, BrowserConfig};

/// Manages the lifecycle of a headless Chrome process for screenshots.
///
/// The process is launched lazily (on first `ensure_browser()` call) and
/// reused across multiple captures. Cleaned up on `Drop`.
pub struct ChromeManager {
    browser: Option<Browser>,
    handler_task: Option<tokio::task::JoinHandle<()>>,
    chrome_path: Option<PathBuf>,
    viewport_width: u32,
    viewport_height: u32,
}

impl ChromeManager {
    pub fn new(chrome_path: Option<PathBuf>, viewport_width: u32, viewport_height: u32) -> Self {
        Self {
            browser: None,
            handler_task: None,
            chrome_path,
            viewport_width,
            viewport_height,
        }
    }

    /// Ensure a Chrome browser is running, launching one if needed.
    pub async fn ensure_browser(&mut self) -> anyhow::Result<()> {
        if self.browser.is_some() {
            return Ok(());
        }

        let executable = self.resolve_chrome_executable()?;

        let config = BrowserConfig::builder()
            .chrome_executable(executable)
            .window_size(self.viewport_width, self.viewport_height)
            .no_sandbox()
            .arg("--disable-gpu")
            .arg("--disable-dev-shm-usage")
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build Chrome config: {}", e))?;

        let (browser, mut handler) = Browser::launch(config).await?;

        // Spawn a task to drive the CDP connection
        let handle = tokio::spawn(async move {
            use futures_util::StreamExt;
            while let Some(_event) = handler.next().await {}
        });

        self.browser = Some(browser);
        self.handler_task = Some(handle);

        tracing::info!(
            "Headless Chrome launched for screenshots (viewport {}x{})",
            self.viewport_width,
            self.viewport_height
        );

        Ok(())
    }

    /// Get a reference to the running browser, if any.
    pub fn browser(&self) -> Option<&Browser> {
        self.browser.as_ref()
    }

    /// Resolve the Chrome/Chromium executable path.
    ///
    /// Resolution order:
    /// 1. Explicit path from config (`screenshot_chrome_path`)
    /// 2. System Chrome/Chromium via `which`
    fn resolve_chrome_executable(&self) -> anyhow::Result<PathBuf> {
        // 1. Explicit path
        if let Some(ref path) = self.chrome_path {
            if path.exists() {
                tracing::info!("Using Chrome from config: {}", path.display());
                return Ok(path.clone());
            } else {
                anyhow::bail!(
                    "Configured Chrome path does not exist: {}",
                    path.display()
                );
            }
        }

        // 2. Search for system Chrome/Chromium
        let candidates = ["chromium", "google-chrome", "google-chrome-stable", "chrome"];

        for name in &candidates {
            if let Ok(path) = which::which(name) {
                tracing::info!("Found system Chrome: {}", path.display());
                return Ok(path);
            }
        }

        // 3. Check common macOS paths
        let macos_paths = [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ];
        for p in &macos_paths {
            let path = PathBuf::from(p);
            if path.exists() {
                tracing::info!("Found Chrome at: {}", path.display());
                return Ok(path);
            }
        }

        anyhow::bail!(
            "Chrome/Chromium not found. Install Chrome or set `screenshot_chrome_path` in config. \
             Searched: [{}], and common macOS paths.",
            candidates.join(", ")
        )
    }
}

impl Drop for ChromeManager {
    fn drop(&mut self) {
        if let Some(handle) = self.handler_task.take() {
            handle.abort();
        }
        // Browser::drop kills the Chrome process
        if self.browser.take().is_some() {
            tracing::debug!("Chrome process cleaned up");
        }
    }
}
