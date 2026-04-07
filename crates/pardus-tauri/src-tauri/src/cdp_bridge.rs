use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::{RwLock, mpsc, oneshot};
use tokio_tungstenite::tungstenite;

const EVENT_BUFFER_SIZE: usize = 500;
const RECONNECT_BASE_MS: u64 = 1000;
const RECONNECT_MAX_MS: u64 = 30000;
const INIT_COMMAND_COUNT: u64 = 4;
type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpEventRecord {
    pub method: String,
    pub params: serde_json::Value,
    pub timestamp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeStatus {
    Connecting,
    Connected,
    Reconnecting,
    Disconnected,
    Failed,
}

struct InstanceBridge {
    #[allow(dead_code)]
    port: u16,
    ws_write: Option<mpsc::Sender<String>>,
    event_buffer: VecDeque<CdpEventRecord>,
    last_activity: std::time::Instant,
    status: BridgeStatus,
    cancel: tokio_util::sync::CancellationToken,
    pending_commands: HashMap<u64, oneshot::Sender<serde_json::Value>>,
    next_command_id: u64,
}

pub struct CdpBridge {
    instances: Arc<RwLock<HashMap<String, InstanceBridge>>>,
}

impl CdpBridge {
    pub fn new() -> Self {
        Self {
            instances: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn connect(&self, instance_id: String, port: u16, app_handle: tauri::AppHandle) {
        let mut instances = self.instances.write().await;
        if instances.contains_key(&instance_id) {
            return;
        }

        let bridge = InstanceBridge {
            port,
            ws_write: None,
            event_buffer: VecDeque::with_capacity(EVENT_BUFFER_SIZE),
            last_activity: std::time::Instant::now(),
            status: BridgeStatus::Connecting,
            cancel: tokio_util::sync::CancellationToken::new(),
            pending_commands: HashMap::new(),
            next_command_id: INIT_COMMAND_COUNT + 1,
        };

        let cancel = bridge.cancel.clone();
        instances.insert(instance_id.clone(), bridge);
        drop(instances);

        let instances_ref = self.instances.clone();
        let ws_url = format!("ws://127.0.0.1:{}", port);

        tokio::spawn(async move {
            run_bridge_loop(
                instances_ref,
                instance_id,
                port,
                &ws_url,
                app_handle,
                cancel,
            )
            .await;
        });
    }

    pub async fn disconnect(&self, instance_id: &str) {
        let mut instances = self.instances.write().await;
        if let Some(mut bridge) = instances.remove(instance_id) {
            bridge.cancel.cancel();
            for (_, tx) in bridge.pending_commands.drain() {
                let _ = tx.send(serde_json::json!({ "error": "bridge disconnected" }));
            }
        }
    }

    pub async fn send_command(
        &self,
        instance_id: &str,
        method: String,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let ws_write = {
            let instances = self.instances.read().await;
            let bridge = instances
                .get(instance_id)
                .ok_or_else(|| format!("no bridge for instance '{}'", instance_id))?;
            bridge
                .ws_write
                .as_ref()
                .ok_or_else(|| "bridge not connected".to_string())?
                .clone()
        };

        let cmd_id = {
            let mut instances = self.instances.write().await;
            let bridge = instances
                .get_mut(instance_id)
                .ok_or_else(|| format!("no bridge for instance '{}'", instance_id))?;
            let id = bridge.next_command_id;
            bridge.next_command_id += 1;
            id
        };

        let (tx, rx) = oneshot::channel();

        {
            let mut instances = self.instances.write().await;
            if let Some(bridge) = instances.get_mut(instance_id) {
                bridge.pending_commands.insert(cmd_id, tx);
            }
        }

        let msg = serde_json::json!({
            "id": cmd_id,
            "method": method,
            "params": params,
        });

        ws_write
            .send(msg.to_string())
            .await
            .map_err(|e| format!("failed to send command: {}", e))?;

        tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| "command timed out".to_string())?
            .map_err(|_| "command response channel dropped".to_string())
    }

    pub async fn get_events(
        &self,
        instance_id: &str,
        limit: usize,
        since: Option<i64>,
    ) -> Vec<CdpEventRecord> {
        let instances = self.instances.read().await;
        if let Some(bridge) = instances.get(instance_id) {
            let mut filtered: Vec<CdpEventRecord> = bridge
                .event_buffer
                .iter()
                .filter(|e| since.map_or(true, |t| e.timestamp > t))
                .cloned()
                .collect();
            let start = filtered.len().saturating_sub(limit);
            filtered.drain(..start);
            filtered
        } else {
            Vec::new()
        }
    }

    pub async fn get_status(&self, instance_id: &str) -> Option<BridgeStatus> {
        let instances = self.instances.read().await;
        instances.get(instance_id).map(|b| b.status.clone())
    }

    pub async fn touch_activity(&self, instance_id: &str) {
        let mut instances = self.instances.write().await;
        if let Some(bridge) = instances.get_mut(instance_id) {
            bridge.last_activity = std::time::Instant::now();
        }
    }
}

async fn run_bridge_loop(
    instances: Arc<RwLock<HashMap<String, InstanceBridge>>>,
    instance_id: String,
    port: u16,
    ws_url: &str,
    app_handle: tauri::AppHandle,
    cancel: tokio_util::sync::CancellationToken,
) {
    let mut reconnect_delay = RECONNECT_BASE_MS;

    loop {
        if cancel.is_cancelled() {
            break;
        }

        update_status(&instances, &instance_id, BridgeStatus::Connecting).await;

        match tokio_tungstenite::connect_async(ws_url).await {
            Ok((ws_stream, _)) => {
                reconnect_delay = RECONNECT_BASE_MS;
                update_status(&instances, &instance_id, BridgeStatus::Connected).await;

                let _ = app_handle.emit(
                    "cdp-bridge-connected",
                    serde_json::json!({
                        "instance_id": instance_id,
                        "port": port,
                    }),
                );

                if let Err(e) =
                    run_connected(&instances, &instance_id, ws_stream, &app_handle, &cancel).await
                {
                    tracing::warn!(
                        instance_id = %instance_id,
                        error = %e,
                        "CDP bridge connection lost"
                    );
                }

                clear_ws_write(&instances, &instance_id).await;

                if cancel.is_cancelled() {
                    break;
                }

                update_status(&instances, &instance_id, BridgeStatus::Reconnecting).await;

                let _ = app_handle.emit(
                    "cdp-bridge-disconnected",
                    serde_json::json!({
                        "instance_id": instance_id,
                        "port": port,
                    }),
                );
            }
            Err(e) => {
                tracing::warn!(
                    instance_id = %instance_id,
                    error = %e,
                    delay_ms = reconnect_delay,
                    "CDP bridge connection failed, retrying"
                );
            }
        }

        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(reconnect_delay)) => {},
            _ = cancel.cancelled() => break,
        }

        reconnect_delay = (reconnect_delay * 2).min(RECONNECT_MAX_MS);
    }

    update_status(&instances, &instance_id, BridgeStatus::Disconnected).await;
}

