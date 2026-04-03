//! Screenshot capture command.

use anyhow::Result;
use std::path::PathBuf;

use pardus_core::screenshot::{ScreenshotFormat, ScreenshotOptions};

pub async fn run(
    url: &str,
    output: &PathBuf,
    element: Option<&str>,
    full_page: bool,
    viewport: &str,
    format: &str,
    quality: Option<u8>,
    chrome_path: Option<&PathBuf>,
    timeout_ms: u64,
) -> Result<()> {
    let mut browser_config = pardus_core::BrowserConfig::default();

    if let Some(path) = chrome_path {
        browser_config.screenshot_chrome_path = Some(path.clone());
    }

    // Parse viewport (e.g., "1920x1080")
    let (vw, vh) = parse_viewport(viewport);
    browser_config.viewport_width = vw;
    browser_config.viewport_height = vh;

    let screenshot_format = match format {
        "jpeg" | "jpg" => ScreenshotFormat::Jpeg {
            quality: quality.unwrap_or(85),
        },
        _ => ScreenshotFormat::Png,
    };

    let opts = ScreenshotOptions {
        viewport_width: vw,
        viewport_height: vh,
        format: screenshot_format,
        full_page,
        timeout_ms,
    };

    let browser = pardus_core::Browser::new(browser_config)?;

    let bytes = if let Some(selector) = element {
        eprintln!("Capturing element '{}' from {}...", selector, url);
        browser.capture_element_screenshot(url, selector, &opts).await?
    } else {
        eprintln!("Capturing {}...", url);
        browser.capture_screenshot(url, &opts).await?
    };

    std::fs::write(output, &bytes)?;
    eprintln!("Screenshot saved to {} ({} bytes)", output.display(), bytes.len());

    Ok(())
}

fn parse_viewport(viewport: &str) -> (u32, u32) {
    let parts: Vec<&str> = viewport.split('x').collect();
    if parts.len() == 2 {
        let w = parts[0].parse().unwrap_or(1280);
        let h = parts[1].parse().unwrap_or(720);
        (w, h)
    } else {
        (1280, 720)
    }
}
