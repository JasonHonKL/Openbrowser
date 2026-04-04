use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use futures::stream::{FuturesUnordered, StreamExt};
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};
use url::Url;

use pardus_core::app::App;
use pardus_core::config::BrowserConfig;
use pardus_core::navigation::graph::NavigationGraph;
use pardus_core::page::Page;
use pardus_core::page_analysis::PageAnalysis;

use crate::config::CrawlConfig;
use crate::discovery::{self, DiscoveredTransition};
use crate::fingerprint::{compute_fingerprint, discover_resources};
use crate::graph::KnowledgeGraph;
use crate::state::{ViewState, ViewStateId};
use crate::transition::{Transition, TransitionOutcome, Trigger};

struct FrontierEntry {
    url: String,
    depth: usize,
    parent_id: Option<ViewStateId>,
    trigger: Option<Trigger>,
    retries: u8,
}

struct ProcessedPage {
    entry: FrontierEntry,
    state_id: ViewStateId,
    discovered: Vec<DiscoveredTransition>,
}

pub async fn crawl(root_url: &str, config: &CrawlConfig) -> Result<KnowledgeGraph> {
    crawl_with_config(root_url, config).await
}

pub async fn crawl_with_config(root_url: &str, config: &CrawlConfig) -> Result<KnowledgeGraph> {
    let start = Instant::now();

    let mut browser_config = BrowserConfig::default();
    browser_config.proxy = config.proxy.clone();
    let app = Arc::new(App::new(browser_config));
    let mut graph = KnowledgeGraph::new(root_url, config.clone());

    let root_origin = Url::parse(root_url)
        .map(|u| u.origin().ascii_serialization())
        .unwrap_or_default();

    let mut frontier: VecDeque<FrontierEntry> = VecDeque::new();
    frontier.push_back(FrontierEntry {
        url: root_url.to_string(),
        depth: 0,
        parent_id: None,
        trigger: None,
        retries: 0,
    });

    let mut url_seen: HashSet<String> = HashSet::new();
    url_seen.insert(normalize_url(root_url));

    let semaphore = Arc::new(Semaphore::new(config.concurrency));
    let mut pages_crawled = 0usize;
    let mut max_depth_reached = 0usize;

    while !frontier.is_empty() {
        if pages_crawled >= config.max_pages {
            debug!("Max pages reached ({})", config.max_pages);
            break;
        }

        let batch_size = frontier.len().min(config.concurrency);
        let batch: Vec<FrontierEntry> = frontier.drain(..batch_size).collect();

        let mut in_flight = FuturesUnordered::new();
        let mut batch_reserved = 0usize;

        for entry in batch {
            if entry.depth > config.max_depth {
                continue;
            }
            if pages_crawled + batch_reserved >= config.max_pages {
                break;
            }

            let app = Arc::clone(&app);
            let sem = semaphore.clone();
            let delay = if pages_crawled + batch_reserved > 0 {
                Some(Duration::from_millis(config.delay_ms))
            } else {
                None
            };

            batch_reserved += 1;

            in_flight.push(async move {
                if let Some(dur) = delay {
                    tokio::time::sleep(dur).await;
                }
                let _permit = sem.acquire().await;
                let page = Page::from_url(&app, &entry.url).await;
                (entry, page)
            });
        }

        while let Some((entry, page_result)) = in_flight.next().await {
            let page = match page_result {
                Ok(p) => p,
                Err(e) => {
                    warn!(url = %entry.url, error = %e, "Failed to fetch page");
                    if entry.retries < 2 {
                        frontier.push_back(FrontierEntry {
                            retries: entry.retries + 1,
                            ..entry
                        });
                    }
                    continue;
                }
            };

            pages_crawled += 1;
            if entry.depth > max_depth_reached {
                max_depth_reached = entry.depth;
            }

            let processed = process_page(entry, &page, &mut graph, &root_origin, config);
            enqueue_transitions(processed, &mut frontier, &mut url_seen, &root_origin);
        }
    }

    let duration_ms = start.elapsed().as_millis();
    graph.compute_stats(max_depth_reached, pages_crawled, duration_ms);

    info!(
        states = graph.stats.total_states,
        transitions = graph.stats.total_transitions,
        verified = graph.stats.verified_transitions,
        duration_ms = duration_ms,
        "Crawl complete"
    );

    Ok(graph)
}

