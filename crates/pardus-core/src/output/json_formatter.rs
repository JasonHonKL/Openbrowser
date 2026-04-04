use crate::navigation::graph::NavigationGraph;
use crate::page::RedirectChain;
use crate::semantic::tree::SemanticTree;
use serde::Serialize;

#[derive(Serialize)]
pub struct JsonResult<'a> {
    pub url: String,
    pub title: Option<String>,
    pub semantic_tree: &'a SemanticTree,
    pub stats: &'a crate::semantic::tree::TreeStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub navigation_graph: Option<&'a NavigationGraph>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_log: Option<&'a pardus_debug::formatter::NetworkLogJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_chain: Option<&'a RedirectChain>,
}

/// Format the full result as JSON.
pub fn format_json(
    url: &str,
    title: Option<String>,
    tree: &SemanticTree,
    nav_graph: Option<&NavigationGraph>,
    network_log: Option<&pardus_debug::formatter::NetworkLogJson>,
    redirect_chain: Option<&RedirectChain>,
) -> anyhow::Result<String> {
    let result = JsonResult {
        url: url.to_string(),
        title,
        semantic_tree: tree,
        stats: &tree.stats,
        navigation_graph: nav_graph,
        network_log,
        redirect_chain,
    };
    Ok(serde_json::to_string_pretty(&result)?)
}
