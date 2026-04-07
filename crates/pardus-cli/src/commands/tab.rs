use anyhow::Result;

use crate::OutputFormatArg;

/// Run the tab list command
pub async fn list(browser: &pardus_core::Browser, format: OutputFormatArg) -> Result<()> {
    match format {
        OutputFormatArg::Json => {
            let tabs: Vec<_> = browser.list_tabs().iter().map(|t| t.info()).collect();
            println!("{}", serde_json::to_string_pretty(&tabs)?);
        }
        _ => {
            let tabs = browser.list_tabs();
            if tabs.is_empty() {
                println!("No tabs open");
                return Ok(());
            }
            println!("Tabs ({} total):", tabs.len());
            for tab in tabs {
                let title = tab.title.as_deref().unwrap_or("(no title)");
                println!("  {} {:?} — {}", tab.id, tab.state, title);
            }
        }
    }
    Ok(())
}

/// Open a new tab with proxy configuration
pub async fn open_with_config(
    url: &str,
    js: bool,
    proxy_config: pardus_core::ProxyConfig,
) -> Result<()> {
    let mut browser_config = pardus_core::BrowserConfig::default();
    browser_config.proxy = proxy_config;
    let mut browser = pardus_core::Browser::new(browser_config)?;
    let tab = if js {
        browser.navigate_with_js(url, 3000).await?
    } else {
        browser.navigate(url).await?
    };
    println!(
        "Tab {}: {} — {}",
        tab.id,
        tab.url,
        tab.title.as_deref().unwrap_or("(no title)")
    );
    Ok(())
}

/// Navigate the active tab with proxy configuration
pub async fn navigate_with_config(url: &str, proxy_config: pardus_core::ProxyConfig) -> Result<()> {
    let mut browser_config = pardus_core::BrowserConfig::default();
    browser_config.proxy = proxy_config;
    let mut browser = pardus_core::Browser::new(browser_config)?;
    browser.navigate(url).await?;
    if let Some(tab) = browser.active_tab() {
        println!("Navigated to: {}", tab.url);
    }
    Ok(())
}

/// Show active tab info
pub fn info(browser: &pardus_core::Browser, format: OutputFormatArg) -> Result<()> {
    match browser.active_tab() {
        Some(tab) => {
            match format {
                OutputFormatArg::Json => {
                    println!("{}", serde_json::to_string_pretty(&tab.info())?);
                }
                _ => {
                    println!("Active Tab {}:", tab.id);
                    println!("  URL: {}", tab.url);
                    println!("  Title: {}", tab.title.as_deref().unwrap_or("(none)"));
                    println!("  State: {:?}", tab.state);
                    println!("  History: {}/{}", tab.history_index + 1, tab.history.len());
                }
            }
            Ok(())
        }
        None => {
            anyhow::bail!("No active tab");
        }
    }
}
