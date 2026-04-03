//! Screenshot capture operations using chromiumoxide.

use std::time::Duration;

use chromiumoxide::browser::Browser;
use chromiumoxide::page::ScreenshotParams;
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;

use super::ScreenshotOptions;

/// Capture a full-page screenshot of the given URL.
pub async fn capture_full_page(
    browser: &Browser,
    url: &str,
    opts: &ScreenshotOptions,
) -> anyhow::Result<Vec<u8>> {
    let page = browser.new_page("about:blank").await
        .map_err(|e| anyhow::anyhow!("Failed to create Chrome page: {}", e))?;

    // Navigate with timeout
    let goto_result = tokio::time::timeout(
        Duration::from_millis(opts.timeout_ms),
        page.goto(url),
    ).await;

    match goto_result {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            let _ = page.close().await;
            anyhow::bail!("Navigation to '{}' failed: {}", url, e);
        }
        Err(_) => {
            let _ = page.close().await;
            anyhow::bail!("Navigation to '{}' timed out after {}ms", url, opts.timeout_ms);
        }
    }

    // Wait for page to settle
    tokio::time::sleep(Duration::from_millis(500)).await;

    let (cdp_format, quality) = match &opts.format {
        super::ScreenshotFormat::Png => (CaptureScreenshotFormat::Png, None),
        super::ScreenshotFormat::Jpeg { quality: q } => (CaptureScreenshotFormat::Jpeg, Some(*q)),
    };

    let mut params_builder = ScreenshotParams::builder()
        .format(cdp_format)
        .full_page(opts.full_page);
    if let Some(q) = quality {
        params_builder = params_builder.quality(q);
    }
    let params = params_builder;

    let bytes = tokio::time::timeout(
        Duration::from_secs(30),
        page.screenshot(params.build()),
    ).await
        .map_err(|_| anyhow::anyhow!("Screenshot capture timed out"))?
        .map_err(|e| anyhow::anyhow!("Screenshot capture failed: {}", e))?;

    let _ = page.close().await;

    Ok(bytes)
}

/// Capture a screenshot of a specific element identified by CSS selector.
pub async fn capture_element(
    browser: &Browser,
    url: &str,
    selector: &str,
    opts: &ScreenshotOptions,
) -> anyhow::Result<Vec<u8>> {
    let page = browser.new_page("about:blank").await
        .map_err(|e| anyhow::anyhow!("Failed to create Chrome page: {}", e))?;

    // Navigate
    let goto_result = tokio::time::timeout(
        Duration::from_millis(opts.timeout_ms),
        page.goto(url),
    ).await;

    match goto_result {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            let _ = page.close().await;
            anyhow::bail!("Navigation to '{}' failed: {}", url, e);
        }
        Err(_) => {
            let _ = page.close().await;
            anyhow::bail!("Navigation to '{}' timed out after {}ms", url, opts.timeout_ms);
        }
    }

    // Wait for page to settle
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Find the element
    let element = page.find_element(selector).await
        .map_err(|e| anyhow::anyhow!("Element '{}' not found: {}", selector, e))?;

    // Scroll into view
    element.scroll_into_view().await
        .map_err(|e| anyhow::anyhow!("Failed to scroll element into view: {}", e))?;

    tokio::time::sleep(Duration::from_millis(200)).await;

    let (cdp_format, quality) = match &opts.format {
        super::ScreenshotFormat::Png => (CaptureScreenshotFormat::Png, None),
        super::ScreenshotFormat::Jpeg { quality: q } => (CaptureScreenshotFormat::Jpeg, Some(*q)),
    };

    let bytes = tokio::time::timeout(
        Duration::from_secs(30),
        element.screenshot(cdp_format),
    ).await
        .map_err(|_| anyhow::anyhow!("Element screenshot timed out"))?
        .map_err(|e| anyhow::anyhow!("Element screenshot failed: {}", e))?;

    let _ = page.close().await;

    Ok(bytes)
}
