use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::{tungstenite, WebSocketStream};

use crate::domain::DomainContext;
use crate::protocol::event_bus::EventBus;
use crate::protocol::message::{CdpErrorResponse, CdpRequest};
use crate::protocol::router::CdpRouter;
use crate::protocol::target::CdpSession;

const DEFAULT_COMMAND_TIMEOUT_SECS: u64 = 30;

pub async fn handle_websocket(
    ws_stream: WebSocketStream<TcpStream>,
    router: Arc<CdpRouter>,
    ctx: Arc<DomainContext>,
    event_bus: Arc<EventBus>,
    timeout_secs: u64,
) {
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let mut event_rx = event_bus.subscribe();
    let session = Arc::new(Mutex::new(CdpSession::new(
        uuid::Uuid::new_v4().to_string(),
    )));

    let inactivity_timer = tokio::time::sleep(std::time::Duration::from_secs(timeout_secs));
    tokio::pin!(inactivity_timer);

    loop {
        tokio::select! {
            msg = ws_receiver.next() => {
                inactivity_timer.as_mut().reset(tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs));

                match msg {
                    Some(Ok(tungstenite::Message::Text(text))) => {
                        handle_text_message(&text, &router, &ctx, &session, &mut ws_sender).await;
                    }
                    Some(Ok(tungstenite::Message::Ping(data))) => {
                        let _ = ws_sender.send(tungstenite::Message::Pong(data)).await;
                    }
                    Some(Ok(tungstenite::Message::Close(_))) | None => {
                        tracing::info!("WebSocket connection closed");
                        break;
                    }
                    Some(Ok(tungstenite::Message::Binary(data))) => {
                        if let Ok(text) = String::from_utf8(data.to_vec()) {
                            handle_text_message(&text, &router, &ctx, &session, &mut ws_sender).await;
                        }
                    }
                    Some(Err(e)) => {
                        tracing::error!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            event = event_rx.recv() => {
                match event {
                    Ok(event) => {
                        let session = session.lock().await;
                        let domain = event.method.split('.').next().unwrap_or("");
                        if session.is_domain_enabled(domain) || domain == "Target" || domain == "Pardus" {
                            let json = serde_json::to_string(&event).unwrap_or_default();
                            drop(session);
                            let msg = tungstenite::Message::Text(json.into());
                            if ws_sender.send(msg).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Event bus lagged by {} messages", n);
                    }
                    Err(_) => break,
                }
            }
            _ = &mut inactivity_timer => {
                tracing::info!("WebSocket connection timed out after {}s of inactivity", timeout_secs);
                break;
            }
        }
    }
}

async fn handle_text_message(
    text: &str,
    router: &Arc<CdpRouter>,
    ctx: &Arc<DomainContext>,
    session: &Arc<Mutex<CdpSession>>,
    ws_sender: &mut futures_util::stream::SplitSink<
        WebSocketStream<TcpStream>,
        tungstenite::Message,
    >,
) {
    let request: CdpRequest = match serde_json::from_str(text) {
        Ok(req) => req,
        Err(e) => {
            let err = CdpErrorResponse {
                id: 0,
                error: crate::error::CdpErrorBody {
                    code: crate::error::PARSE_ERROR,
                    message: format!("Parse error: {}", e),
                },
                session_id: None,
            };
            let json = serde_json::to_string(&err).unwrap_or_default();
            let _ = ws_sender.send(tungstenite::Message::Text(json.into())).await;
            return;
        }
    };

    let request_id = request.id;

    let mut session_guard = session.lock().await;
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(DEFAULT_COMMAND_TIMEOUT_SECS),
        router.route(request, &mut session_guard, ctx)
    ).await;
    drop(session_guard);

    match result {
        Ok(Ok(response)) => {
            let json = serde_json::to_string(&response).unwrap_or_default();
            let _ = ws_sender.send(tungstenite::Message::Text(json.into())).await;
        }
        Ok(Err(error)) => {
            let json = serde_json::to_string(&error).unwrap_or_default();
            let _ = ws_sender.send(tungstenite::Message::Text(json.into())).await;
        }
        Err(_) => {
            let err = CdpErrorResponse {
                id: request_id,
                error: crate::error::CdpErrorBody {
                    code: crate::error::SERVER_ERROR,
                    message: format!("Command timed out after {}s", DEFAULT_COMMAND_TIMEOUT_SECS),
                },
                session_id: None,
            };
            let json = serde_json::to_string(&err).unwrap_or_default();
            let _ = ws_sender.send(tungstenite::Message::Text(json.into())).await;
        }
    }
}