async fn run_connected(
    instances: &Arc<RwLock<HashMap<String, InstanceBridge>>>,
    instance_id: &str,
    ws_stream: WsStream,
    app_handle: &AppHandle,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<(), String> {
    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<String>(256);
    {
        let mut instances = instances.write().await;
        if let Some(bridge) = instances.get_mut(instance_id) {
            bridge.ws_write = Some(cmd_tx);
        }
    }

    let init_commands = [
        r#"{"id":1,"method":"Target.setDiscoverTargets","params":{"discover":true}}"#,
        r#"{"id":2,"method":"Page.enable","params":{}}"#,
        r#"{"id":3,"method":"Network.enable","params":{"maxTotalBufferSize":10000000,"maxResourceBufferSize":5000000}}"#,
        r#"{"id":4,"method":"Pardus.enable","params":{}}"#,
    ];

    for cmd in &init_commands {
        if cancel.is_cancelled() {
            return Ok(());
        }
        ws_sink
            .send(tungstenite::Message::Text((*cmd).into()))
            .await
            .map_err(|e| e.to_string())?;
    }

    loop {
        if cancel.is_cancelled() {
            return Ok(());
        }

        tokio::select! {
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(tungstenite::Message::Text(text))) => {
                        handle_cdp_message(&text, instances, instance_id, app_handle).await;
                    }
                    Some(Ok(tungstenite::Message::Ping(data))) => {
                        let _ = ws_sink.send(tungstenite::Message::Pong(data)).await;
                    }
                    Some(Ok(tungstenite::Message::Close(_))) | None => {
                        return Err("connection closed".to_string());
                    }
                    Some(Ok(tungstenite::Message::Binary(data))) => {
                        if let Ok(text) = String::from_utf8(data.to_vec()) {
                            handle_cdp_message(&text, instances, instance_id, app_handle).await;
                        }
                    }
                    Some(Err(e)) => {
                        return Err(format!("websocket error: {}", e));
                    }
                    _ => {}
                }
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(text) => {
                        ws_sink
                            .send(tungstenite::Message::Text(text.into()))
                            .await
                            .map_err(|e| format!("failed to send: {}", e))?;
                    }
                    None => {
                        return Err("command channel closed".to_string());
                    }
                }
            }
            _ = cancel.cancelled() => {
                return Ok(());
            }
        }
    }
}

