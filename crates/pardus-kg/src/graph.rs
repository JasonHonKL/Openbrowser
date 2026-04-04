use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::config::CrawlConfig;
use crate::state::{ViewState, ViewStateId};
use crate::transition::Transition;

/// The complete knowledge graph of a site.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    /// Site root URL.
    pub root_url: String,
    /// When this graph was built (ISO 8601).
    pub built_at: String,
    /// Crawl configuration used.
    pub config: CrawlConfig,
    /// All view-states, keyed by ViewStateId.
    pub states: HashMap<ViewStateId, ViewState>,
    /// All transitions.
    pub transitions: Vec<Transition>,
    /// Summary statistics.
    pub stats: KgStats,
}

/// Summary statistics about the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgStats {
    pub total_states: usize,
    pub total_transitions: usize,
    pub verified_transitions: usize,
    pub max_depth_reached: usize,
    pub pages_crawled: usize,
    pub crawl_duration_ms: u128,
}

impl Default for KgStats {
    fn default() -> Self {
        Self {
            total_states: 0,
            total_transitions: 0,
            verified_transitions: 0,
            max_depth_reached: 0,
            pages_crawled: 0,
            crawl_duration_ms: 0,
        }
    }
}

impl KnowledgeGraph {
    pub fn new(root_url: &str, config: CrawlConfig) -> Self {
        Self {
            root_url: root_url.to_string(),
            built_at: chrono::Utc::now().to_rfc3339(),
            config,
            states: HashMap::new(),
            transitions: Vec::new(),
            stats: KgStats::default(),
        }
    }

    /// Add a view-state. Returns true if it was new.
    pub fn add_state(&mut self, state: ViewState) -> bool {
        let id = state.id.clone();
        self.states.insert(id, state).is_none()
    }

    /// Add a transition.
    pub fn add_transition(&mut self, transition: Transition) {
        self.transitions.push(transition);
    }

    /// Check if a ViewStateId is already known.
    pub fn has_state(&self, id: &ViewStateId) -> bool {
        self.states.contains_key(id)
    }

    /// Compute final stats.
    pub fn compute_stats(
        &mut self,
        max_depth_reached: usize,
        pages_crawled: usize,
        duration_ms: u128,
    ) {
        self.stats = KgStats {
            total_states: self.states.len(),
            total_transitions: self.transitions.len(),
            verified_transitions: self.transitions.iter().filter(|t| t.verified).count(),
            max_depth_reached,
            pages_crawled,
            crawl_duration_ms: duration_ms,
        };
    }
}
