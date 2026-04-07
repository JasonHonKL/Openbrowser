use std::{path::PathBuf, sync::Arc, time::Instant};

use anyhow::Result;
use pardus_core::BrowserConfig;

use crate::OutputFormatArg;

pub async fn run_with_config(
    url: &str,
    format: OutputFormatArg,
    interactive_only: bool,
    with_nav: bool,
    js: bool,
    wait_ms: u32,
    network_log: bool,
    har_output: Option<PathBuf>,
    coverage_output: Option<PathBuf>,
    config: BrowserConfig,
) -> Result<()> {
    let start = Instant::now();

    println!("{:02}:{:02}  pardus-browser navigate {}", 0, 0, url);

    let mut browser = pardus_core::Browser::new(config)?;

    if js {
        println!("       JS execution enabled — executing scripts…");
        browser.navigate_with_js(url, wait_ms).await?;
    } else {
        browser.navigate(url).await?;
    }

    let elapsed_connected = start.elapsed().as_secs();
    let ms_connected = start.elapsed().as_millis() % 1000 / 10;
    println!(
        "{:02}:{:02}  connected — parsing semantic state…",
        elapsed_connected, ms_connected
    );

    // Clone references before borrowing page
    let net_log = browser.network_log().clone();
    let http_client = browser.http_client().clone();

    let page = browser
        .current_page()
        .ok_or_else(|| anyhow::anyhow!("no page loaded"))?;

    // Show redirect chain info
    if let Some(ref chain) = page.redirect_chain {
        if !chain.hops.is_empty() {
            let original = chain.original_url().unwrap_or(&page.url);
            println!(
                "       redirected {} -> {} ({} hop{})",
                original,
                page.url,
                chain.hops.len(),
                if chain.hops.len() == 1 { "" } else { "s" }
            );
            for hop in &chain.hops {
                println!("         {} {} -> {}", hop.status, hop.from, hop.to);
            }
        }
    }

    if network_log {
        page.discover_subresources(&net_log);
        pardus_core::Page::fetch_subresources(&http_client, &net_log).await;
    }

    // HAR export
    if let Some(ref har_path) = har_output {
        let log = net_log.lock().unwrap_or_else(|e| e.into_inner());
        let har = pardus_debug::har::HarFile::from_network_log(&log);
        let json = serde_json::to_string_pretty(&har)?;
        std::fs::write(har_path, json)?;
        println!("       HAR exported to {}", har_path.display());
    }

    // Coverage report
    if let Some(ref cov_path) = coverage_output {
        let css_sources = pardus_debug::coverage::extract_inline_styles(&page.html);
        let log = net_log.lock().unwrap_or_else(|e| e.into_inner());
        let report = pardus_debug::coverage::CoverageReport::build(
            &page.url,
            &page.html,
            &css_sources,
            &log,
        );
        let json = serde_json::to_string_pretty(&report)?;
        std::fs::write(cov_path, json)?;
        println!(
            "       Coverage report written to {} — {} CSS rules ({} matched, {} unmatched, {} \
             untestable)",
            cov_path.display(),
            report.summary.total_css_rules,
            report.summary.matched_css_rules,
            report.summary.unmatched_css_rules,
            report.summary.untestable_css_rules,
        );
    }

    let tree = page.semantic_tree();

    let tree = if interactive_only {
        Arc::new(filter_interactive(&tree))
    } else {
        tree
    };

    let elapsed_parsed = start.elapsed().as_secs();
    let ms_parsed = start.elapsed().as_millis() % 1000 / 10;

    match format {
        OutputFormatArg::Md => {
            let output = pardus_core::output::md_formatter::format_md(&tree);
            for line in output.lines() {
                if !line.trim().is_empty() {
                    println!("       {}", line);
                }
            }
        }
        OutputFormatArg::Tree => {
            let output = pardus_core::output::tree_formatter::format_tree(&tree);
            for line in output.lines() {
                if !line.trim().is_empty() {
                    println!("       {}", line);
                }
            }
        }
        OutputFormatArg::Llm => {
            let output = pardus_core::output::llm_formatter::format_llm(&tree);
            println!("{}", output);
            return Ok(());
        }
        OutputFormatArg::Json => {
            let nav_graph = if with_nav {
                Some(page.navigation_graph())
            } else {
                None
            };

            let network = if network_log {
                let log = net_log.lock().unwrap_or_else(|e| e.into_inner());
                Some(pardus_debug::formatter::NetworkLogJson::from_log(&log))
            } else {
                None
            };

            let json = pardus_core::output::json_formatter::format_json(
                &page.url,
                page.title(),
                &tree,
                nav_graph.as_ref(),
                network.as_ref(),
                page.redirect_chain.as_ref(),
            )?;
            println!("{}", json);
            return Ok(());
        }
    }

    println!(
        "{:02}:{:02}  semantic tree ready — {} landmarks, {} links, {} headings, {} actions",
        elapsed_parsed,
        ms_parsed,
        tree.stats.landmarks,
        tree.stats.links,
        tree.stats.headings,
        tree.stats.actions,
    );

    if network_log {
        let log = net_log.lock().unwrap();
        let table = pardus_debug::formatter::format_table_with_initiator(&log);
        for line in table.lines() {
            if !line.trim().is_empty() {
                println!("       {}", line);
            }
        }
    }

    if with_nav {
        let nav = page.navigation_graph();
        let elapsed_nav = start.elapsed().as_secs();
        let ms_nav = start.elapsed().as_millis() % 1000 / 10;

        if !nav.internal_links.is_empty() {
            println!(
                "{:02}:{:02}  navigation graph built — {} internal routes, {} external links",
                elapsed_nav,
                ms_nav,
                nav.internal_links.len(),
                nav.external_links.len(),
            );
        }
    }

    let elapsed_final = start.elapsed().as_secs();
    let ms_final = start.elapsed().as_millis() % 1000 / 10;
    println!(
        "{:02}:{:02}  agent-ready: structured state exposed · no pixel buffer · 0 screenshots",
        elapsed_final, ms_final,
    );

    Ok(())
}

