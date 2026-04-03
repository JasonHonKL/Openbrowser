//! Resource scheduler with HTTP/2 prioritization and cache support

use super::{Resource, ResourceConfig, ResourceKind};
use super::priority::PriorityQueue;
use super::fetcher::{CachedFetcher, FetchResult};

use crate::cache::ResourceCache;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;
use tracing::{instrument, debug};
use url::Url;

#[derive(Debug, Clone)]
pub struct ResourceTask {
    pub url: String,
    pub kind: ResourceKind,
    pub priority: u8,
    pub origin: String,
}

impl ResourceTask {
    pub fn new(url: String, kind: ResourceKind, priority: u8) -> Self {
        let origin = Self::extract_origin(&url);
        Self { url, kind, priority, origin }
    }

    fn extract_origin(url: &str) -> String {
        Url::parse(url)
            .ok()
            .map(|u| u.origin().ascii_serialization())
            .unwrap_or_default()
    }
}

impl From<Resource> for ResourceTask {
    fn from(r: Resource) -> Self {
        Self::new(r.url, r.kind, r.priority)
    }
}

#[derive(Debug)]
pub struct ScheduleResult {
    pub tasks: Vec<ResourceTask>,
    pub results: Vec<FetchResult>,
    pub duration_ms: u64,
}

pub struct ResourceScheduler {
    config: ResourceConfig,
    fetcher: Arc<CachedFetcher>,
    origin_semaphores: parking_lot::Mutex<HashMap<String, Arc<Semaphore>>>,
}

impl ResourceScheduler {
    pub fn new(client: rquest::Client, config: ResourceConfig, cache: Arc<ResourceCache>) -> Self {
        let fetcher = Arc::new(CachedFetcher::new(client, config.clone(), cache));
        Self {
            config,
            fetcher,
            origin_semaphores: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    #[instrument(skip(self, tasks), level = "debug")]
    pub async fn schedule_batch(
        self: Arc<Self>,
        tasks: Vec<ResourceTask>,
    ) -> Vec<FetchResult> {
        let start = std::time::Instant::now();
        debug!("scheduling {} tasks", tasks.len());

        let mut queue = PriorityQueue::new();
        for task in tasks {
            queue.push(task.priority, task);
        }

        let by_origin = self.group_by_origin(&queue.into_vec());

        let mut results = Vec::new();
        let mut join_set = JoinSet::new();

        for (origin, origin_tasks) in by_origin {
            let scheduler = self.clone();
            join_set.spawn(async move {
                scheduler.fetch_origin_group(origin, origin_tasks).await
            });
        }

        while let Some(Ok(group_results)) = join_set.join_next().await {
            results.extend(group_results);
        }

        let elapsed = start.elapsed();
        debug!("batch fetch completed in {:?}, {} results", elapsed, results.len());

        results
    }

    fn group_by_origin(
        &self,
        tasks: &[ResourceTask],
    ) -> HashMap<String, Vec<ResourceTask>> {
        let mut groups: HashMap<String, Vec<ResourceTask>> = HashMap::new();

        for task in tasks {
            groups.entry(task.origin.clone())
                .or_default()
                .push(task.clone());
        }

        groups
    }

    async fn fetch_origin_group(
        self: Arc<Self>,
        origin: String,
        tasks: Vec<ResourceTask>,
    ) -> Vec<FetchResult> {
        let semaphore = self.get_origin_semaphore(&origin);
        let mut results = Vec::new();

        for task in tasks {
            let permit = semaphore.clone().acquire_owned().await;
            if permit.is_err() {
                results.push(FetchResult::error(&task.url, "semaphore closed"));
                continue;
            }

            let result = self.fetcher.fetch(&task.url).await;
            results.push(result);

            drop(permit);
        }

        results
    }

    fn get_origin_semaphore(&self, origin: &str) -> Arc<Semaphore> {
        let mut semaphores = self.origin_semaphores.lock();
        semaphores.entry(origin.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(self.config.max_concurrent)))
            .clone()
    }

    /// Remove semaphores for origins that have been idle, preventing unbounded growth.
    pub fn cleanup_idle_origins(&self, active_origins: &[String]) {
        let mut semaphores = self.origin_semaphores.lock();
        semaphores.retain(|origin, _| active_origins.contains(origin));
    }

    pub async fn schedule_with_priority(
        &self,
        tasks: Vec<ResourceTask>,
        _priority_hints: HashMap<String, u8>,
    ) -> Vec<FetchResult> {
        let mut queue = PriorityQueue::new();
        for task in tasks {
            queue.push(task.priority, task);
        }

        let mut results = Vec::new();
        for (_, task) in queue.drain() {
            let result = self.fetcher.fetch(&task.url).await;
            results.push(result);
        }

        results
    }
}

pub struct CriticalPathFetcher {
    scheduler: Arc<ResourceScheduler>,
}

impl CriticalPathFetcher {
    pub fn new(scheduler: Arc<ResourceScheduler>) -> Self {
        Self { scheduler }
    }

    pub async fn fetch_critical(
        &self,
        resources: Vec<Resource>,
    ) -> Vec<FetchResult> {
        let (critical, non_critical): (Vec<_>, Vec<_>) = resources.into_iter()
            .partition(|r| matches!(r.kind, ResourceKind::Stylesheet | ResourceKind::Script));

        let critical_tasks: Vec<_> = critical.into_iter()
            .map(|r| ResourceTask::from(r))
            .collect();

        let mut results = self.scheduler.clone().schedule_batch(critical_tasks).await;

        if !non_critical.is_empty() {
            let non_critical_tasks: Vec<_> = non_critical.into_iter()
                .map(|r| ResourceTask::from(r))
                .collect();
            let more_results = self.scheduler.clone().schedule_batch(non_critical_tasks).await;
            results.extend(more_results);
        }

        results
    }
}

pub struct StreamingResourceFetcher {
    tx: mpsc::Sender<FetchResult>,
}

impl StreamingResourceFetcher {
    pub fn new() -> (Self, mpsc::Receiver<FetchResult>) {
        let (tx, rx) = mpsc::channel(100);
        (Self { tx }, rx)
    }

    pub async fn fetch_streaming(
        &self,
        scheduler: Arc<ResourceScheduler>,
        resources: Vec<ResourceTask>,
    ) -> anyhow::Result<()> {
        for task in resources {
            let tx = self.tx.clone();
            let sched = scheduler.clone();

            tokio::spawn(async move {
                let result = sched.fetcher.fetch(&task.url).await;
                if tx.send(result).await.is_err() {
                    tracing::debug!("fetch result dropped for {}: receiver gone", task.url);
                }
            });
        }

        Ok(())
    }
}
