use scraper::Html;

use crate::navigation::graph::NavigationGraph;
use crate::semantic::tree::SemanticTree;

pub struct PageAnalysis {
    pub semantic_tree: SemanticTree,
    pub navigation_graph: NavigationGraph,
}

impl PageAnalysis {
    pub fn build(html: &Html, page_url: &str) -> Self {
        let semantic_tree = SemanticTree::build(html, page_url);
        let navigation_graph = NavigationGraph::build(html, page_url);
        Self {
            semantic_tree,
            navigation_graph,
        }
    }
}
