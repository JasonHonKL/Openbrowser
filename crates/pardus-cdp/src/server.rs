use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, Semaphore};

use crate::domain::browser::BrowserDomain;
use crate::domain::console::ConsoleDomain;
use crate::domain::css::CssDomain;
use crate::domain::dom::DomDomain;
use crate::domain::emulation::EmulationDomain;
use crate::domain::input::InputDomain;
use crate::domain::log::LogDomain;
use crate::domain::network::NetworkDomain;
use crate::domain::oauth::OAuthDomain;
use crate::domain::pardus_ext::PardusDomain;
use crate::domain::page::PageDomain;
use crate::domain::performance::PerformanceDomain;
use crate::domain::runtime::RuntimeDomain;
use crate::domain::security::SecurityDomain;
use crate::domain::target::TargetDomain;
use crate::domain::DomainContext;
use crate::protocol::event_bus::EventBus;
use crate::protocol::node_map::NodeMap;
use crate::protocol::registry::DomainRegistry;
use crate::protocol::router::CdpRouter;

const DEFAULT_MAX_CONNECTIONS: usize = 16;

/// CDP WebSocket server.
pub struct CdpServer {
    host: String,
    port: u16,
    timeout: u64,
    app: Arc<pardus_core::App>,
    max_connections: usize,
    shutdown: tokio::sync::watch::Sender<bool>,
}

impl CdpServer {
    pub fn new(host: String, port: u16, timeout: u64, app: Arc<pardus_core::App>) -> Self {
        let (shutdown_tx, _) = tokio::sync::watch::channel(false);
        Self {
            host,
            port,
            timeout,
            app,
            max_connections: DEFAULT_MAX_CONNECTIONS,
            shutdown: shutdown_tx,
        }
    }

    pub fn with_max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }

    pub fn host(&self) -> &str { &self.host }
    pub fn port(&self) -> u16 { self.port }

    pub fn shutdown(&self) {
        let _ = self.shutdown.send(true);
    }

    pub fn shutdown_rx(&self) -> tokio::sync::watch::Receiver<bool> {
        self.shutdown.subscribe()
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let addr = format!("{}:{}", self.host, self.port);
        let listener = TcpListener::bind(&addr).await?;
        tracing::info!("CDP server listening on ws://{}", addr);
        tracing::info!("Discovery: http://{}/json/version", addr);
        tracing::info!("Max connections: {}", self.max_connections);

        let event_bus = Arc::new(EventBus::new(1024));
        let oauth_sessions = Arc::new(Mutex::new(pardus_core::oauth::OAuthSessionManager::new()));
        let registry = build_registry();
        let router = Arc::new(CdpRouter::new(registry));
        let conn_semaphore = Arc::new(Semaphore::new(self.max_connections));
        let mut shutdown_rx = self.shutdown.subscribe();

        let local = tokio::task::LocalSet::new();

        let result: anyhow::Result<()> = local.run_until(async move {
            loop {
                tokio::select! {
                    accept_result = listener.accept() => {
                        let (stream, peer_addr) = match accept_result {
                            Ok(v) => v,
                            Err(e) => break Err(e.into()),
                        };

                        let permit = match conn_semaphore.clone().try_acquire_owned() {
                            Ok(permit) => permit,
                            Err(_) => {
                                tracing::warn!(
                                    "Connection from {} rejected: max connections ({}) reached",
                                    peer_addr, self.max_connections
                                );
                                continue;
                            }
                        };

                        tracing::info!("New connection from {}", peer_addr);

                        let router = router.clone();
                        let event_bus = event_bus.clone();
                        let oauth_sessions = oauth_sessions.clone();
                        let app = self.app.clone();
                        let timeout = self.timeout;

                        tokio::task::spawn_local(async move {
                            if let Err(e) = handle_connection(stream, peer_addr, router, event_bus, oauth_sessions, app, timeout).await {
                                tracing::error!("Connection error from {}: {}", peer_addr, e);
                            }
                            drop(permit);
                            tracing::info!("Connection from {} closed", peer_addr);
                        });
                    }
                    _ = shutdown_rx.changed() => {
                        tracing::info!("CDP server shutting down");
                        break Ok(());
                    }
                }
            }
        }).await;

        result
    }
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    peer_addr: std::net::SocketAddr,
    router: Arc<CdpRouter>,
    event_bus: Arc<EventBus>,
    oauth_sessions: Arc<Mutex<pardus_core::oauth::OAuthSessionManager>>,
    app: Arc<pardus_core::App>,
    timeout: u64,
) -> anyhow::Result<()> {
    let mut buf = [0u8; 4096];
    let n = stream.peek(&mut buf).await?;

    if !crate::transport::http::is_websocket_upgrade(&buf[..n]) {
        let path = crate::transport::http::parse_http_path(&buf[..n])
            .unwrap_or_else(|| "/".to_string());

        let response = match path.as_str() {
            "/json/version" => {
                let body = crate::transport::http::version_response(&peer_addr.ip().to_string(), 0);
                crate::transport::http::http_response(200, "application/json", &body)
            }
            "/json/list" | "/json" => {
                let body = crate::transport::http::list_response(&peer_addr.ip().to_string(), 0);
                crate::transport::http::http_response(200, "application/json", &body)
            }
            _ => {
                crate::transport::http::http_response(404, "text/plain", "Not Found")
            }
        };

        use tokio::io::AsyncWriteExt;
        let mut stream = stream;
        let _ = stream.write_all(&response).await;
        return Ok(());
    }

    let ws_result = tokio_tungstenite::accept_async(stream).await;

    match ws_result {
        Ok(ws_stream) => {
            let targets = Arc::new(Mutex::new(HashMap::<String, crate::domain::TargetEntry>::new()));
            let node_map = Arc::new(Mutex::new(NodeMap::new()));
            let ctx = Arc::new(DomainContext::new(
                app,
                targets,
                event_bus.clone(),
                node_map,
                oauth_sessions,
            ));
            crate::transport::ws::handle_websocket(
                ws_stream, router, ctx, event_bus, timeout,
            ).await;
        }
        Err(e) => {
            tracing::debug!("WebSocket upgrade failed from {}: {}", peer_addr, e);
        }
    }

    Ok(())
}

fn build_registry() -> DomainRegistry {
    let mut registry = DomainRegistry::new();
    registry.register(Box::new(BrowserDomain));
    registry.register(Box::new(TargetDomain));
    registry.register(Box::new(PageDomain));
    registry.register(Box::new(RuntimeDomain));
    registry.register(Box::new(DomDomain));
    registry.register(Box::new(NetworkDomain));
    registry.register(Box::new(EmulationDomain));
    registry.register(Box::new(InputDomain));
    registry.register(Box::new(CssDomain));
    registry.register(Box::new(LogDomain));
    registry.register(Box::new(ConsoleDomain));
    registry.register(Box::new(SecurityDomain));
    registry.register(Box::new(PerformanceDomain));
    registry.register(Box::new(PardusDomain));
    registry.register(Box::new(OAuthDomain));
    registry
}
