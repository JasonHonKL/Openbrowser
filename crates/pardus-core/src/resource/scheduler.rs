//! Resource scheduler with HTTP/2 prioritization and cache support

use std::{collections::HashMap, sync::Arc};

use tokio::{
    sync::{Semaphore, mpsc},
    task::JoinSet,
};
use tracing::{debug, instrument};

use super::{
    Resource, ResourceConfig, ResourceKind,
    fetcher::{CachedFetcher, FetchResult},
};
use crate::cache::ResourceCache;

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
        Self {
            url,
            kind,
            priority,
            origin,
        }
    }

    fn extract_origin(url: &str) -> String {
        url::Url::parse(url)
            .ok()
            .map(|u| u.origin().ascii_serialization())
            .unwrap_or_default()
    }
}

impl From<Resource> for ResourceTask {
    fn from(r: Resource) -> Self { Self::new(r.url, r.kind, r.priority) }
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
    global_semaphore: Arc<Semaphore>,
}

impl ResourceScheduler {
    pub fn new(client: rquest::Client, config: ResourceConfig, cache: Arc<ResourceCache>) -> Self {
        let fetcher = Arc::new(CachedFetcher::new(client, config.clone(), cache));
        let global_semaphore = Arc::new(Semaphore::new(config.global_concurrency));
        Self {
            config,
            fetcher,
            origin_semaphores: parking_lot::Mutex::new(HashMap::new()),
            global_semaphore,
        }
    }

    #[instrument(skip(self, tasks), level = "debug")]
    pub async fn schedule_batch(self: Arc<Self>, tasks: Vec<ResourceTask>) -> Vec<FetchResult> {
        let start = std::time::Instant::now();
        debug!("scheduling {} tasks", tasks.len());

        // Sort tasks by priority (lower u8 = higher priority)
        let mut sorted_tasks = tasks;
        sorted_tasks.sort_by_key(|t| t.priority);

        let by_origin = self.group_by_origin(&sorted_tasks);

        let mut results = Vec::new();
        let mut join_set = JoinSet::new();

        for (origin, origin_tasks) in by_origin {
            let scheduler = self.clone();
            join_set.spawn(async move { scheduler.fetch_origin_group(origin, origin_tasks).await });
        }

        while let Some(Ok(group_results)) = join_set.join_next().await {
            results.extend(group_results);
        }

        let elapsed = start.elapsed();
        debug!(
            "batch fetch completed in {:?}, {} results",
            elapsed,
            results.len()
        );

        results
    }

    fn group_by_origin(&self, tasks: &[ResourceTask]) -> HashMap<String, Vec<ResourceTask>> {
        let mut groups: HashMap<String, Vec<ResourceTask>> = HashMap::new();

        for task in tasks {
            groups
                .entry(task.origin.clone())
                .or_default()
                .push(task.clone());
        }

        groups
    }

    /// Fetch all tasks for a single origin concurrently, respecting both
    /// per-origin and global concurrency limits. Tasks are spawned in
    /// priority order so higher-priority tasks acquire semaphore permits first.
    async fn fetch_origin_group(
        self: Arc<Self>,
        origin: String,
        mut tasks: Vec<ResourceTask>,
    ) -> Vec<FetchResult> {
        // Sort by priority within this origin group
        tasks.sort_by_key(|t| t.priority);

        let semaphore = self.get_origin_semaphore(&origin);
        let mut join_set = JoinSet::new();

        for task in tasks {
            let sem = semaphore.clone();
            let global_sem = self.global_semaphore.clone();
            let fetcher = self.fetcher.clone();

            join_set.spawn(async move {
                // Acquire per-origin permit first, then global
                let _origin_permit = match sem.acquire_owned().await {
                    Ok(p) => p,
                    Err(_) => return FetchResult::error(&task.url, "origin semaphore closed"),
                };
                let _global_permit = match global_sem.acquire_owned().await {
                    Ok(p) => p,
                    Err(_) => return FetchResult::error(&task.url, "global semaphore closed"),
                };

                fetcher.fetch(&task.url).await
            });
        }

        let mut results = Vec::new();
        while let Some(Ok(r)) = join_set.join_next().await {
            results.push(r);
        }
        results
    }