async fn clear_ws_write(
    instances: &Arc<RwLock<HashMap<String, InstanceBridge>>>,
    instance_id: &str,
) {
    let mut instances = instances.write().await;
    if let Some(bridge) = instances.get_mut(instance_id) {
        bridge.ws_write = None;
    }
}

async fn handle_cdp_message(
    text: &str,
    instances: &Arc<RwLock<HashMap<String, InstanceBridge>>>,
    instance_id: &str,
    app_handle: &AppHandle,
) {
    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return,
    };

    if value.get("id").is_some() {
        let cmd_id = value["id"].as_u64().unwrap_or(0);
        if cmd_id <= INIT_COMMAND_COUNT {
            return;
        }
        let mut instances = instances.write().await;
        if let Some(bridge) = instances.get_mut(instance_id) {
            if let Some(tx) = bridge.pending_commands.remove(&cmd_id) {
                let _ = tx.send(value);
            }
        }
        return;
    }

    let method = match value["method"].as_str() {
        Some(m) => m.to_string(),
        None => return,
    };

    let timestamp = chrono::Utc::now().timestamp_millis();

    let record = CdpEventRecord {
        method: method.clone(),
        params: value
            .get("params")
            .cloned()
            .unwrap_or(serde_json::json!({})),
        timestamp,
    };

    {
        let mut instances = instances.write().await;
        if let Some(bridge) = instances.get_mut(instance_id) {
            bridge.last_activity = std::time::Instant::now();

            if bridge.event_buffer.len() >= EVENT_BUFFER_SIZE {
                bridge.event_buffer.pop_front();
            }
            bridge.event_buffer.push_back(record.clone());
        }
    }

    let _ = app_handle.emit(
        "cdp-event",
        serde_json::json!({
            "instance_id": instance_id,
            "method": method,
            "params": record.params,
            "timestamp": timestamp,
        }),
    );
}

async fn update_status(
    instances: &Arc<RwLock<HashMap<String, InstanceBridge>>>,
    instance_id: &str,
    status: BridgeStatus,
) {
    let mut instances = instances.write().await;
    if let Some(bridge) = instances.get_mut(instance_id) {
        bridge.status = status;
    }
}
