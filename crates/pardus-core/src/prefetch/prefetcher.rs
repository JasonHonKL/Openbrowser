//! Prefetch worker for background loading with cache integration

use crate::cache::ResourceCache;
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Semaphore};
use tokio::time::{Duration, Instant};
use tracing::trace;

#[derive(Debug, Clone)]
pub struct PrefetchConfig {
    pub max_concurrent: usize,
    pub max_predictions: usize,
    pub min_confidence: f64,
    pub cooldown_ms: u64,
    pub enabled: bool,
}

impl Default for PrefetchConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 2,
            max_predictions: 3,
            min_confidence: 0.3,
            cooldown_ms: 100,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PrefetchJob {
    pub url: String,
    pub priority: u8,
    pub source_url: String,
}

#[derive(Debug, Clone)]
pub struct PrefetchResult {
    pub url: String,
    pub success: bool,
    pub data: Option<Bytes>,
    pub duration_ms: u64,
}

pub struct Prefetcher {
    config: PrefetchConfig,
    queue: mpsc::Sender<PrefetchJob>,
    stats: Arc<parking_lot::Mutex<PrefetcherStats>>,
    #[allow(dead_code)]
    semaphore: Arc<Semaphore>,
}

impl Prefetcher {
    pub fn new(client: rquest::Client, config: PrefetchConfig, cache: Arc<ResourceCache>) -> Self {
        let (tx, mut rx) = mpsc::channel::<PrefetchJob>(100);
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent));

        let stats = Arc::new(parking_lot::Mutex::new(PrefetcherStats::default()));

        let worker_stats = stats.clone();
        let cooldown_ms = config.cooldown_ms;
        tokio::spawn(async move {
            while let Some(job) = rx.recv().await {
                let start = Instant::now();

                if let Some(entry) = cache.get(&job.url) {
                    let guard = entry.read().unwrap();
                    if guard.is_fresh() {
                        trace!("prefetch cache hit: {}", job.url);
                        let mut s = worker_stats.lock();
                        s.successful += 1;
                        s.cache_hits += 1;
                        continue;
                    }
                }

                match client.get(&job.url).send().await {
                    Ok(response) => {
                        let status = response.status().as_u16();
                        if (200..300).contains(&status) {
                            if let Ok(bytes) = response.bytes().await {
                                let duration = start.elapsed();
                                trace!("prefetched {} in {:?}", job.url, duration);

                                let content_type = None;
                                cache.insert(&job.url, bytes.clone(), content_type, &rquest::header::HeaderMap::new());

                                let mut s = worker_stats.lock();
                                s.successful += 1;
                                s.total_bytes += bytes.len();
                            }
                        }
                    }
                    Err(e) => {
                        trace!("prefetch failed for {}: {}", job.url, e);
                        let mut s = worker_stats.lock();
                        s.failed += 1;
                    }
                }

                tokio::time::sleep(Duration::from_millis(cooldown_ms)).await;
            }
        });

        Self {
            config,
            queue: tx,
            stats,
            semaphore,
        }
    }

    pub async fn queue(&self, job: PrefetchJob) {
        if !self.config.enabled {
            return;
        }
        let _ = self.queue.send(job).await;
    }

    pub fn stats(&self) -> PrefetcherStats {
        *self.stats.lock()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PrefetcherStats {
    pub queued: usize,
    pub successful: usize,
    pub failed: usize,
    pub total_bytes: usize,
    pub cache_hits: usize,
}

pub struct AdaptivePrefetcher {
    #[allow(dead_code)]
    base: Prefetcher,
    success_rates: parking_lot::RwLock<HashMap<String, f64>>,
}

impl AdaptivePrefetcher {
    pub fn new(client: rquest::Client, config: PrefetchConfig, cache: Arc<ResourceCache>) -> Self {
        let base = Prefetcher::new(client, config, cache);

        Self {
            base,
            success_rates: parking_lot::RwLock::new(HashMap::new()),
        }
    }

    pub fn record_success(&self, pattern: &str, success: bool) {
        let mut rates = self.success_rates.write();
        let entry = rates.entry(pattern.to_string()).or_insert(0.5);

        let alpha = 0.1;
        let new_rate = if success {
            *entry * (1.0 - alpha) + alpha
        } else {
            *entry * (1.0 - alpha)
        };

        *entry = new_rate;
    }

    pub fn should_prefetch(&self, url: &str) -> bool {
        let rates = self.success_rates.read();

        for (pattern, rate) in rates.iter() {
            if url.ends_with(pattern) && *rate > 0.5 {
                return true;
            }
        }

        rates.is_empty()
    }
}
