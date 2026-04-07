use std::{
    collections::HashMap,
    io::{BufRead, Write},
    process::{Child, Command, Stdio},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::sync::{Mutex, oneshot};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
    pub temperature: f64,
    pub max_tokens: u32,
    pub max_rounds: u32,
}

struct PendingRequest {
    resolve: oneshot::Sender<serde_json::Value>,
}

struct AgentProcess {
    #[allow(dead_code)]
    child: Child,
    stdin: std::sync::Mutex<std::process::ChildStdin>,
    next_id: std::sync::atomic::AtomicU64,
    pending: Mutex<HashMap<u64, PendingRequest>>,
    status: Mutex<String>,
}

pub struct AgentBridge {
    processes: Arc<Mutex<HashMap<String, AgentProcess>>>,
}

impl AgentBridge {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn spawn(
        &self,
        instance_id: &str,
        config: AgentConfig,
        app_handle: tauri::AppHandle,
    ) -> Result<(), String> {
        {
            let processes = self.processes.lock().await;
            if processes.contains_key(instance_id) {
                return Err(format!("agent already running for {}", instance_id));
            }
        }

        let node_bin = Self::find_node_binary()?;
        let sidecar_script = Self::find_sidecar_script()?;

        let mut child = Command::new(&node_bin)
            .arg(&sidecar_script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to spawn agent sidecar: {}", e))?;

        let stdin = child
            .stdin
            .take()
            .ok_or("failed to open stdin for sidecar")?;

        let stdout = child
            .stdout
            .take()
            .ok_or("failed to open stdout for sidecar")?;

        let iid = instance_id.to_string();
        let app = app_handle.clone();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<serde_json::Value>(256);

        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        let trimmed = l.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        if let Ok(msg) = serde_json::from_str::<serde_json::Value>(trimmed) {
                            if tx.blocking_send(msg).is_err() {
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let reader_iid = iid.clone();
        let reader_bridge = self.processes.clone();
        let reader_app = app.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(msg) = rx.recv().await {
                let method = msg.get("method").and_then(|m| m.as_str());
                let params = msg
                    .get("params")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let id_num = msg.get("id").and_then(|i| i.as_u64());

                if let Some(method) = method {
                    match method {
                        "agent.thinking" => {
                            let chunk = params
                                .get("chunk")
                                .and_then(|c| c.as_str())
                                .unwrap_or("")
                                .to_string();
                            let _ = reader_app.emit(
                                "agent-thinking",
                                serde_json::json!({
                                    "instance_id": reader_iid,
                                    "chunk": chunk,
                                }),
                            );
                        }
                        "agent.tool_call" => {
                            let _ = reader_app.emit(
                                "agent-tool-call",
                                serde_json::json!({
                                    "instance_id": reader_iid,
                                    "id": params.get("id").and_then(|c| c.as_str()).unwrap_or(""),
                                    "name": params.get("name").and_then(|c| c.as_str()).unwrap_or(""),
                                    "args": params.get("args").cloned().unwrap_or(serde_json::Value::Null),
                                }),
                            );
                        }
                        "agent.tool_result" => {
                            let _ = reader_app.emit(
                                "agent-tool-result",
                                serde_json::json!({
                                    "instance_id": reader_iid,
                                    "id": params.get("id").and_then(|c| c.as_str()).unwrap_or(""),
                                    "name": params.get("name").and_then(|c| c.as_str()).unwrap_or(""),
                                    "success": params.get("success").and_then(|c| c.as_bool()).unwrap_or(false),
                                    "duration_ms": params.get("duration_ms").and_then(|c| c.as_u64()).unwrap_or(0),
                                }),
                            );
                        }
                        "agent.status" => {
                            let new_status = params
                                .get("status")
                                .and_then(|c| c.as_str())
                                .unwrap_or("idle")
                                .to_string();
                            let old_status = {
                                let procs = reader_bridge.lock().await;
                                if let Some(p) = procs.get(&reader_iid) {
                                    p.status.lock().await.clone()
                                } else {
                                    "idle".to_string()
                                }
                            };
                            let _ = reader_app.emit(
                                "agent-status-changed",
                                serde_json::json!({
                                    "instance_id": reader_iid,
                                    "old_status": old_status,
                                    "new_status": new_status,
                                }),
                            );
                            {
                                let mut procs = reader_bridge.lock().await;
                                if let Some(proc) = procs.get_mut(&reader_iid) {
                                    *proc.status.lock().await = new_status;
                                }
                            }
                        }
                        "agent.error" => {
                            let message = params
                                .get("message")
                                .and_then(|c| c.as_str())
                                .unwrap_or("")
                                .to_string();
                            let _ = reader_app.emit(
                                "agent-error",
                                serde_json::json!({
                                    "instance_id": reader_iid,
                                    "message": message,
                                }),
                            );
                        }
                        "agent.complete" => {
                            let content = params
                                .get("content")
                                .and_then(|c| c.as_str())
                                .unwrap_or("")
                                .to_string();
                            let _ = reader_app.emit(
                                "agent-complete",
                                serde_json::json!({
                                    "instance_id": reader_iid,
                                    "content": content,
                                }),
                            );
                        }
                        "agent.history_cleared" => {
                            let _ = reader_app.emit(
                                "agent-history-cleared",
                                serde_json::json!({
                                    "instance_id": reader_iid,
                                }),
                            );
                        }
                        _ => {
                            info!("agent sidecar notification: {}", method);
                        }
                    }
                }

                if let Some(id_num) = id_num {
                    let is_result = msg.get("result").is_some();
                    let is_error = msg.get("error").is_some();

                    if is_result || is_error {
                        let mut procs = reader_bridge.lock().await;
                        if let Some(proc) = procs.get_mut(&reader_iid) {
                            let mut pending = proc.pending.lock().await;
                            if let Some(pending_req) = pending.remove(&id_num) {
                                let send_value = if is_result {
                                    params.clone()
                                } else {
                                    let error_msg =
                                        msg["error"]["message"].as_str().unwrap_or("unknown error");
                                    serde_json::json!({ "error": error_msg })
                                };
                                let _ = pending_req.resolve.send(send_value);
                            }
                        }
                    }
                }
            }
        });

        let proc = AgentProcess {
            child,
            stdin: std::sync::Mutex::new(stdin),
            next_id: std::sync::atomic::AtomicU64::new(2),
            pending: Mutex::new(HashMap::new()),
            status: Mutex::new("idle".to_string()),
        };

        self.send_request_raw(
            &proc,
            1,
            "agent.init",
            &serde_json::json!({
                "apiKey": config.api_key,
                "model": config.model,
                "baseURL": config.base_url,
                "temperature": config.temperature,
                "maxTokens": config.max_tokens,
                "maxRounds": config.max_rounds,
            }),
        )
        .await
        .map_err(|e| format!("agent.init failed: {}", e))?;

        {
            let mut processes = self.processes.lock().await;
            processes.insert(instance_id.to_string(), proc);
        }

        Ok(())
    }

    pub async fn send_message(
        &self,
        instance_id: &str,
        message: &str,
    ) -> Result<serde_json::Value, String> {
        self.send_request(
            instance_id,
            "agent.chat",
            &serde_json::json!({ "message": message }),
        )
        .await
    }

    pub async fn clear_history(&self, instance_id: &str) -> Result<(), String> {
        self.send_request(instance_id, "agent.clearHistory", &serde_json::json!({}))
            .await?;
        Ok(())
    }

    pub async fn stop(&self, instance_id: &str) -> Result<(), String> {
        self.send_request(instance_id, "agent.stop", &serde_json::json!({}))
            .await?;
        Ok(())
    }

    pub async fn shutdown(&self, instance_id: &str) -> Result<(), String> {
        let _ = self
            .send_request(instance_id, "agent.shutdown", &serde_json::json!({}))
            .await;
        let mut processes = self.processes.lock().await;
        if let Some(mut proc) = processes.remove(instance_id) {
            let _ = proc.child.kill();
        }
        Ok(())
    }

    pub async fn is_running(&self, instance_id: &str) -> bool {
        let processes = self.processes.lock().await;
        processes.contains_key(instance_id)
    }

    pub async fn get_status(&self, instance_id: &str) -> Option<String> {
        let processes = self.processes.lock().await;
        if let Some(proc) = processes.get(instance_id) {
            Some(proc.status.lock().await.clone())
        } else {
            None
        }
    }

    async fn send_request(
        &self,
        instance_id: &str,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (tx, rx) = oneshot::channel();

        {
            let processes = self.processes.lock().await;
            let proc = processes
                .get(instance_id)
                .ok_or_else(|| format!("no agent for instance '{}'", instance_id))?;

            let id = proc
                .next_id
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            proc.pending
                .lock()
                .await
                .insert(id, PendingRequest { resolve: tx });

            let msg = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params,
            });

            let line = format!("{}\n", msg);
            let mut stdin = proc.stdin.lock().unwrap_or_else(|e| e.into_inner());
            stdin
                .write_all(line.as_bytes())
                .map_err(|e| format!("write to sidecar: {}", e))?;
        }

