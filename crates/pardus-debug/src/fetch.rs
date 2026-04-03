use crate::record::{NetworkLog, NetworkRecord};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// Result from checking a pre-fetch cache.
#[derive(Debug, Clone)]
pub struct CacheHit {
    pub status: u16,
    pub body_size: usize,
    pub content_type: Option<String>,
}

pub async fn fetch_subresources(
    client: &reqwest::Client,
    log: &Arc<std::sync::Mutex<NetworkLog>>,
    concurrency: usize,
) {
    fetch_subresources_with_cache(client, log, concurrency, None::<fn(&str) -> Option<CacheHit>>).await
}

/// Fetch subresources, optionally checking a pre-fetch cache first.
///
/// The `cache_check` closure is called for each unfetched resource URL.
/// If it returns `Some(CacheHit)`, the resource is marked as fetched in
/// the network log without making an HTTP request.
pub async fn fetch_subresources_with_cache<F>(
    client: &reqwest::Client,
    log: &Arc<std::sync::Mutex<NetworkLog>>,
    concurrency: usize,
    cache_check: Option<F>,
)
where
    F: Fn(&str) -> Option<CacheHit> + Send + Sync + 'static,
{
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut join_set = JoinSet::new();
    let cache_check: Arc<Option<F>> = Arc::new(cache_check);

    let entries: Vec<NetworkRecord> = {
        let log = log.lock().unwrap();
        log.records
            .iter()
            .filter(|r| r.status.is_none() && r.error.is_none())
            .cloned()
            .collect()
    };

    for record in entries {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let client = client.clone();
        let log = log.clone();
        let id = record.id;
        let url = record.url.clone();
        let cache_check = cache_check.clone();

        join_set.spawn(async move {
            let started_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            let start = Instant::now();

            if let Some(ref check) = *cache_check {
                if let Some(hit) = check(&url) {
                    let timing = Some(start.elapsed().as_millis());
                    let mut log = log.lock().unwrap();
                    if let Some(r) = log.records.iter_mut().find(|r| r.id == id) {
                        r.status = Some(hit.status);
                        r.status_text = Some("OK".to_string());
                        r.content_type = hit.content_type;
                        r.body_size = Some(hit.body_size);
                        r.timing_ms = timing;
                        r.response_headers = vec![("x-push-cache".to_string(), "hit".to_string())];
                        r.started_at = Some(started_at);
                    }
                    drop(permit);
                    return;
                }
            }

            let result = client.get(&url).send().await;

            let (status, status_text, content_type, body_size, headers, error, http_ver) = match result {
                Ok(resp) => {
                    let ver = format_http_version(resp.version());
                    let s = resp.status().as_u16();
                    let st = resp.status().canonical_reason().unwrap_or("").to_string();
                    let ct = resp
                        .headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    let hdrs: Vec<(String, String)> = resp
                        .headers()
                        .iter()
                        .filter_map(|(k, v)| Some((k.to_string(), v.to_str().ok()?.to_string())))
                        .collect();
                    let body = resp.bytes().await.unwrap_or_default();
                    (Some(s), Some(st), ct, Some(body.len()), hdrs, None, ver)
                }
                Err(e) => (None, None, None, None, Vec::new(), Some(e.to_string()), "unknown".to_string()),
            };

            let timing = Some(start.elapsed().as_millis());

            {
                let mut log = log.lock().unwrap();
                if let Some(r) = log.records.iter_mut().find(|r| r.id == id) {
                    r.status = status;
                    r.status_text = status_text;
                    r.content_type = content_type;
                    r.body_size = body_size;
                    r.timing_ms = timing;
                    r.response_headers = headers;
                    r.error = error;
                    r.started_at = Some(started_at);
                    r.http_version = Some(http_ver);
                }
            }

            drop(permit);
        });
    }

    while let Some(_result) = join_set.join_next().await {
        // Each task handles its own error recording
    }
}

