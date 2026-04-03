//! Speculative prefetching for predictive loading
//!
//! Learns navigation patterns and pre-fetches likely next pages.

pub mod predictor;
pub mod prefetcher;

pub use predictor::{NavigationPredictor, NavigationModel, PageSequence};
pub use prefetcher::{Prefetcher, PrefetchConfig, PrefetchJob};

use crate::cache::ResourceCache;
use std::sync::Arc;

/// Prefetch manager that coordinates prediction and fetching
pub struct PrefetchManager {
    predictor: Arc<NavigationPredictor>,
    prefetcher: Arc<Prefetcher>,
    config: PrefetchConfig,
}

impl PrefetchManager {
    pub fn new(client: rquest::Client, config: PrefetchConfig, cache: Arc<ResourceCache>) -> Self {
        let predictor = Arc::new(NavigationPredictor::new());
        let prefetcher = Arc::new(Prefetcher::new(client, config.clone(), cache));

        Self {
            predictor,
            prefetcher,
            config,
        }
    }

    /// Record a navigation event
    pub fn record_navigation(
        &self,
        from: &str,
        to: &str,
    ) {
        self.predictor.record_transition(from, to);
    }

    /// Get predicted next URLs
    pub fn predict_next(
        &self,
        current_url: &str,
    ) -> Vec<String> {
        self.predictor.predict_next(current_url, self.config.max_predictions)
    }

    /// Start prefetching for current page
    pub async fn prefetch_for(&self,
        current_url: &str,
    ) {
        let predictions = self.predict_next(current_url);
        
        for url in predictions {
            let job = PrefetchJob {
                url: url.clone(),
                priority: 1, // Low priority for predictions
                source_url: current_url.to_string(),
            };
            
            self.prefetcher.queue(job).await;
        }
    }

    /// Get prefetch statistics
    pub fn stats(&self) -> PrefetchStats {
        PrefetchStats {
            predictions: self.predictor.stats(),
            prefetches: self.prefetcher.stats(),
        }
    }
}

/// Prefetch statistics
#[derive(Debug)]
pub struct PrefetchStats {
    pub predictions: predictor::PredictorStats,
    pub prefetches: prefetcher::PrefetcherStats,
}
