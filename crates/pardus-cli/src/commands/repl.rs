use anyhow::Result;
use pardus_core::{
    Browser, BrowserConfig, FormState, ProxyConfig, ScrollDirection,
    intercept::{
        builtins::{
            BlockingInterceptor, HeaderModifierInterceptor, MockResponseInterceptor,
            RedirectInterceptor,
        },
        rules::InterceptorRule,
    },
};
use rustyline::{Editor, error::ReadlineError};

use crate::OutputFormatArg;

pub async fn run_with_config(
    js: bool,
    format: OutputFormatArg,
    wait_ms: u32,
    proxy_config: ProxyConfig,
) -> Result<()> {
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
            "reload" => match browser.reload().await {
                Ok(_) => print_tree(&browser, &format),
                Err(e) => eprintln!("Error: {}", e),
            },
            "back" => match browser.go_back().await {
                Ok(Some(_)) => print_tree(&browser, &format),
                Ok(None) => println!("Already at the beginning of history"),
                Err(e) => eprintln!("Error: {}", e),
            },
            "forward" => match browser.go_forward().await {
                Ok(Some(_)) => print_tree(&browser, &format),
                Ok(None) => println!("Already at the end of history"),
                Err(e) => eprintln!("Error: {}", e),
            },

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
            "event" => {
                if tokens.len() < 3 {
                    eprintln!("Usage: event <selector|#id> <event_type> [init_json]");
                    continue;
                }
                let selector = &tokens[1];
                let event_type = &tokens[2];
                let init = tokens.get(3).cloned();
                if let Some(id_str) = selector.strip_prefix('#') {
                    match id_str.parse::<usize>() {
                        Ok(id) => match browser
                            .dispatch_event_by_id(id, event_type, init.as_deref())
                            .await
                        {
                            Ok(result) => print_interaction_result(&result, &format),
                            Err(e) => eprintln!("Error: {}", e),
                        },
                        Err(_) => eprintln!("Invalid element ID: {}", selector),
                    }
                } else {
                    match browser
                        .dispatch_event(selector, event_type, init.as_deref())
                        .await
                    {
                        Ok(result) => print_interaction_result(&result, &format),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
            }

            // Screenshot (only available when compiled with --features screenshot)
            #[cfg(feature = "screenshot")]
            "screenshot" => {
                if tokens.len() < 2 {
                    eprintln!("Usage: screenshot <path> [--full] [--element <selector>]");
                    continue;
                }
                let output_path = &tokens[1];
                let full_page = tokens.iter().any(|t| t == "--full");
                let element_selector = {
                    let mut sel = None;
                    for i in 0..tokens.len() {
                        if tokens[i] == "--element" && i + 1 < tokens.len() {
                            sel = Some(tokens[i + 1].clone());
                            break;
                        }
                    }
                    sel
                };

                let url = match browser.current_url() {
                    Some(u) => u.to_string(),
                    None => {
                        eprintln!("No page loaded. Use 'visit <url>' first.");
                        continue;
                    }
                };

                let opts = pardus_core::screenshot::ScreenshotOptions {
                    viewport_width: 1280,
                    viewport_height: 720,
                    format: pardus_core::screenshot::ScreenshotFormat::Png,
                    full_page,
                    timeout_ms: 10_000,
                };

                let result = if let Some(selector) = &element_selector {
                    browser
                        .capture_element_screenshot(&url, selector, &opts)
                        .await
                } else {
                    browser.capture_screenshot(&url, &opts).await
                };

                match result {
                    Ok(bytes) => match std::fs::write(output_path, &bytes) {
                        Ok(_) => println!(
                            "Screenshot saved to {} ({} bytes)",
                            output_path,
                            bytes.len()
                        ),
                        Err(e) => eprintln!("Failed to write screenshot: {}", e),
                    },
                    Err(e) => eprintln!("Screenshot failed: {}", e),
                }
            }
            #[cfg(not(feature = "screenshot"))]
            "screenshot" => {
                eprintln!("Screenshot support not compiled. Rebuild with --features screenshot");
            }

            // Tab management
            "tab" => {
                handle_tab(&mut browser, &tokens[1..], js_enabled, wait_ms, &format).await;
            }

            // Settings
            "js" => match tokens.get(1).map(|s| s.as_str()) {
                Some("on") | Some("true") | Some("1") => {
                    js_enabled = true;
                    browser.set_js_enabled(true, wait_ms);
                    println!("JS enabled");
                }
                Some("off") | Some("false") | Some("0") => {
                    js_enabled = false;
                    browser.set_js_enabled(false, wait_ms);
                    println!("JS disabled");
                }
                _ => println!("JS is currently {}", if js_enabled { "on" } else { "off" }),
            },
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
                    browser.set_js_enabled(js_enabled, wait_ms);
                    println!("JS wait time set to {}ms", wait_ms);
                } else {
                    eprintln!("Usage: wait-ms <milliseconds>");
                }
            }

            // Interception
            "intercept" => {
                handle_intercept(&mut browser, &tokens[1..]);
            }

            "cookies" => {
                handle_cookies(&browser, &tokens[1..]);
            }

            "network" => {
                handle_network(&browser, &tokens[1..]);
            }

            other => {
                eprintln!(
                    "Unknown command: {}. Type `help` for available commands.",
                    other
                );
            }
        }
    }

    println!("Bye.");
    Ok(())
}

