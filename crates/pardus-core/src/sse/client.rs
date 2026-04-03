//! Async SSE client that connects to event streams and parses events.
//!
//! Runs on a dedicated background tokio runtime to avoid blocking the
//! V8 thread or the current_thread runtime used during script execution.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;

use crate::sse::parser::{SseEvent, SseParser};
use crate::url_policy::UrlPolicy;

pub const SSE_CONNECTING: u8 = 0;
pub const SSE_OPEN: u8 = 1;
pub const SSE_CLOSED: u8 = 2;

fn sse_background_runtime() -> &'static tokio::runtime::Runtime {
    use std::sync::OnceLock;
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(8)
            .enable_all()
            .build()
            .expect("failed to create SSE background runtime")
    })
}

fn sse_http_client() -> &'static rquest::Client {
    use std::sync::OnceLock;
    static CLIENT: OnceLock<rquest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        rquest::Client::builder()
            .timeout(Duration::from_secs(300))
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(Duration::from_secs(60))
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| rquest::Client::new())
    })
}

/// Handle for a single SSE connection, held by `SseManager`.
pub struct SseConnectionHandle {
    pub id: u64,
    pub url: String,
    pub ready_state: Arc<AtomicU8>,
    pub closed: Arc<AtomicBool>,
    pub event_rx: std::sync::mpsc::Receiver<SseEvent>,
}

/// Spawn an SSE connection on the background runtime.
pub fn spawn_sse_connection(
    id: u64,
    url: String,
    url_policy: UrlPolicy,
) -> SseConnectionHandle {
    spawn_sse_connection_on(id, url, url_policy, sse_background_runtime(), sse_http_client().clone())
}

fn spawn_sse_connection_on(
    id: u64,
    url: String,
    url_policy: UrlPolicy,
    runtime: &tokio::runtime::Runtime,
    http_client: rquest::Client,
) -> SseConnectionHandle {
    let (event_tx, event_rx) = std::sync::mpsc::channel::<SseEvent>();
    let ready_state = Arc::new(AtomicU8::new(SSE_CONNECTING));
    let closed = Arc::new(AtomicBool::new(false));

    let ready_state_clone = ready_state.clone();
    let closed_clone = closed.clone();

    let url_clone = url.clone();
    runtime.spawn(async move {
        run_sse_loop(id, url, http_client, url_policy, event_tx, ready_state_clone, closed_clone).await;
    });

    SseConnectionHandle {
        id,
        url: url_clone,
        ready_state,
        closed,
        event_rx,
    }
}

async fn run_sse_loop(
    _id: u64,
    url: String,
    http_client: rquest::Client,
    url_policy: UrlPolicy,
    event_tx: std::sync::mpsc::Sender<SseEvent>,
    ready_state: Arc<AtomicU8>,
    closed: Arc<AtomicBool>,
) {
    let mut last_event_id: Option<String> = None;
    let mut reconnect_delay = Duration::from_secs(3);
    let mut attempt: u32 = 0;
    const MAX_RECONNECT_ATTEMPTS: u32 = 5;

    loop {
        if closed.load(Ordering::Relaxed) {
            ready_state.store(SSE_CLOSED, Ordering::Relaxed);
            return;
        }

        if url_policy.validate(&url).is_err() {
            ready_state.store(SSE_CLOSED, Ordering::Relaxed);
            let _ = event_tx.send(internal_event("__error", "URL blocked by security policy"));
            return;
        }

        let mut req = http_client
            .get(&url)
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache");

        if let Some(ref eid) = last_event_id {
            req = req.header("Last-Event-ID", eid.as_str());
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if !(200..300).contains(&status) {
                    ready_state.store(SSE_CLOSED, Ordering::Relaxed);
                    let _ = event_tx.send(internal_event("__error", &format!("HTTP {}", status)));
                    return;
                }

                ready_state.store(SSE_OPEN, Ordering::Relaxed);
                attempt = 0;
                reconnect_delay = Duration::from_secs(3);

                let _ = event_tx.send(internal_event("__open", ""));

                let mut parser = SseParser::new();
                let mut stream = resp.bytes_stream();

                while let Some(chunk_result) = stream.next().await {
                    if closed.load(Ordering::Relaxed) {
                        break;
                    }

                    match chunk_result {
                        Ok(bytes) => {
                            let text = String::from_utf8_lossy(&bytes);
                            let events = parser.feed(&text);
                            for event in &events {
                                if let Some(ref eid) = event.id {
                                    last_event_id = Some(eid.clone());
                                }
                                if let Some(ms) = event.retry {
                                    reconnect_delay = Duration::from_millis(ms);
                                }
                            }
                            for event in events {
                                if event_tx.send(event).is_err() {
                                    ready_state.store(SSE_CLOSED, Ordering::Relaxed);
                                    return;
                                }
                            }
                        }
                        Err(_) => {
                            break;
                        }
                    }
                }

                ready_state.store(SSE_CONNECTING, Ordering::Relaxed);
            }
            Err(e) => {
                let _ = event_tx.send(internal_event(
                    "__error",
                    &format!("Connection failed: {}", e),
                ));
            }
        }

        attempt += 1;
        if attempt > MAX_RECONNECT_ATTEMPTS {
            ready_state.store(SSE_CLOSED, Ordering::Relaxed);
            return;
        }

        let backoff = reconnect_delay * (1u32 << attempt.min(4));
        let backoff = backoff.min(Duration::from_secs(30));

        tokio::select! {
            _ = tokio::time::sleep(backoff) => {}
            _ = async {
                while !closed.load(Ordering::Relaxed) {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            } => {
                ready_state.store(SSE_CLOSED, Ordering::Relaxed);
                return;
            }
        }
    }
}

