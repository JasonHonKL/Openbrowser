use anyhow::Result;
use pardus_core::{Browser, BrowserConfig, FormState, ProxyConfig, ScrollDirection};
use rustyline::error::ReadlineError;
use rustyline::Editor;

use crate::OutputFormatArg;

pub async fn run_with_config(js: bool, format: OutputFormatArg, wait_ms: u32, proxy_config: ProxyConfig) -> Result<()> {
    let mut browser_config = BrowserConfig::default();
    browser_config.proxy = proxy_config;
    let mut browser = Browser::new(browser_config)?;
    let mut format = format;
    let mut js_enabled = js;
    let mut wait_ms = wait_ms;

    let mut rl = Editor::<(), rustyline::history::DefaultHistory>::new()?;

    println!("pardus-browser repl — type `help` for commands, `exit` to quit");

    loop {
        let prompt = match browser.current_url() {
            Some(url) => {
                let short = if url.len() > 50 {
                    format!("…{}", &url[url.len() - 47..])
                } else {
                    url.to_string()
                };
                format!("pardus [{}]> ", short)
            }
            None => "pardus> ".to_string(),
        };

        let line = match rl.readline(&prompt) {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => {
                println!("(use `exit` to quit)");
                continue;
            }
            Err(ReadlineError::Eof) => break,
            Err(e) => return Err(e.into()),
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let _ = rl.add_history_entry(trimmed);

        let tokens = split_tokens(trimmed);

        if tokens.is_empty() {
            continue;
        }

        match tokens[0].as_str() {
            "exit" | "quit" => break,
            "help" => print_help(),

            // Navigation
            "visit" | "open" => {
                if tokens.len() < 2 {
                    eprintln!("Usage: visit <url>");
                    continue;
                }
                let url = &tokens[1];
                match navigate(&mut browser, url, js_enabled, wait_ms).await {
                    Ok(()) => print_tree(&browser, &format),
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            "reload" => {
                match browser.reload().await {
                    Ok(_) => print_tree(&browser, &format),
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            "back" => {
                match browser.go_back().await {
                    Ok(Some(_)) => print_tree(&browser, &format),
                    Ok(None) => println!("Already at the beginning of history"),
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            "forward" => {
                match browser.go_forward().await {
                    Ok(Some(_)) => print_tree(&browser, &format),
                    Ok(None) => println!("Already at the end of history"),
                    Err(e) => eprintln!("Error: {}", e),
                }
            }

            // Interactions
            "click" => {
                if tokens.len() < 2 {
                    eprintln!("Usage: click <selector|#id>");
                    continue;
                }
                let selector = &tokens[1];
                if let Some(id_str) = selector.strip_prefix('#') {
                    match id_str.parse::<usize>() {
                        Ok(id) => match browser.click_by_id(id).await {
                            Ok(result) => print_interaction_result(&result, &format),
                            Err(e) => eprintln!("Error: {}", e),
                        },
                        Err(_) => eprintln!("Invalid element ID: {}", selector),
                    }
                } else {
                    match browser.click(selector).await {
                        Ok(result) => print_interaction_result(&result, &format),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
            }
            "type" => {
                if tokens.len() < 3 {
                    eprintln!("Usage: type <selector|#id> <value>");
                    continue;
                }
                let selector = &tokens[1];
                let value = &tokens[2];
                if let Some(id_str) = selector.strip_prefix('#') {
                    match id_str.parse::<usize>() {
                        Ok(id) => match browser.type_by_id(id, value).await {
                            Ok(result) => print_interaction_result(&result, &format),
                            Err(e) => eprintln!("Error: {}", e),
                        },
                        Err(_) => eprintln!("Invalid element ID: {}", selector),
                    }
                } else {
                    match browser.type_text(selector, value).await {
                        Ok(result) => print_interaction_result(&result, &format),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
            }
            "submit" => {
                if tokens.len() < 2 {
                    eprintln!("Usage: submit <selector> [name=value ...]");
                    continue;
                }
                let mut state = FormState::new();
                for f in &tokens[2..] {
                    let parts: Vec<&str> = f.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        state.set(parts[0], parts[1]);
                    } else {
                        eprintln!("Invalid field '{}', expected name=value", f);
                    }
                }
                match browser.submit(&tokens[1], &state).await {
                    Ok(result) => print_interaction_result(&result, &format),
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            "scroll" => {
                let dir = tokens.get(1).map(|s| s.as_str()).unwrap_or("down");
                let direction = match dir {
                    "up" => ScrollDirection::Up,
                    "to-top" => ScrollDirection::ToTop,
                    "to-bottom" => ScrollDirection::ToBottom,
                    _ => ScrollDirection::Down,
                };
                match browser.scroll(direction).await {
                    Ok(result) => print_interaction_result(&result, &format),
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            "wait" => {
                if tokens.len() < 2 {
                    eprintln!("Usage: wait <selector> [timeout_ms]");
                    continue;
                }
                let timeout: u32 = tokens.get(2).and_then(|s| s.parse().ok()).unwrap_or(5000);
                match browser.wait_for(&tokens[1], timeout).await {
                    Ok(result) => print_interaction_result(&result, &format),
                    Err(e) => eprintln!("Error: {}", e),
                }
            }

            // Tab management
            "tab" => {
                handle_tab(&mut browser, &tokens[1..], js_enabled, wait_ms, &format).await;
            }

            // Settings
            "js" => {
                match tokens.get(1).map(|s| s.as_str()) {
                    Some("on") | Some("true") | Some("1") => {
                        js_enabled = true;
                        println!("JS enabled");
                    }
                    Some("off") | Some("false") | Some("0") => {
                        js_enabled = false;
                        println!("JS disabled");
                    }
                    _ => println!("JS is currently {}", if js_enabled { "on" } else { "off" }),
                }
            }
            "format" => {
                match tokens.get(1).map(|s| s.as_str()) {
                    Some("md") => format = OutputFormatArg::Md,
                    Some("tree") => format = OutputFormatArg::Tree,
                    Some("json") => format = OutputFormatArg::Json,
                    _ => {
                        eprintln!("Usage: format md|tree|json");
                        continue;
                    }
                }
                println!("Format set to {:?}", format);
            }
            "wait-ms" => {
                if let Some(ms) = tokens.get(1).and_then(|s| s.parse::<u32>().ok()) {
                    wait_ms = ms;
                    println!("JS wait time set to {}ms", wait_ms);
                } else {
                    eprintln!("Usage: wait-ms <milliseconds>");
                }
            }

            other => {
                eprintln!("Unknown command: {}. Type `help` for available commands.", other);
            }
        }
    }

    println!("Bye.");
    Ok(())
}

async fn navigate(
    browser: &mut Browser,
    url: &str,
    js: bool,
    wait_ms: u32,
) -> Result<()> {
    if js {
        browser.navigate_with_js(url, wait_ms).await?;
    } else {
        browser.navigate(url).await?;
    }
    Ok(())
}

fn print_tree(browser: &Browser, format: &OutputFormatArg) {
    let page = match browser.current_page() {
        Some(p) => p,
        None => {
            eprintln!("No page loaded");
            return;
        }
    };
    let tree = page.semantic_tree();
    match format {
        OutputFormatArg::Md => {
            let output = pardus_core::output::md_formatter::format_md(&tree);
            for line in output.lines() {
                if !line.trim().is_empty() {
                    println!("  {}", line);
                }
            }
        }
        OutputFormatArg::Tree => {
            let output = pardus_core::output::tree_formatter::format_tree(&tree);
            for line in output.lines() {
                if !line.trim().is_empty() {
                    println!("  {}", line);
                }
            }
        }
        OutputFormatArg::Json => {
            let json = pardus_core::output::json_formatter::format_json(
                &page.url,
                page.title(),
                &tree,
                None,
                None,
            )
            .unwrap_or_default();
            println!("{}", json);
        }
        OutputFormatArg::Llm => {
            let output = pardus_core::output::llm_formatter::format_llm(&tree);
            println!("{}", output);
        }
    }
    println!(
        "  {} landmarks, {} links, {} headings, {} actions",
        tree.stats.landmarks,
        tree.stats.links,
        tree.stats.headings,
        tree.stats.actions,
    );
}

fn print_interaction_result(
    result: &pardus_core::InteractionResult,
    format: &OutputFormatArg,
) {
    use pardus_core::InteractionResult;
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
        InteractionResult::Scrolled { url, page: new_page } => {
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
    }
}

async fn handle_tab(
    browser: &mut Browser,
    args: &[String],
    js: bool,
    wait_ms: u32,
    format: &OutputFormatArg,
) {
    if args.is_empty() {
        eprintln!("Usage: tab <list|open|switch|close|info>");
        return;
    }

    match args[0].as_str() {
        "list" | "ls" => {
            let tabs = browser.list_tabs();
            if tabs.is_empty() {
                println!("No tabs open");
                return;
            }
            let active = browser.active_tab().map(|t| t.id);
            println!("Tabs ({} total):", tabs.len());
            for tab in tabs {
                let title = tab.title.as_deref().unwrap_or("(no title)");
                let marker = if active == Some(tab.id) { "*" } else { " " };
                println!(
                    "  {} [{}] {:?} — {} — {}",
                    marker, tab.id, tab.state, title, tab.url
                );
            }
        }
        "open" => {
            if args.len() < 2 {
                eprintln!("Usage: tab open <url>");
                return;
            }
            let url = &args[1];
            if js {
                match browser.navigate_with_js(url, wait_ms).await {
                    Ok(tab) => println!(
                        "Opened tab {}: {}",
                        tab.id,
                        tab.title.as_deref().unwrap_or("(no title)")
                    ),
                    Err(e) => eprintln!("Error: {}", e),
                }
            } else {
                match browser.open_tab(url).await {
                    Ok(tab) => println!(
                        "Opened tab {}: {}",
                        tab.id,
                        tab.title.as_deref().unwrap_or("(no title)")
                    ),
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
        }
        "switch" => {
            if args.len() < 2 {
                eprintln!("Usage: tab switch <id>");
                return;
            }
            match args[1].parse::<u64>() {
                Ok(id) => {
                    let tab_id = pardus_core::TabId::from_u64(id);
                    match browser.switch_to(tab_id).await {
                        Ok(tab) => {
                            println!(
                                "Switched to tab {}: {}",
                                tab.id,
                                tab.url
                            );
                            print_tree(browser, format);
                        }
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Err(_) => eprintln!("Invalid tab ID: {}", args[1]),
            }
        }
        "close" => {
            let id = if args.len() >= 2 {
                match args[1].parse::<u64>() {
                    Ok(id) => Some(pardus_core::TabId::from_u64(id)),
                    Err(_) => {
                        eprintln!("Invalid tab ID: {}", args[1]);
                        return;
                    }
                }
            } else {
                browser.active_tab().map(|t| t.id)
            };

            if let Some(tab_id) = id {
                let was_active = browser.close_tab(tab_id);
                if was_active {
                    println!("Closed active tab (switched to next available)");
                } else {
                    println!("Closed tab {}", tab_id);
                }
            } else {
                eprintln!("No tab to close");
            }
        }
        "info" => {
            match browser.active_tab() {
                Some(tab) => {
                    println!("Active Tab [{}]:", tab.id);
                    println!("  URL: {}", tab.url);
                    println!("  Title: {}", tab.title.as_deref().unwrap_or("(none)"));
                    println!("  State: {:?}", tab.state);
                    println!("  History: {}/{}", tab.history_index + 1, tab.history.len());
                }
                None => println!("No active tab"),
            }
        }
        other => {
            eprintln!("Unknown tab command: {}. Use: list, open, switch, close, info", other);
        }
    }
}

/// Split a line into tokens by whitespace, respecting double-quoted strings.
/// Unlike shell_words::split, this does NOT treat # as a comment character.
fn split_tokens(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '"' {
            in_quotes = !in_quotes;
        } else if ch.is_whitespace() && !in_quotes {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn print_help() {
    println!("pardus-browser repl commands:");
    println!();
    println!("Navigation:");
    println!("  visit <url>          Navigate to URL (creates tab if none)");
    println!("  open <url>           Alias for visit");
    println!("  reload               Reload current page");
    println!("  back                 Go back in history");
    println!("  forward              Go forward in history");
    println!();
    println!("Interaction:");
    println!("  click <selector>     Click an element");
    println!("  type <sel> <value>   Type text into a field");
    println!("  submit <sel> [k=v..] Submit a form");
    println!("  scroll [dir]         Scroll (down/up/to-top/to-bottom)");
    println!("  wait <sel> [timeout] Wait for element to appear");
    println!();
    println!("Tabs:");
    println!("  tab list             List all tabs");
    println!("  tab open <url>       Open new tab");
    println!("  tab switch <id>      Switch to tab");
    println!("  tab close [id]       Close tab (active if no id)");
    println!("  tab info             Show active tab info");
    println!();
    println!("Settings:");
    println!("  js [on|off]          Toggle/show JS execution");
    println!("  format md|tree|json  Set output format");
    println!("  wait-ms <ms>         Set JS wait time");
    println!();
    println!("  help                 Show this help");
    println!("  exit                 Exit REPL");
}
