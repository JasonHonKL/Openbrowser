use std::{path::PathBuf, time::Instant};

use anyhow::Result;
use pardus_core::{BrowserConfig, FormState, InteractionResult, ScrollDirection};

use crate::{InteractAction, OutputFormatArg};

pub async fn run_with_config(
    url: &str,
    action: InteractAction,
    format: OutputFormatArg,
    js: bool,
    wait_ms: u32,
    browser_config: BrowserConfig,
) -> Result<()> {
    let start = Instant::now();
    let mut browser = pardus_core::Browser::new(browser_config)?;

    // Navigate first
    if js {
        browser.navigate_with_js(url, wait_ms).await?;
    } else {
        browser.navigate(url).await?;
    }

    let elapsed = start.elapsed().as_millis();
    eprintln!("Connected in {}ms", elapsed);

    match action {
        InteractAction::Click { selector } => {
            let result = browser.click(&selector).await?;
            output_result(&result, &format);
        }
        InteractAction::ClickId { id } => {
            let result = browser.click_by_id(id).await?;
            output_result(&result, &format);
        }
        InteractAction::Type { selector, value } => {
            let result = browser.type_text(&selector, &value).await?;
            output_result(&result, &format);
        }
        InteractAction::TypeId { id, value } => {
            let result = browser.type_by_id(id, &value).await?;
            output_result(&result, &format);
        }
        InteractAction::Submit { selector, field } => {
            let mut state = FormState::new();
            for f in &field {
                let parts: Vec<&str> = f.splitn(2, '=').collect();
                if parts.len() == 2 {
                    state.set(parts[0], parts[1]);
                } else {
                    eprintln!("Invalid field format '{}', expected name=value", f);
                }
            }
            let result = browser.submit(&selector, &state).await?;
            output_result(&result, &format);
        }
        InteractAction::Wait {
            selector,
            timeout_ms,
        } => {
            let result = browser.wait_for(&selector, timeout_ms).await?;
            output_result(&result, &format);
        }
        InteractAction::Scroll { direction } => {
            let dir = match direction.as_str() {
                "up" => ScrollDirection::Up,
                "to-top" => ScrollDirection::ToTop,
                "to-bottom" => ScrollDirection::ToBottom,
                _ => ScrollDirection::Down,
            };
            let result = browser.scroll(dir).await?;
            output_result(&result, &format);
        }
        InteractAction::DispatchEvent {
            selector,
            event_type,
            init,
        } => {
            let result = browser
                .dispatch_event(&selector, &event_type, init.as_deref())
                .await?;
            output_result(&result, &format);
        }
        InteractAction::Upload { selector, files } => {
            let paths: Vec<PathBuf> = files.iter().map(|f| PathBuf::from(f)).collect();
            let result = browser.upload(&selector, paths)?;
            output_result(&result, &format);
        }
        InteractAction::UploadId { id, files } => {
            let paths: Vec<PathBuf> = files.iter().map(|f| PathBuf::from(f)).collect();
            let result = browser.upload_by_id(id, paths)?;
            output_result(&result, &format);
        }
    }

    Ok(())
}

fn output_result(result: &InteractionResult, format: &OutputFormatArg) {
    match result {
        InteractionResult::Navigated(new_page) => {
            eprintln!("Navigated to: {}", new_page.url);
            let tree = new_page.semantic_tree();
            match format {
                OutputFormatArg::Md => {
                    let output = pardus_core::output::md_formatter::format_md(&tree);
                    println!("{}", output);
                }
                OutputFormatArg::Tree => {
                    let output = pardus_core::output::tree_formatter::format_tree(&tree);
                    println!("{}", output);
                }
                OutputFormatArg::Json => {
                    let json = pardus_core::output::json_formatter::format_json(
                        &new_page.url,
                        new_page.title(),
                        &tree,
                        None,
                        None,
                        new_page.redirect_chain.as_ref(),
                    )
                    .unwrap_or_default();
                    println!("{}", json);
                }
                OutputFormatArg::Llm => {
                    let output = pardus_core::output::llm_formatter::format_llm(&tree);
                    println!("{}", output);
                }
            }
        }
        InteractionResult::Typed { selector, value } => {
            println!("Typed '{}' into {}", value, selector);
        }
        InteractionResult::Toggled { selector, checked } => {
            println!(
                "Toggled {} -> {}",
                selector,
                if *checked { "checked" } else { "unchecked" }
            );
        }
        InteractionResult::Selected { selector, value } => {
            println!("Selected '{}' in {}", value, selector);
        }
        InteractionResult::ElementNotFound { selector, reason } => {
            eprintln!("Element not found: {} — {}", selector, reason);
        }
        InteractionResult::WaitSatisfied { selector, found } => {
            if *found {
                println!("Wait satisfied: {} found", selector);
            } else {
                eprintln!("Wait timeout: {} not found", selector);
            }
        }
        InteractionResult::Scrolled {
            url,
            page: new_page,
        } => {
            eprintln!("Scrolled to: {}", url);
            let tree = new_page.semantic_tree();
            match format {
                OutputFormatArg::Md => {
                    let output = pardus_core::output::md_formatter::format_md(&tree);
                    println!("{}", output);
                }
                OutputFormatArg::Tree => {
                    let output = pardus_core::output::tree_formatter::format_tree(&tree);
                    println!("{}", output);
                }
                OutputFormatArg::Json => {
                    let json = pardus_core::output::json_formatter::format_json(
                        &new_page.url,
                        new_page.title(),
                        &tree,
                        None,
                        None,
                        new_page.redirect_chain.as_ref(),
                    )
                    .unwrap_or_default();
                    println!("{}", json);
                }
                OutputFormatArg::Llm => {
                    let output = pardus_core::output::llm_formatter::format_llm(&tree);
                    println!("{}", output);
                }
            }
        }
        InteractionResult::EventDispatched {
            selector,
            event_type,
        } => {
            println!("Dispatched '{}' on {}", event_type, selector);
        }
        InteractionResult::FilesSet { selector, count } => {
            println!("Set {} file(s) on {}", count, selector);
        }
    }
}
