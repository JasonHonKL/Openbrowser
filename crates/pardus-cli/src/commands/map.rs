use std::path::Path;

use anyhow::Result;

use pardus_core::ProxyConfig;
use pardus_kg::CrawlConfig;

pub async fn run_with_config(
    url: &str,
    output: &Path,
    depth: usize,
    max_pages: usize,
    delay: u64,
    skip_verify: bool,
    pagination: bool,
    hash_nav: bool,
    verbose: bool,
    proxy_config: ProxyConfig,
) -> Result<()> {
    if verbose {
        tracing_subscriber::fmt()
            .with_env_filter("pardus_kg=info,pardus_core=warn")
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter("pardus_kg=info")
            .init();
    }

    let config = CrawlConfig {
        max_depth: depth,
        max_pages,
        delay_ms: delay,
        discover_pagination: pagination,
        discover_hash_nav: hash_nav,
        discover_forms: false,
        store_full_trees: true,
        concurrency: 4,
        proxy: proxy_config,
    };

    tracing::info!(url = %url, depth, max_pages, "Starting site mapping");

    let kg = pardus_kg::crawl(url, &config).await?;

    let json = pardus_kg::output::serialize_kg(&kg)?;
    std::fs::write(output, &json)?;

    tracing::info!(
        output = %output.display(),
        states = kg.stats.total_states,
        transitions = kg.stats.total_transitions,
        duration_ms = kg.stats.crawl_duration_ms,
        "Knowledge graph written"
    );

    Ok(())
}