fn format_http_version(version: http::Version) -> String {
    match version {
        http::Version::HTTP_09 => "HTTP/0.9",
        http::Version::HTTP_10 => "HTTP/1.0",
        http::Version::HTTP_11 => "HTTP/1.1",
        http::Version::HTTP_2 => "HTTP/2",
        http::Version::HTTP_3 => "HTTP/3",
        _ => "unknown",
    }.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{Initiator, ResourceType};

    fn make_client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap()
    }

    #[allow(dead_code)]
    fn make_log_with_unfetched() -> Arc<std::sync::Mutex<NetworkLog>> {
        let mut log = NetworkLog::new();
        log.push(NetworkRecord::discovered(1, ResourceType::Stylesheet, "a.css".into(), "https://example.com/a.css".into(), Initiator::Link));
        log.push(NetworkRecord::discovered(2, ResourceType::Script, "b.js".into(), "https://example.com/b.js".into(), Initiator::Script));
        Arc::new(std::sync::Mutex::new(log))
    }

    fn make_log_with_already_fetched() -> Arc<std::sync::Mutex<NetworkLog>> {
        let mut log = NetworkLog::new();
        let mut r = NetworkRecord::fetched(1, "GET".into(), ResourceType::Document, "nav".into(), "https://example.com".into(), Initiator::Navigation);
        r.status = Some(200);
        r.body_size = Some(4096);
        log.push(r);
        Arc::new(std::sync::Mutex::new(log))
    }

    fn make_log_with_mixed() -> Arc<std::sync::Mutex<NetworkLog>> {
        let mut log = NetworkLog::new();
        let mut r1 = NetworkRecord::fetched(1, "GET".into(), ResourceType::Document, "nav".into(), "https://example.com".into(), Initiator::Navigation);
        r1.status = Some(200);
        r1.body_size = Some(4096);
        log.push(r1);
        log.push(NetworkRecord::discovered(2, ResourceType::Stylesheet, "a.css".into(), "https://example.com/a.css".into(), Initiator::Link));
        Arc::new(std::sync::Mutex::new(log))
    }

    #[tokio::test]
    async fn test_fetch_no_unfetched() {
        let client = make_client();
        let log = make_log_with_already_fetched();
        fetch_subresources(&client, &log, 6).await;
        let log = log.lock().unwrap();
        assert_eq!(log.records[0].status, Some(200));
    }

    #[tokio::test]
    async fn test_fetch_empty_log() {
        let client = make_client();
        let log = Arc::new(std::sync::Mutex::new(NetworkLog::new()));
        fetch_subresources(&client, &log, 6).await;
        let log = log.lock().unwrap();
        assert!(log.records.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_records_error_on_invalid_url() {
        let client = make_client();
        let mut log = NetworkLog::new();
        log.push(NetworkRecord::discovered(1, ResourceType::Script, "bad".into(), "http://0.0.0.0:1/impossible.js".into(), Initiator::Script));
        let log = Arc::new(std::sync::Mutex::new(log));
        fetch_subresources(&client, &log, 6).await;
        let log = log.lock().unwrap();
        assert!(log.records[0].error.is_some(), "Expected error for unreachable URL");
        assert!(log.records[0].status.is_none());
        assert!(log.records[0].timing_ms.is_some());
    }

    #[tokio::test]
    async fn test_fetch_only_unfetched_in_mixed_log() {
        let client = make_client();
        let log = make_log_with_mixed();
        fetch_subresources(&client, &log, 6).await;
        let log = log.lock().unwrap();
        assert_eq!(log.records[0].status, Some(200));
        assert!(log.records[0].body_size.is_some());
        let r2 = &log.records[1];
        assert!(r2.status.is_some() || r2.error.is_some(), "Unfetched record should now be populated");
    }

    #[tokio::test]
    async fn test_fetch_concurrency_limit() {
        let client = make_client();
        let mut log = NetworkLog::new();
        for i in 1..=20 {
            log.push(NetworkRecord::discovered(
                i,
                ResourceType::Image,
                format!("img{}.png", i),
                format!("https://example.com/img{}.png", i),
                Initiator::Img,
            ));
        }
        let log = Arc::new(std::sync::Mutex::new(log));
        fetch_subresources(&client, &log, 4).await;
        let log = log.lock().unwrap();
        assert_eq!(log.records.len(), 20);
        for r in &log.records {
            assert!(r.timing_ms.is_some() || r.error.is_some());
        }
    }

    #[tokio::test]
    async fn test_fetch_populates_fields() {
        let client = make_client();
        let mut log = NetworkLog::new();
        log.push(NetworkRecord::discovered(1, ResourceType::Document, "test".into(), "https://example.com".into(), Initiator::Navigation));
        let log = Arc::new(std::sync::Mutex::new(log));
        fetch_subresources(&client, &log, 6).await;
        let log = log.lock().unwrap();
        let r = &log.records[0];
        assert!(r.status.is_some());
        assert!(r.status_text.is_some());
        assert!(r.content_type.is_some());
        assert!(r.body_size.is_some());
        assert!(r.timing_ms.is_some());
        assert!(!r.response_headers.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_with_cache_hit() {
        let client = make_client();
        let mut log = NetworkLog::new();
        log.push(NetworkRecord::discovered(1, ResourceType::Stylesheet, "cached.css".into(), "https://example.com/cached.css".into(), Initiator::Link));
        let log = Arc::new(std::sync::Mutex::new(log));

        let cache_check = |url: &str| -> Option<CacheHit> {
            if url == "https://example.com/cached.css" {
                Some(CacheHit {
                    status: 200,
                    body_size: 1024,
                    content_type: Some("text/css".to_string()),
                })
            } else {
                None
            }
        };

        fetch_subresources_with_cache(&client, &log, 6, Some(cache_check)).await;
        let log = log.lock().unwrap();
        let r = &log.records[0];
        assert_eq!(r.status, Some(200));
        assert_eq!(r.body_size, Some(1024));
        assert_eq!(r.content_type, Some("text/css".to_string()));
        assert!(r.timing_ms.is_some());
        assert!(!r.response_headers.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_with_cache_miss() {
        let client = make_client();
        let mut log = NetworkLog::new();
        log.push(NetworkRecord::discovered(1, ResourceType::Document, "test".into(), "https://example.com".into(), Initiator::Navigation));
        let log = Arc::new(std::sync::Mutex::new(log));

        let cache_check = |_: &str| -> Option<CacheHit> { None };

        fetch_subresources_with_cache(&client, &log, 6, Some(cache_check)).await;
        let log = log.lock().unwrap();
        let r = &log.records[0];
        assert!(r.status.is_some() || r.error.is_some());
    }
}