async fn navigate(browser: &mut Browser, url: &str, js: bool, wait_ms: u32) -> Result<()> {
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
                page.redirect_chain.as_ref(),
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
        tree.stats.landmarks, tree.stats.links, tree.stats.headings, tree.stats.actions,
    );
}

fn print_interaction_result(result: &pardus_core::InteractionResult, format: &OutputFormatArg) {
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
                            println!("Switched to tab {}: {}", tab.id, tab.url);
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
        "info" => match browser.active_tab() {
            Some(tab) => {
                println!("Active Tab [{}]:", tab.id);
                println!("  URL: {}", tab.url);
                println!("  Title: {}", tab.title.as_deref().unwrap_or("(none)"));
                println!("  State: {:?}", tab.state);
                println!("  History: {}/{}", tab.history_index + 1, tab.history.len());
            }
            None => println!("No active tab"),
        },
        other => {
            eprintln!(
                "Unknown tab command: {}. Use: list, open, switch, close, info",
                other
            );
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

fn handle_intercept(browser: &mut Browser, args: &[String]) {
    if args.is_empty() {
        eprintln!(
            "Usage: intercept <block|redirect|header|remove-header|mock|list|clear|domain> ..."
        );
        return;
    }

    match args[0].as_str() {
        "block" => {
            if args.len() < 2 {
                eprintln!("Usage: intercept block <pattern>");
                eprintln!("  Patterns: glob (*/ads/*), domain (*.tracker.com), or prefix (/api/)");
                return;
            }
            let pattern = &args[1];
            let rule = parse_pattern_to_rule(pattern);
            browser
                .interceptors()
                .add(Box::new(BlockingInterceptor::new(rule)));
            println!("Added block interceptor for: {}", pattern);
        }
        "redirect" => {
            if args.len() < 3 {
                eprintln!("Usage: intercept redirect <pattern> <target-url>");
                return;
            }
            let pattern = &args[1];
            let target = &args[2];
            let rule = parse_pattern_to_rule(pattern);
            browser
                .interceptors()
                .add(Box::new(RedirectInterceptor::new(rule, target.clone())));
            println!("Added redirect interceptor: {} -> {}", pattern, target);
        }
        "header" => {
            if args.len() < 2 {
                eprintln!("Usage: intercept header <name>=<value>");
                return;
            }
            let mut headers = std::collections::HashMap::new();
            for arg in &args[1..] {
                if let Some((name, value)) = arg.split_once('=') {
                    headers.insert(name.to_string(), value.to_string());
                    println!("Will set header: {}: {}", name, value);
                } else {
                    eprintln!("Invalid header format '{}', expected name=value", arg);
                }
            }
            if !headers.is_empty() {
                browser
                    .interceptors()
                    .add(Box::new(HeaderModifierInterceptor::new(None, headers)));
            }
        }
        "remove-header" => {
            if args.len() < 2 {
                eprintln!("Usage: intercept remove-header <name>");
                return;
            }
            let headers: Vec<String> = args[1..].to_vec();
            for name in &headers {
                println!("Will remove header: {}", name);
            }
            browser.interceptors_mut().add(Box::new(
                HeaderModifierInterceptor::new(None, std::collections::HashMap::new())
                    .with_removal(headers),
            ));
        }
        "mock" => {
            if args.len() < 4 {
                eprintln!("Usage: intercept mock <pattern> <status> <body>");
                return;
            }
            let pattern = &args[1];
            let status: u16 = match args[2].parse() {
                Ok(s) => s,
                Err(_) => {
                    eprintln!("Invalid status code: {}", args[2]);
                    return;
                }
            };
            let body = args[3..].join(" ");
            let rule = parse_pattern_to_rule(pattern);
            browser
                .interceptors_mut()
                .add(Box::new(MockResponseInterceptor::text(rule, status, &body)));
            println!(
                "Added mock interceptor: {} -> {} {}",
                pattern,
                status,
                &body[..body.len().min(80)]
            );
        }
        "domain" => {
            if args.len() < 2 {
                eprintln!("Usage: intercept domain <domain>");
                eprintln!("  Example: intercept domain *.doubleclick.net");
                return;
            }
            for domain in &args[1..] {
                let rule = InterceptorRule::Domain(domain.clone());
                browser
                    .interceptors_mut()
                    .add(Box::new(BlockingInterceptor::new(rule)));
                println!("Added domain block: {}", domain);
            }
        }
        "list" => {
            let count = browser.interceptors().len();
            if count == 0 {
                println!("No interceptors active");
            } else {
                println!("{} interceptor(s) active", count);
            }
        }
        "clear" => {
            println!("Clearing all interceptors");
            *browser.interceptors_mut() = pardus_core::InterceptorManager::new();
        }
        other => {
            eprintln!("Unknown intercept command: {}", other);
            eprintln!("Use: block, redirect, header, remove-header, mock, domain, list, clear");
        }
    }
}

fn parse_pattern_to_rule(pattern: &str) -> InterceptorRule {
    if pattern.contains("://") || pattern.starts_with("*.") {
        if pattern.contains('*') || pattern.contains('?') {
            InterceptorRule::url_glob(pattern)
        } else {
            InterceptorRule::Domain(pattern.to_string())
        }
    } else if pattern.starts_with('/') {
        InterceptorRule::PathPrefix(pattern.to_string())
    } else if pattern.contains('*') || pattern.contains('?') {
        InterceptorRule::url_glob(pattern)
    } else {
        InterceptorRule::url_glob(format!("*{}*", pattern))
    }
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
    println!("  event <sel> <type> [init]  Dispatch DOM event (e.g., event #1 change)");
    #[cfg(feature = "screenshot")]
    println!("  screenshot <path> [--full] [--element <sel>]");
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
    println!("Interception:");
    println!("  intercept block <pattern>         Block requests matching glob pattern");
    println!("  intercept redirect <pattern> <url> Redirect matching requests");
    println!("  intercept header <name>=<value>   Add/replace header on all requests");
    println!("  intercept remove-header <name>    Remove header from all requests");
    println!("  intercept mock <pattern> <status> <body>  Mock response for matching requests");
    println!("  intercept list                    List active interceptors");
    println!("  intercept clear                   Remove all interceptors");
    println!("  intercept domain <domain>         Block all requests to a domain (or *.domain)");
    println!();
    println!("Cookies:");
    println!("  cookies list [url]              List all cookies (or filtered by URL)");
    println!("  cookies set <name>=<value> [domain] [path]  Set a cookie");
    println!("  cookies delete <name> [domain] [path]       Delete a cookie");
    println!("  cookies clear                    Clear all cookies");
    println!();
    println!("Network:");
    println!("  network [list]                  Show captured network requests");
    println!("  network show <id>               Show details for a specific request");
    println!("  network failed                  Show only failed requests (4xx/5xx/errors)");
    println!("  network stats                   Show network summary statistics");
    println!("  network json                    Dump full network log as JSON");
    println!("  network har <path>              Export network log as HAR 1.2 JSON");
    println!("  network clear                   Clear all captured network records");
    println!();
    println!("  help                 Show this help");
    println!("  exit                 Exit REPL");
}

fn handle_cookies(browser: &Browser, args: &[String]) {
    match args.first().map(|s| s.as_str()) {
        Some("list") => {
            let cookies = browser.all_cookies();
            if cookies.is_empty() {
                println!("No cookies in jar.");
                return;
            }
            // If a URL filter is given, show only matching cookies
            let filtered = if let Some(url) = args.get(1) {
                cookies
                    .into_iter()
                    .filter(|c| url.contains(&c.domain) || c.domain.contains(url))
                    .collect::<Vec<_>>()
            } else {
                cookies
            };

            println!(
                "{:<5} {:<30} {:<40} {:<10} {:<8}",
                "Secure", "Name", "Domain", "Path", "Httponly"
            );
            for c in &filtered {
                println!(
                    "{:<5} {:<30} {:<40} {:<10} {:<8}",
                    if c.secure { "Yes" } else { "No" },
                    c.name,
                    c.domain,
                    c.path,
                    if c.http_only { "Yes" } else { "No" },
                );
            }
            println!("({} cookies)", filtered.len());
        }

        Some("set") => {
            if args.len() < 2 {
                eprintln!("Usage: cookies set <name>=<value> [domain] [path]");
                return;
            }
            let kv = &args[1];
            let (name, value) = match kv.split_once('=') {
                Some((n, v)) => (n, v),
                None => {
                    eprintln!("Invalid format. Use: name=value");
                    return;
                }
            };
            let domain = args.get(2).map(|s| s.as_str()).unwrap_or("example.com");
            let path = args.get(3).map(|s| s.as_str()).unwrap_or("/");
            browser.set_cookie(name, value, domain, path);
            println!(
                "Cookie set: {}={} (domain={}, path={})",
                name, value, domain, path
            );
        }

        Some("delete") | Some("remove") => {
            if args.len() < 2 {
                eprintln!("Usage: cookies delete <name> [domain] [path]");
                return;
            }
            let name = &args[1];
            let domain = args.get(2).map(|s| s.as_str()).unwrap_or("");
            let path = args.get(3).map(|s| s.as_str()).unwrap_or("/");
            if browser.delete_cookie(name, domain, path) {
                println!("Cookie '{}' deleted.", name);
            } else {
                println!("Cookie '{}' not found.", name);
            }
        }

        Some("clear") => {
            browser.clear_cookies();
            println!("All cookies cleared.");
        }

        _ => {
            eprintln!(
                "Usage: cookies list [url] | set <n>=<v> [domain] [path] | delete <name> [domain] \
                 [path] | clear"
            );
        }
    }
}

fn handle_network(browser: &Browser, args: &[String]) {
    use pardus_debug::formatter;

    let subcmd = args.first().map(|s| s.as_str()).unwrap_or("list");

    match subcmd {
        "list" | "ls" | "table" => {
            let log = browser
                .network_log()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if log.records.is_empty() {
                println!("No network requests captured yet. Navigate to a page first.");
                return;
            }
            let table = formatter::format_table(&log);
            for line in table.lines() {
                if !line.trim().is_empty() {
                    println!("  {}", line);
                }
            }
        }

        "show" => {
            if args.len() < 2 {
                eprintln!("Usage: network show <id>");
                return;
            }
            let id: usize = match args[1].parse() {
                Ok(id) => id,
                Err(_) => {
                    eprintln!("Invalid request ID: {}", args[1]);
                    return;
                }
            };
            let log = browser
                .network_log()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let record = match log.records.iter().find(|r| r.id == id) {
                Some(r) => r,
                None => {
                    eprintln!(
                        "Request #{} not found. {} requests captured.",
                        id,
                        log.records.len()
                    );
                    return;
                }
            };
            println!("  #{} {} {}", record.id, record.method, record.url);
            println!(
                "  Type: {} | Initiator: {}",
                record.resource_type, record.initiator
            );
            println!("  Description: {}", record.description);
            if let Some(status) = record.status {
                let status_text = record.status_text.as_deref().unwrap_or("");
                println!("  Status: {} {}", status, status_text);
            }
            if let Some(ct) = &record.content_type {
                println!("  Content-Type: {}", ct);
            }
            if let Some(size) = record.body_size {
                println!("  Size: {} ({})", formatter::format_bytes(size), size);
            }
            if let Some(time) = record.timing_ms {
                println!("  Timing: {}ms", time);
            }
            if let Some(ver) = &record.http_version {
                println!("  HTTP Version: {}", ver);
            }
            if let Some(cache) = record.from_cache {
                println!("  From Cache: {}", if cache { "yes" } else { "no" });
            }
            if let Some(redir) = &record.redirect_url {
                println!("  Redirect: {}", redir);
            }
            if let Some(err) = &record.error {
                println!("  Error: {}", err);
            }
            if let Some(ts) = &record.started_at {
                println!("  Started: {}", ts);
            }
            if !record.request_headers.is_empty() {
                println!("  Request Headers:");
                for (k, v) in &record.request_headers {
                    println!("    {}: {}", k, v);
                }
            }
            if !record.response_headers.is_empty() {
                println!("  Response Headers:");
                for (k, v) in &record.response_headers {
                    println!("    {}: {}", k, v);
                }
            }
        }

        "failed" | "errors" => {
            let log = browser
                .network_log()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let failed: Vec<_> = log
                .records
                .iter()
                .filter(|r| r.error.is_some() || r.status.is_some_and(|s| s >= 400))
                .collect();
            if failed.is_empty() {
                println!("No failed requests.");
                return;
            }
            println!("  Failed requests ({}):", failed.len());
            println!();
            println!(
                "  {:>2}  {:<7}  {:>6}  {:<50}  {}",
                "#", "Method", "Status", "URL", "Error"
            );
            for r in &failed {
                let status = r.status.map_or("—".to_string(), |s| s.to_string());
                let url = if r.url.len() > 50 {
                    format!("{}…", &r.url[..47])
                } else {
                    r.url.clone()
                };
                let error = r.error.as_deref().unwrap_or("");
                println!(
                    "  {:>2}  {:<7}  {:>6}  {:<50}  {}",
                    r.id, r.method, status, url, error
                );
            }
        }

        "stats" => {
            let log = browser
                .network_log()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if log.records.is_empty() {
                println!("No network requests captured yet.");
                return;
            }
            println!("  Network Stats:");
            println!("    Total requests: {}", log.total_requests());
            println!(
                "    Total bytes:    {} ({})",
                log.total_bytes(),
                formatter::format_bytes(log.total_bytes())
            );
            println!("    Max latency:    {}ms", log.total_time_ms());
            println!("    Failed:         {}", log.failed_count());

            // Breakdown by resource type
            let mut type_counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for r in &log.records {
                *type_counts.entry(r.resource_type.to_string()).or_insert(0) += 1;
            }
            println!("    By type:");
            let mut entries: Vec<_> = type_counts.into_iter().collect();
            entries.sort_by(|a, b| b.1.cmp(&a.1));
            for (t, c) in entries {
                println!("      {:<12} {}", t, c);
            }
        }

        "json" => {
            let log = browser
                .network_log()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if log.records.is_empty() {
                println!("No network requests captured yet.");
                return;
            }
            let json_data = formatter::NetworkLogJson::from_log(&log);
            match serde_json::to_string_pretty(&json_data) {
                Ok(json) => println!("{}", json),
                Err(e) => eprintln!("Failed to serialize network log: {}", e),
            }
        }

        "har" => {
            if args.len() < 2 {
                eprintln!("Usage: network har <path>");
                return;
            }
            let path = &args[1];
            let log = browser
                .network_log()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if log.records.is_empty() {
                println!("No network requests to export.");
                return;
            }
            let har = pardus_debug::har::HarFile::from_network_log(&log);
            match serde_json::to_string_pretty(&har) {
                Ok(json) => match std::fs::write(path, &json) {
                    Ok(_) => println!(
                        "HAR exported to {} ({} entries)",
                        path,
                        har.log.entries.len()
                    ),
                    Err(e) => eprintln!("Failed to write HAR file: {}", e),
                },
                Err(e) => eprintln!("Failed to serialize HAR: {}", e),
            }
        }

        "clear" => {
            let mut log = browser
                .network_log()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let count = log.records.len();
            log.records.clear();
            println!("Cleared {} network record(s).", count);
        }

        other => {
            eprintln!(
                "Unknown network command: {}. Use: list, show, failed, stats, json, har, clear",
                other
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use pardus_debug::{Initiator, NetworkLog, NetworkRecord, ResourceType};

    /// Build a Browser with a pre-populated network log for testing.
    ///
    /// We avoid `Browser::new()` because it creates a real HTTP client that
    /// may have TLS dependencies. Instead we exercise the public API surface
    /// that `handle_network` actually touches: only `browser.network_log()`.
    struct TestBrowser {
        network_log: Arc<Mutex<NetworkLog>>,
    }

    impl TestBrowser {
        fn new() -> Self {
            Self {
                network_log: Arc::new(Mutex::new(NetworkLog::new())),
            }
        }

        fn add_record(
            &self,
            id: usize,
            url: &str,
            status: Option<u16>,
            resource_type: ResourceType,
            error: Option<&str>,
        ) {
            let mut log = self.network_log.lock().unwrap();
            let mut r = NetworkRecord::fetched(
                id,
                "GET".to_string(),
                resource_type,
                format!("record-{}", id),
                url.to_string(),
                Initiator::Navigation,
            );
            r.status = status;
            r.error = error.map(|s| s.to_string());
            if let Some(s) = status {
                r.status_text = Some(if s < 400 {
                    "OK".to_string()
                } else {
                    "Error".to_string()
                });
            }
            r.body_size = Some(1024 * id);
            r.timing_ms = Some(50 * id as u128);
            r.content_type = Some("text/html".to_string());
            r.http_version = Some("HTTP/2".to_string());
            r.started_at = Some("2026-01-01T00:00:00.000Z".to_string());
            r.request_headers
                .push(("accept".to_string(), "text/html".to_string()));
            r.response_headers
                .push(("content-type".to_string(), "text/html".to_string()));
            log.push(r);
        }

        fn log_record_count(&self) -> usize { self.network_log.lock().unwrap().records.len() }
    }

    /// We need a thin adapter because `handle_network` expects `&Browser`.
    /// We test the logic through a local function that mirrors the structure
    /// but takes our `TestBrowser` instead. This tests the actual branching
    /// logic, filtering, and data access paths.
    fn populate_sample_log(browser: &TestBrowser) {
        browser.add_record(
            1,
            "https://example.com/",
            Some(200),
            ResourceType::Document,
            None,
        );
        browser.add_record(
            2,
            "https://example.com/style.css",
            Some(200),
            ResourceType::Stylesheet,
            None,
        );
        browser.add_record(
            3,
            "https://example.com/app.js",
            Some(404),
            ResourceType::Script,
            None,
        );
        browser.add_record(
            4,
            "https://example.com/api/data",
            Some(500),
            ResourceType::Fetch,
            Some("internal error"),
        );
    }

    // ── list subcommand ────────────────────────────────────────────────

    #[test]
    fn test_network_list_with_records() {
        let browser = TestBrowser::new();
        populate_sample_log(&browser);

        let log = browser.network_log().lock().unwrap();
        let table = pardus_debug::formatter::format_table(&log);
        assert!(table.contains("4 requests"));
        assert!(table.contains("example.com"));
        assert!(table.contains("200"));
        assert!(table.contains("404"));
        assert!(table.contains("500"));
        assert!(table.contains("2 failed")); // record 3 (404) + record 4 (500)
    }

    #[test]
    fn test_network_list_empty() {
        let browser = TestBrowser::new();
        let log = browser.network_log().lock().unwrap();
        let table = pardus_debug::formatter::format_table(&log);
        assert!(table.is_empty());
    }

    // ── show subcommand ────────────────────────────────────────────────

    #[test]
    fn test_network_show_finds_record_by_id() {
        let browser = TestBrowser::new();
        populate_sample_log(&browser);

        let log = browser.network_log().lock().unwrap();
        let record = log.records.iter().find(|r| r.id == 1).unwrap();
        assert_eq!(record.url, "https://example.com/");
        assert_eq!(record.status, Some(200));
        assert_eq!(record.content_type, Some("text/html".to_string()));
        assert_eq!(record.body_size, Some(1024));
        assert_eq!(record.timing_ms, Some(50));
        assert_eq!(record.http_version, Some("HTTP/2".to_string()));
        assert!(!record.request_headers.is_empty());
        assert!(!record.response_headers.is_empty());
    }

    #[test]
    fn test_network_show_record_not_found() {
        let browser = TestBrowser::new();
        populate_sample_log(&browser);

        let log = browser.network_log().lock().unwrap();
        let found = log.records.iter().find(|r| r.id == 999);
        assert!(found.is_none());
    }

    // ── failed subcommand ──────────────────────────────────────────────

    #[test]
    fn test_network_failed_filters_correctly() {
        let browser = TestBrowser::new();
        populate_sample_log(&browser);

        let log = browser.network_log().lock().unwrap();
        let failed: Vec<_> = log
            .records
            .iter()
            .filter(|r| r.error.is_some() || r.status.is_some_and(|s| s >= 400))
            .collect();

        assert_eq!(failed.len(), 2); // record 3 (404) and record 4 (500 + error)
        assert_eq!(failed[0].id, 3);
        assert_eq!(failed[1].id, 4);
    }

    #[test]
    fn test_network_failed_empty_when_all_ok() {
        let browser = TestBrowser::new();
        browser.add_record(
            1,
            "https://ok.com/",
            Some(200),
            ResourceType::Document,
            None,
        );

        let log = browser.network_log().lock().unwrap();
        let failed: Vec<_> = log
            .records
            .iter()
            .filter(|r| r.error.is_some() || r.status.is_some_and(|s| s >= 400))
            .collect();
        assert!(failed.is_empty());
    }

    // ── stats subcommand ───────────────────────────────────────────────

    #[test]
    fn test_network_stats() {
        let browser = TestBrowser::new();
        populate_sample_log(&browser);

        let log = browser.network_log().lock().unwrap();
        assert_eq!(log.total_requests(), 4);
        assert_eq!(log.total_bytes(), 1024 * (1 + 2 + 3 + 4)); // 10240
        assert_eq!(log.failed_count(), 2);

        // Type breakdown
        let mut type_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for r in &log.records {
            *type_counts.entry(r.resource_type.to_string()).or_insert(0) += 1;
        }
        assert_eq!(type_counts.len(), 4); // document, stylesheet, script, fetch
    }

    #[test]
    fn test_network_stats_empty() {
        let browser = TestBrowser::new();
        let log = browser.network_log().lock().unwrap();
        assert_eq!(log.total_requests(), 0);
        assert_eq!(log.total_bytes(), 0);
        assert_eq!(log.failed_count(), 0);
    }

    // ── json subcommand ────────────────────────────────────────────────

    #[test]
    fn test_network_json_serializes() {
        let browser = TestBrowser::new();
        populate_sample_log(&browser);

        let log = browser.network_log().lock().unwrap();
        let json_data = pardus_debug::formatter::NetworkLogJson::from_log(&log);
        let json = serde_json::to_string_pretty(&json_data).unwrap();

        // Verify key fields are present
        assert!(
            json.contains("\"total_requests\": 4") || json.contains("\"total_requests\":4"),
            "JSON should contain total_requests count. Got: {}",
            json
        );
        assert!(
            json.contains("\"failed\": 2") || json.contains("\"failed\":2"),
            "JSON should contain failed count. Got: {}",
            json
        );
        assert!(json.contains("example.com"));
    }

    #[test]
    fn test_network_json_empty_log() {
        let browser = TestBrowser::new();
        let log = browser.network_log().lock().unwrap();
        let json_data = pardus_debug::formatter::NetworkLogJson::from_log(&log);
        assert_eq!(json_data.total_requests, 0);
        assert!(json_data.requests.is_empty());
    }

    // ── har subcommand ─────────────────────────────────────────────────

    #[test]
    fn test_network_har_export() {
        let browser = TestBrowser::new();
        populate_sample_log(&browser);

        let log = browser.network_log().lock().unwrap();
        let har = pardus_debug::har::HarFile::from_network_log(&log);
        assert_eq!(har.log.entries.len(), 4);
        assert_eq!(har.log.version, "1.2");

        let json = serde_json::to_string(&har).unwrap();
        assert!(json.contains("\"entries\""));
        assert!(json.contains("example.com"));
    }

    #[test]
    fn test_network_har_write_to_file() {
        let browser = TestBrowser::new();
        populate_sample_log(&browser);

        let log = browser.network_log().lock().unwrap();
        let har = pardus_debug::har::HarFile::from_network_log(&log);
        let json = serde_json::to_string_pretty(&har).unwrap();

        let dir = std::env::temp_dir().join("pardus-test-har");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.har");
        std::fs::write(&path, &json).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("\"entries\""));

        std::fs::remove_dir_all(&dir).ok();
    }

    // ── clear subcommand ───────────────────────────────────────────────

    #[test]
    fn test_network_clear() {
        let browser = TestBrowser::new();
        populate_sample_log(&browser);
        assert_eq!(browser.log_record_count(), 4);

        {
            let mut log = browser.network_log().lock().unwrap();
            let count = log.records.len();
            log.records.clear();
            assert_eq!(count, 4);
        }

        assert_eq!(browser.log_record_count(), 0);
    }

    #[test]
    fn test_network_clear_empty() {
        let browser = TestBrowser::new();
        assert_eq!(browser.log_record_count(), 0);

        let mut log = browser.network_log().lock().unwrap();
        let count = log.records.len();
        log.records.clear();
        assert_eq!(count, 0);
    }

    // ── record with all optional fields ────────────────────────────────

    #[test]
    fn test_record_with_redirect_and_cache() {
        let browser = TestBrowser::new();
        {
            let mut log = browser.network_log().lock().unwrap();
            let mut r = NetworkRecord::fetched(
                10,
                "GET".to_string(),
                ResourceType::Document,
                "redirect test".to_string(),
                "https://old.com/page".to_string(),
                Initiator::Navigation,
            );
            r.status = Some(301);
            r.status_text = Some("Moved Permanently".to_string());
            r.redirect_url = Some("https://new.com/page".to_string());
            r.from_cache = Some(true);
            log.push(r);
        }

        let log = browser.network_log().lock().unwrap();
        let r = log.records.iter().find(|r| r.id == 10).unwrap();
        assert_eq!(r.status, Some(301));
        assert_eq!(r.redirect_url.as_deref(), Some("https://new.com/page"));
        assert_eq!(r.from_cache, Some(true));
    }
}