        match rx.await {
            Ok(value) => Ok(value),
            Err(_) => Err("sidecar response channel closed".to_string()),
        }
    }

    async fn send_request_raw(
        &self,
        proc: &AgentProcess,
        id: u64,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (tx, rx) = oneshot::channel();

        proc.pending
            .lock()
            .await
            .insert(id, PendingRequest { resolve: tx });

        {
            let msg = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params,
            });

            let line = format!("{}\n", msg);
            let mut stdin = proc.stdin.lock().unwrap_or_else(|e| e.into_inner());
            stdin
                .write_all(line.as_bytes())
                .map_err(|e| format!("write to sidecar: {}", e))?;
        }

        match rx.await {
            Ok(value) => Ok(value),
            Err(_) => Err("sidecar init response channel closed".to_string()),
        }
    }

    pub async fn kill_all(&self) {
        let mut processes = self.processes.lock().await;
        for (_, mut proc) in processes.drain() {
            let _ = proc.child.kill();
        }
    }

    fn find_node_binary() -> Result<std::path::PathBuf, String> {
        if let Ok(path) = std::env::var("NODE_PATH") {
            let p = std::path::PathBuf::from(path);
            if p.exists() {
                return Ok(p);
            }
        }

        let candidates = ["node", "nodejs"];
        for name in &candidates {
            if let Ok(output) = std::process::Command::new("which").arg(name).output() {
                if output.status.success() {
                    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    return Ok(std::path::PathBuf::from(path));
                }
            }
        }

        Err("Node.js not found. Install Node.js or set NODE_PATH.".to_string())
    }

    fn find_sidecar_script() -> Result<std::path::PathBuf, String> {
        let exe_dir = std::env::current_exe()
            .map_err(|e| format!("cannot find exe dir: {}", e))?
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();

        let candidates: Vec<std::path::PathBuf> = [
            exe_dir.join("sidecar.js"),
            exe_dir.join("dist").join("sidecar.js"),
            exe_dir.join("..").join("sidecar.js"),
            exe_dir.join("..").join("dist").join("sidecar.js"),
            exe_dir
                .join("..")
                .join("ai-agent")
                .join("pardus-browser")
                .join("dist")
                .join("sidecar.js"),
        ]
        .to_vec();

        for path in &candidates {
            if path.exists() {
                return Ok(path.clone());
            }
        }

        if let Ok(base) = std::env::var("PARDUS_SIDECAR_PATH") {
            let p = std::path::PathBuf::from(base);
            if p.exists() {
                return Ok(p);
            }
        }

        Err(format!(
            "sidecar.js not found. Searched: {:?}. Set PARDUS_SIDECAR_PATH.",
            candidates
        ))
    }
}