fn process_page(
    entry: FrontierEntry,
    page: &Page,
    graph: &mut KnowledgeGraph,
    root_origin: &str,
    config: &CrawlConfig,
) -> Option<ProcessedPage> {
    let analysis = PageAnalysis::build(&page.html, &page.url);
    let resource_urls = discover_resources(&page.html, &page.base_url);
    let (fingerprint, state_id) = compute_fingerprint(&page.url, &analysis.semantic_tree, &resource_urls);

    if let Some(ref parent_id) = entry.parent_id {
        if let Some(ref trigger) = entry.trigger {
            graph.add_transition(Transition {
                from: parent_id.clone(),
                to: state_id.clone(),
                trigger: trigger.clone(),
                verified: true,
                outcome: Some(TransitionOutcome {
                    status: page.status,
                    final_url: page.url.clone(),
                    matched_prediction: true,
                }),
            });
        }
    }

    if graph.has_state(&state_id) {
        debug!(id = %state_id.0, "State already known, skipping discovery");
        return None;
    }

    let (semantic_tree, navigation_graph) = if config.store_full_trees {
        (Some(analysis.semantic_tree), Some(analysis.navigation_graph.clone()))
    } else {
        (None, None)
    };

    let view_state = ViewState {
        id: state_id.clone(),
        url: page.url.clone(),
        fragment: fingerprint.fragment.clone(),
        fingerprint,
        semantic_tree,
        navigation_graph,
        resource_urls,
        title: page.title(),
        status: page.status,
    };

    info!(id = %state_id.0, url = %view_state.url, "New view-state discovered");
    graph.add_state(view_state);

    if entry.depth < config.max_depth {
        let discovered = discover_transitions_for_page(
            &analysis.navigation_graph,
            page,
            &state_id,
            root_origin,
            config,
        );
        Some(ProcessedPage {
            entry,
            state_id,
            discovered,
        })
    } else {
        None
    }
}

fn enqueue_transitions(
    processed: Option<ProcessedPage>,
    frontier: &mut VecDeque<FrontierEntry>,
    url_seen: &mut HashSet<String>,
    root_origin: &str,
) {
    let Some(processed) = processed else { return };

    for dt in processed.discovered {
        if !is_same_origin(&dt.target_url, root_origin) {
            continue;
        }
        let normalized = normalize_url(&dt.target_url);
        if url_seen.insert(normalized) {
            frontier.push_back(FrontierEntry {
                url: dt.target_url,
                depth: processed.entry.depth + 1,
                parent_id: Some(processed.state_id.clone()),
                trigger: Some(dt.trigger),
                retries: 0,
            });
        }
    }
}

fn discover_transitions_for_page(
    nav_graph: &NavigationGraph,
    page: &Page,
    state_id: &ViewStateId,
    root_origin: &str,
    config: &CrawlConfig,
) -> Vec<DiscoveredTransition> {
    let mut all = Vec::new();

    all.extend(discovery::discover_link_transitions(
        nav_graph, root_origin, state_id,
    ));

    if config.discover_hash_nav {
        let hash_transitions = discovery::discover_hash_transitions(&page.html, &page.url);
        all.extend(hash_transitions);
    }

    if config.discover_pagination {
        let pagination_transitions = discovery::discover_pagination_transitions(&page.url);
        all.extend(pagination_transitions);
    }

    if config.discover_forms {
        for form in &nav_graph.forms {
            let action_url = form.action.clone().unwrap_or_default();
            all.push(DiscoveredTransition {
                target_url: action_url,
                trigger: Trigger::FormSubmit {
                    form_id: form.id.clone(),
                    action: form.action.clone(),
                    method: form.method.clone(),
                    field_count: form.fields.len(),
                },
            });
        }
    }

    all
}

fn normalize_url(url: &str) -> String {
    let Ok(mut parsed) = Url::parse(url) else {
        return url.to_lowercase();
    };
    parsed.set_fragment(None);

    let mut pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    {
        let mut q = parsed.query_pairs_mut();
        q.clear();
        for (k, v) in &pairs {
            q.append_pair(k, v);
        }
    }

    let mut result = parsed.to_string();
    if result.ends_with('/') && !result.ends_with("://") {
        result.pop();
    }
    result
}

fn is_same_origin(url_str: &str, root_origin: &str) -> bool {
    url::Url::parse(url_str)
        .map(|u| u.origin().ascii_serialization() == root_origin)
        .unwrap_or(false)
}
