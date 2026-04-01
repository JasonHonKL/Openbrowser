use crate::navigation::graph::NavigationGraph;
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
}

// Implement Serialize for SemanticTree and SemanticNode
impl Serialize for SemanticTree {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("SemanticTree", 2)?;
        state.serialize_field("root", &self.root)?;
        state.serialize_field("stats", &self.stats)?;
        state.end()
    }
}

impl Serialize for crate::semantic::tree::SemanticNode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("SemanticNode", 8)?;
        state.serialize_field("role", &self.role.to_string())?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("tag", &self.tag)?;
        state.serialize_field("interactive", &self.is_interactive)?;
        state.serialize_field("href", &self.href)?;
        state.serialize_field("action", &self.action)?;
        state.serialize_field("children", &self.children)?;
        state.end()
    }
}

impl Serialize for crate::semantic::tree::TreeStats {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("TreeStats", 7)?;
        state.serialize_field("landmarks", &self.landmarks)?;
        state.serialize_field("links", &self.links)?;
        state.serialize_field("headings", &self.headings)?;
        state.serialize_field("actions", &self.actions)?;
        state.serialize_field("forms", &self.forms)?;
        state.serialize_field("images", &self.images)?;
        state.serialize_field("total_nodes", &self.total_nodes)?;
        state.end()
    }
}

/// Format the full result as JSON.
pub fn format_json(
    url: &str,
    title: Option<String>,
    tree: &SemanticTree,
    nav_graph: Option<&NavigationGraph>,
    network_log: Option<&pardus_debug::formatter::NetworkLogJson>,
) -> anyhow::Result<String> {
    let result = JsonResult {
        url: url.to_string(),
        title,
        semantic_tree: tree,
        stats: &tree.stats,
        navigation_graph: nav_graph,
        network_log,
    };
    Ok(serde_json::to_string_pretty(&result)?)
}
