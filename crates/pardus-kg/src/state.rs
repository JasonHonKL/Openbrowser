use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

use pardus_core::NavigationGraph;
use pardus_core::SemanticTree;

/// Unique fingerprint identifying a distinct page state.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct ViewStateId(pub String);

/// Composite fingerprint components that produce a ViewStateId.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fingerprint {
    /// Normalized URL path (no query, no fragment).
    pub url_path: String,
    /// Query params that affect page content (pagination params).
    pub content_query_params: BTreeMap<String, String>,
    /// blake3 hash of the semantic tree's structural skeleton.
    pub tree_hash: String,
    /// blake3 hash of sorted subresource URL set.
    pub resource_set_hash: String,
    /// Hash fragment if present.
    pub fragment: Option<String>,
}

/// A snapshot of a single view-state within the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewState {
    pub id: ViewStateId,
    /// The URL that produced this state.
    pub url: String,
    /// Hash fragment, if present.
    pub fragment: Option<String>,
    /// Fingerprint components.
    pub fingerprint: Fingerprint,
    /// Semantic tree from pardus-core (only when `store_full_trees` is enabled).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_tree: Option<SemanticTree>,
    /// Navigation graph from pardus-core (only when `store_full_trees` is enabled).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub navigation_graph: Option<NavigationGraph>,
    /// The set of subresource URLs loaded by this state.
    pub resource_urls: HashSet<String>,
    /// Page title.
    pub title: Option<String>,
    /// HTTP status code.
    pub status: u16,
}