fn filter_interactive(tree: &pardus_core::SemanticTree) -> pardus_core::SemanticTree {
    use pardus_core::{SemanticNode, SemanticRole, TreeStats};

    fn filter_node(node: &SemanticNode) -> Option<SemanticNode> {
        if node.is_interactive {
            let filtered_children: Vec<SemanticNode> =
                node.children.iter().filter_map(filter_node).collect();
            return Some(SemanticNode {
                children: filtered_children,
                ..node.clone()
            });
        }

        let filtered_children: Vec<SemanticNode> =
            node.children.iter().filter_map(filter_node).collect();

        if filtered_children.is_empty() {
            return None;
        }

        Some(SemanticNode {
            children: filtered_children,
            ..node.clone()
        })
    }

    let filtered_root = filter_node(&tree.root).unwrap_or_else(|| SemanticNode {
        role: SemanticRole::Document,
        name: None,
        tag: "document".to_string(),
        is_interactive: false,
        is_disabled: false,
        href: None,
        action: None,
        element_id: None,
        selector: None,
        input_type: None,
        placeholder: None,
        is_required: false,
        is_readonly: false,
        current_value: None,
        is_checked: false,
        options: Vec::new(),
        pattern: None,
        min_length: None,
        max_length: None,
        min_val: None,
        max_val: None,
        step_val: None,
        autocomplete: None,
        accept: None,
        multiple: false,
        children: vec![],
    });

    let mut stats = TreeStats::default();
    collect_stats(&filtered_root, &mut stats);
    stats.total_nodes = count_all_nodes(&filtered_root);

    pardus_core::SemanticTree {
        root: filtered_root,
        stats,
    }
}

fn collect_stats(node: &pardus_core::SemanticNode, stats: &mut pardus_core::TreeStats) {
    use pardus_core::SemanticRole;
    if node.role.is_landmark() {
        stats.landmarks += 1;
    }
    if matches!(node.role, SemanticRole::Link) {
        stats.links += 1;
    }
    if node.role.is_heading() {
        stats.headings += 1;
    }
    if matches!(node.role, SemanticRole::Form) {
        stats.forms += 1;
    }
    if matches!(node.role, SemanticRole::Image) {
        stats.images += 1;
    }
    if node.is_interactive {
        stats.actions += 1;
    }
    for child in &node.children {
        collect_stats(child, stats);
    }
}

fn count_all_nodes(node: &pardus_core::SemanticNode) -> usize {
    1 + node.children.iter().map(count_all_nodes).sum::<usize>()
}
