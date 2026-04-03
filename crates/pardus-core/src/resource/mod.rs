//! High-performance resource loading with HTTP/2 prioritization
//!
//! Implements parallel subresource fetching with intelligent scheduling.

pub mod scheduler;
pub mod fetcher;
pub mod priority;
pub mod pool;

pub use scheduler::{ResourceScheduler, ResourceTask, ScheduleResult};
pub use fetcher::{ResourceFetcher, FetchOptions, FetchResult};
pub use priority::{PriorityQueue, PriorityTask};
pub use pool::{ConnectionPool, H2Connection, PoolConfig};

use std::sync::Arc;

/// Resource loading configuration
#[derive(Debug, Clone)]
pub struct ResourceConfig {
    /// Max concurrent connections per origin
    pub max_concurrent: usize,
    /// HTTP/2 stream limit
    pub h2_stream_limit: usize,
    /// Connection pool size
    pub pool_size: usize,
    /// Enable HTTP/2 prioritization
    pub h2_priority: bool,
    /// DNS cache duration
    pub dns_cache_secs: u64,
    /// TCP keepalive
    pub tcp_keepalive: bool,
}

impl Default for ResourceConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 6,
            h2_stream_limit: 100,
            pool_size: 32,
            h2_priority: true,
            dns_cache_secs: 300,
            tcp_keepalive: true,
        }
    }
}

/// Resource type categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ResourceKind {
    /// HTML document
    Document,
    /// Critical CSS
    Stylesheet,
    /// JavaScript
    Script,
    /// Images
    Image,
    /// Fonts
    Font,
    /// Media
    Media,
    /// Other resources
    Other,
}

/// Resource metadata
#[derive(Debug, Clone)]
pub struct Resource {
    pub url: String,
    pub kind: ResourceKind,
    pub priority: u8, // 0-255, lower = higher priority
    pub size_hint: Option<usize>,
}

use crate::cache::ResourceCache;

/// Resource manager that coordinates all resource operations
pub struct ResourceManager {
    scheduler: Arc<ResourceScheduler>,
    #[allow(dead_code)]
    config: ResourceConfig,
}

impl ResourceManager {
    pub fn new(client: rquest::Client, config: ResourceConfig, cache: Arc<ResourceCache>) -> Self {
        let scheduler = Arc::new(ResourceScheduler::new(client, config.clone(), cache));
        Self { scheduler, config }
    }

    /// Fetch multiple resources in parallel
    pub async fn fetch_batch(
        &self,
        resources: Vec<Resource>,
    ) -> Vec<FetchResult> {
        let scheduler = self.scheduler.clone();
        let tasks: Vec<_> = resources.into_iter()
            .map(|r| ResourceTask::from(r))
            .collect();

        scheduler.schedule_batch(tasks).await
    }
}