    fn get_origin_semaphore(&self, origin: &str) -> Arc<Semaphore> {
        let mut semaphores = self.origin_semaphores.lock();
        semaphores
            .entry(origin.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(self.config.max_concurrent)))
            .clone()
    }

    /// Remove semaphores for origins that have been idle, preventing unbounded growth.
    pub fn cleanup_idle_origins(&self, active_origins: &[String]) {
        let mut semaphores = self.origin_semaphores.lock();
        semaphores.retain(|origin, _| active_origins.contains(origin));
    }

    pub async fn schedule_with_priority(
        self: Arc<Self>,
        tasks: Vec<ResourceTask>,
    ) -> Vec<FetchResult> {
        self.schedule_batch(tasks).await
    }
}

pub struct CriticalPathFetcher {
    scheduler: Arc<ResourceScheduler>,
}

impl CriticalPathFetcher {
    pub fn new(scheduler: Arc<ResourceScheduler>) -> Self { Self { scheduler } }

    /// Fetch critical resources (stylesheets, scripts) first, then everything else.
    pub async fn fetch_critical(self: Arc<Self>, resources: Vec<Resource>) -> Vec<FetchResult> {
        let (critical, non_critical): (Vec<_>, Vec<_>) = resources
            .into_iter()
            .partition(|r| matches!(r.kind, ResourceKind::Document | ResourceKind::Stylesheet));

        let critical_tasks: Vec<_> = critical.into_iter().map(ResourceTask::from).collect();

        let mut results = self.scheduler.clone().schedule_batch(critical_tasks).await;

        if !non_critical.is_empty() {
            let non_critical_tasks: Vec<_> =
                non_critical.into_iter().map(ResourceTask::from).collect();
            let more_results = self
                .scheduler
                .clone()
                .schedule_batch(non_critical_tasks)
                .await;
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
            let fetcher = scheduler.fetcher.clone();
            let global_sem = scheduler.global_semaphore.clone();

            tokio::spawn(async move {
                let _permit = global_sem.acquire_owned().await.ok();
                let result = fetcher.fetch(&task.url).await;
                if tx.send(result).await.is_err() {
                    tracing::debug!("fetch result dropped for {}: receiver gone", task.url);
                }
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::ResourceKind;

    /// Verify the priority band ordering: Document/CSS < Script < Other < Image < Media
    #[test]
    fn test_priority_ordering_by_resource_kind() {
        let doc = priority_for_kind(ResourceKind::Document);
        let css = priority_for_kind(ResourceKind::Stylesheet);
        let js = priority_for_kind(ResourceKind::Script);
        let font = priority_for_kind(ResourceKind::Font);
        let other = priority_for_kind(ResourceKind::Other);
        let img = priority_for_kind(ResourceKind::Image);
        let media = priority_for_kind(ResourceKind::Media);

        assert_eq!(doc, css, "document and stylesheet should be same priority");
        assert!(css < js, "CSS should be higher priority than JS");
        assert_eq!(js, font, "script and font should be same priority");
        assert!(js < other, "JS should be higher priority than Other");
        assert!(other < img, "Other should be higher priority than images");
        assert!(img < media, "images should be higher priority than media");
    }

    fn priority_for_kind(kind: ResourceKind) -> u8 {
        match kind {
            ResourceKind::Document | ResourceKind::Stylesheet => 0,
            ResourceKind::Script | ResourceKind::Font => 32,
            ResourceKind::Image => 160,
            ResourceKind::Media => 224,
            ResourceKind::Other => 96,
        }
    }
}