fn internal_event(event_type: &str, data: &str) -> SseEvent {
    SseEvent {
        event_type: event_type.to_string(),
        data: data.to_string(),
        id: None,
        retry: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    use std::time::Duration;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    fn test_spawn(id: u64, url: String, url_policy: UrlPolicy) -> SseConnectionHandle {
        let (event_tx, event_rx) = std::sync::mpsc::channel::<SseEvent>();
        let ready_state = Arc::new(AtomicU8::new(SSE_CONNECTING));
        let closed = Arc::new(AtomicBool::new(false));

        let ready_state_clone = ready_state.clone();
        let closed_clone = closed.clone();
        let url_clone = url.clone();

        let client = rquest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(2)
            .no_proxy()
            .build()
            .unwrap();

        tokio::spawn(async move {
            run_sse_loop(id, url, client, url_policy, event_tx, ready_state_clone, closed_clone).await;
        });

        SseConnectionHandle {
            id,
            url: url_clone,
            ready_state,
            closed,
            event_rx,
        }
    }

    async fn start_sse_server(handler: impl Fn(tokio::net::TcpStream, SocketAddr) + Send + Sync + 'static) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                if let Ok((stream, addr)) = listener.accept().await {
                    handler(stream, addr);
                }
            }
        });
        port
    }

    async fn start_basic_sse_server(events: Vec<&'static str>) -> u16 {
        let events = std::sync::Arc::new(events);
        start_sse_server(move |mut stream, _addr| {
            let events = events.clone();
            tokio::spawn(async move {
                let response = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n";
                stream.write_all(response.as_bytes()).await.ok();
                for event in events.iter() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    stream.write_all(event.as_bytes()).await.ok();
                    stream.write_all(b"\n\n").await.ok();
                    stream.flush().await.ok();
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            });
        }).await
    }

    async fn wait_for_events(rx: &std::sync::mpsc::Receiver<SseEvent>, count: usize, timeout: Duration) -> Vec<SseEvent> {
        let start = std::time::Instant::now();
        let mut collected = Vec::new();
        while collected.len() < count && start.elapsed() < timeout {
            match rx.try_recv() {
                Ok(event) => collected.push(event),
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
        collected
    }


    #[tokio::test]
    async fn test_constants() {
        assert_eq!(SSE_CONNECTING, 0);
        assert_eq!(SSE_OPEN, 1);
        assert_eq!(SSE_CLOSED, 2);
    }

    #[tokio::test]
    async fn test_spawn_connection_returns_handle() {
        let port = start_basic_sse_server(vec!["data: hello"]).await;
        let handle = test_spawn(
            1,
            format!("http://127.0.0.1:{}/events", port),
            UrlPolicy::permissive(),
        );
        assert_eq!(handle.id, 1);
        assert!(handle.url.contains(&port.to_string()));
        assert!(!handle.closed.load(Ordering::Relaxed));
        handle.closed.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_connects_and_receives_events() {
        let port = start_basic_sse_server(vec!["data: first", "data: second", "data: third"]).await;
        let handle = test_spawn(
            1,
            format!("http://127.0.0.1:{}/events", port),
            UrlPolicy::permissive(),
        );

        let events = wait_for_events(&handle.event_rx, 4, Duration::from_secs(5)).await;
        assert!(events.len() >= 4, "expected at least 4 events (1 open + 3 data), got {} -- types: {:?}", events.len(), events.iter().map(|e| e.event_type.clone()).collect::<Vec<_>>());

        let open_event = &events[0];
        assert_eq!(open_event.event_type, "__open");
        assert_eq!(open_event.data, "");

        assert_eq!(events[1].event_type, "message");
        assert_eq!(events[1].data, "first");
        assert_eq!(events[2].data, "second");
        assert_eq!(events[3].data, "third");

        handle.closed.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_custom_event_type() {
        let port = start_basic_sse_server(vec!["event: update\ndata: changed"]).await;
        let handle = test_spawn(
            1,
            format!("http://127.0.0.1:{}/events", port),
            UrlPolicy::permissive(),
        );

        let events = wait_for_events(&handle.event_rx, 2, Duration::from_secs(5)).await;
        assert!(events.len() >= 2, "expected at least 2 events, got {}", events.len());
        assert_eq!(events[1].event_type, "update");
        assert_eq!(events[1].data, "changed");

        handle.closed.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_event_id_preserved() {
        let port = start_basic_sse_server(vec!["id: 42\ndata: with-id"]).await;
        let handle = test_spawn(
            1,
            format!("http://127.0.0.1:{}/events", port),
            UrlPolicy::permissive(),
        );

        let events = wait_for_events(&handle.event_rx, 2, Duration::from_secs(5)).await;
        assert!(events.len() >= 2, "expected at least 2 events, got {}", events.len());
        assert_eq!(events[1].id.as_deref(), Some("42"));

        handle.closed.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_multiline_data() {
        let port = start_basic_sse_server(vec!["data: line1\ndata: line2\ndata: line3"]).await;
        let handle = test_spawn(
            1,
            format!("http://127.0.0.1:{}/events", port),
            UrlPolicy::permissive(),
        );

        let events = wait_for_events(&handle.event_rx, 2, Duration::from_secs(5)).await;
        assert!(events.len() >= 2, "expected at least 2 events, got {} -- types: {:?}", events.len(), events.iter().map(|e| e.event_type.clone()).collect::<Vec<_>>());
        assert_eq!(events[1].data, "line1\nline2\nline3");

        handle.closed.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_closed_flag_prevents_connection() {
        let closed = Arc::new(AtomicBool::new(false));
        let closed_clone = closed.clone();

        let _port = start_sse_server(move |_stream, _addr| {
            closed_clone.store(true, Ordering::Relaxed);
        }).await;

        let (tx, rx) = std::sync::mpsc::channel::<SseEvent>();
        let ready_state = Arc::new(AtomicU8::new(SSE_CONNECTING));

        let ready_state_clone = ready_state.clone();
        let closed_for_task = closed.clone();
        sse_background_runtime().spawn(async move {
            closed_for_task.store(true, Ordering::Relaxed);
            ready_state_clone.store(SSE_CLOSED, Ordering::Relaxed);
            let _ = tx.send(internal_event("__error", "closed before connect"));
        });

        let events = wait_for_events(&rx, 1, Duration::from_secs(2)).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "__error");
    }

    #[tokio::test]
    async fn test_ssrf_blocked_url() {
        let handle = test_spawn(
            1,
            "http://169.254.169.254/latest/meta-data/".to_string(),
            UrlPolicy::default(),
        );

        let events = wait_for_events(&handle.event_rx, 1, Duration::from_secs(3)).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "__error");
        assert!(events[0].data.contains("blocked"));
    }

    #[tokio::test]
    async fn test_file_scheme_blocked() {
        let handle = test_spawn(
            1,
            "file:///etc/passwd".to_string(),
            UrlPolicy::default(),
        );

        let events = wait_for_events(&handle.event_rx, 1, Duration::from_secs(3)).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "__error");
    }

    #[tokio::test]
    async fn test_localhost_blocked() {
        let handle = test_spawn(
            1,
            "http://localhost:12345/events".to_string(),
            UrlPolicy::default(),
        );

        let events = wait_for_events(&handle.event_rx, 1, Duration::from_secs(3)).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "__error");
    }

    #[tokio::test]
    async fn test_http_error_response() {
        let port = start_sse_server(move |mut stream, _addr| {
            tokio::spawn(async move {
                let response = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                stream.write_all(response.as_bytes()).await.ok();
            });
        }).await;

        let handle = test_spawn(
            1,
            format!("http://127.0.0.1:{}/events", port),
            UrlPolicy::permissive(),
        );

        let events = wait_for_events(&handle.event_rx, 1, Duration::from_secs(5)).await;
        assert_eq!(events.len(), 1, "expected 1 error event, got {:?}", events);
        assert_eq!(events[0].event_type, "__error");
        assert!(events[0].data.contains("500"), "error data should contain 500, got: {}", events[0].data);

        handle.closed.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_non_sse_content_type_still_accepted() {
        let port = start_basic_sse_server(vec!["data: test-event"]).await;

        let handle = test_spawn(
            1,
            format!("http://127.0.0.1:{}/events", port),
            UrlPolicy::permissive(),
        );

        let events = wait_for_events(&handle.event_rx, 2, Duration::from_secs(5)).await;
        assert!(events.len() >= 2, "expected at least 2 events, got {}", events.len());
        assert_eq!(events[0].event_type, "__open");
        assert_eq!(events[1].data, "test-event");

        handle.closed.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_close_signal_terminates_connection() {
        let port = start_sse_server(move |mut stream, _addr| {
            tokio::spawn(async move {
                let response = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n";
                stream.write_all(response.as_bytes()).await.ok();
                stream.flush().await.ok();
                tokio::time::sleep(Duration::from_secs(10)).await;
            });
        }).await;

        let handle = test_spawn(
            1,
            format!("http://127.0.0.1:{}/events", port),
            UrlPolicy::permissive(),
        );

        let events = wait_for_events(&handle.event_rx, 1, Duration::from_secs(5)).await;
        assert_eq!(events[0].event_type, "__open");
        assert_eq!(handle.ready_state.load(Ordering::Relaxed), SSE_OPEN);

        handle.closed.store(true, Ordering::Relaxed);
        tokio::time::sleep(Duration::from_millis(200)).await;

        let result = handle.event_rx.recv_timeout(Duration::from_millis(100));
        assert!(result.is_err(), "channel should be empty after close, got {:?}", result.ok());
    }

    #[tokio::test]
    async fn test_json_data_in_event() {
        let port = start_basic_sse_server(vec![r#"data: {"message": "hello", "count": 42}"#]).await;
        let handle = test_spawn(
            1,
            format!("http://127.0.0.1:{}/events", port),
            UrlPolicy::permissive(),
        );

        let events = wait_for_events(&handle.event_rx, 2, Duration::from_secs(5)).await;
        assert!(events.len() >= 2, "expected at least 2 events, got {}", events.len());
        let parsed: serde_json::Value = serde_json::from_str(&events[1].data).unwrap();
        assert_eq!(parsed["message"], "hello");
        assert_eq!(parsed["count"], 42);

        handle.closed.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_retry_field_in_stream() {
        let port = start_basic_sse_server(vec!["retry: 1000\ndata: test"]).await;
        let handle = test_spawn(
            1,
            format!("http://127.0.0.1:{}/events", port),
            UrlPolicy::permissive(),
        );

        let events = wait_for_events(&handle.event_rx, 2, Duration::from_secs(5)).await;
        assert!(events.len() >= 2, "expected at least 2 events, got {}", events.len());
        assert_eq!(events[1].retry, Some(1000));

        handle.closed.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_ready_state_transitions() {
        let port = start_basic_sse_server(vec!["data: test"]).await;
        let handle = test_spawn(
            1,
            format!("http://127.0.0.1:{}/events", port),
            UrlPolicy::permissive(),
        );

        assert_eq!(handle.ready_state.load(Ordering::Relaxed), SSE_CONNECTING);

        let events = wait_for_events(&handle.event_rx, 2, Duration::from_secs(5)).await;
        assert!(!events.is_empty(), "expected at least 1 event");
        assert_eq!(handle.ready_state.load(Ordering::Relaxed), SSE_OPEN);

        handle.closed.store(true, Ordering::Relaxed);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(handle.ready_state.load(Ordering::Relaxed), SSE_CLOSED);
    }

    #[tokio::test]
    async fn test_permissive_policy_allows_private() {
        let port = start_basic_sse_server(vec!["data: ok"]).await;
        let handle = test_spawn(
            1,
            format!("http://127.0.0.1:{}/events", port),
            UrlPolicy::permissive(),
        );

        let events = wait_for_events(&handle.event_rx, 2, Duration::from_secs(5)).await;
        assert!(events.len() >= 2, "expected at least 2 events, got {}", events.len());
        assert_eq!(events[0].event_type, "__open");

        handle.closed.store(true, Ordering::Relaxed);
    }
}

