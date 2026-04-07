use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::{
    AppState,
    agent_bridge::AgentConfig,
    cdp_bridge::{BridgeStatus, CdpEventRecord},
};

// ---------------------------------------------------------------------------
// Instance management commands
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct InstanceInfo {
    pub id: String,
    pub port: u16,
    pub ws_url: String,
    pub running: bool,
    pub browser_window_open: bool,
    pub current_url: Option<String>,
    pub agent_status: String,
}

#[tauri::command]
pub async fn list_instances(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<InstanceInfo>, String> {
    let instances = state
        .instances
        .lock()
        .map_err(|e| format!("lock poisoned: {}", e))?;
    let list: Vec<InstanceInfo> = instances
        .values()
        .map(|inst| InstanceInfo {
            id: inst.id.clone(),
            port: inst.port,
            ws_url: inst.ws_url.clone(),
            running: true,
            browser_window_open: inst.browser_window_label.is_some(),
            current_url: inst.current_url.clone(),
            agent_status: inst.agent_status.clone(),
        })
        .collect();
    Ok(list)
}

#[tauri::command]
pub async fn spawn_instance(state: tauri::State<'_, AppState>) -> Result<InstanceInfo, String> {
    let port = crate::instance::find_free_port(9222);
    let mut child = crate::instance::spawn_browser_process(port)
        .map_err(|e| format!("failed to spawn pardus-browser: {}", e))?;

    if !crate::instance::wait_for_ready(port, 10_000).await {
        let _ = child.kill();
        return Err("pardus-browser failed to start within 10s".to_string());
    }

    let id = {
        let mut next = state
            .next_id
            .lock()
            .map_err(|e| format!("lock poisoned: {}", e))?;
        let val = *next;
        *next += 1;
        format!("instance-{}", val)
    };

    let ws_url = format!("ws://127.0.0.1:{}", port);

    let info = InstanceInfo {
        id: id.clone(),
        port,
        ws_url: ws_url.clone(),
        running: true,
        browser_window_open: false,
        current_url: None,
        agent_status: "idle".to_string(),
    };

    let managed = crate::instance::ManagedInstance {
        id: id.clone(),
        port,
        process: child,
        ws_url,
        browser_window_label: None,
        current_url: None,
        agent_status: "idle".to_string(),
    };

    state
        .instances
        .lock()
        .map_err(|e| format!("lock poisoned: {}", e))?
        .insert(id.clone(), managed);
    Ok(info)
}

#[tauri::command]
pub async fn kill_instance(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.cdp_bridge.disconnect(&id).await;
    let mut instances = state
        .instances
        .lock()
        .map_err(|e| format!("lock poisoned: {}", e))?;
    if let Some(mut inst) = instances.remove(&id) {
        if let Some(label) = &inst.browser_window_label {
            if let Some(window) = app.get_webview_window(label) {
                let _ = window.close();
            }
        }
        let _ = inst.process.kill();
        Ok(())
    } else {
        Err(format!("instance '{}' not found", id))
    }
}

#[tauri::command]
pub async fn kill_all_instances(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let ids: Vec<String> = {
        let instances = state
            .instances
            .lock()
            .map_err(|e| format!("lock poisoned: {}", e))?;
        instances.keys().cloned().collect()
    };
    for id in &ids {
        state.cdp_bridge.disconnect(id).await;
    }
    let mut instances = state
        .instances
        .lock()
        .map_err(|e| format!("lock poisoned: {}", e))?;
    for (_, mut inst) in instances.drain() {
        if let Some(label) = &inst.browser_window_label {
            if let Some(window) = app.get_webview_window(label) {
                let _ = window.close();
            }
        }
        let _ = inst.process.kill();
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// CAPTCHA challenge commands
// ---------------------------------------------------------------------------

/// Open a standalone challenge webview window (for manual use).
#[tauri::command]
pub async fn open_challenge_window(
    app: AppHandle,
    url: String,
    title: Option<String>,
) -> Result<String, String> {
    let sanitized: String = url
        .chars()
        .take(30)
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let label = format!("challenge-{}", sanitized);

    let parsed_url: url::Url = url.parse().map_err(|e: url::ParseError| e.to_string())?;
    let window_title = title.unwrap_or_else(|| "Solve Challenge".to_string());

    WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(parsed_url))
        .title(&window_title)
        .inner_size(480.0, 640.0)
        .resizable(true)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(label)
}

/// Submit cookies obtained from solving a challenge manually.
/// Used when the automatic cookie detection doesn't trigger (e.g. the user
/// copies cookies from the webview's dev tools).
#[tauri::command]
pub async fn submit_challenge_resolution(
    state: tauri::State<'_, AppState>,
    challenge_url: String,
    cookies: String,
    _headers: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let resolver = {
        let resolver_lock = state
            .resolver
            .lock()
            .map_err(|e| format!("lock poisoned: {}", e))?;
        resolver_lock
            .as_ref()
            .ok_or("challenge resolver not initialized")?
            .clone()
    };
    resolver.handle_cookies(challenge_url, cookies).await;
    Ok(())
}

/// Cancel a pending challenge (user gave up).
#[tauri::command]
pub async fn cancel_challenge(
    state: tauri::State<'_, AppState>,
    challenge_url: String,
) -> Result<(), String> {
    let resolver = {
        let resolver_lock = state
            .resolver
            .lock()
            .map_err(|e| format!("lock poisoned: {}", e))?;
        resolver_lock
            .as_ref()
            .ok_or("challenge resolver not initialized")?
            .clone()
    };
    resolver
        .handle_failed(challenge_url, "cancelled by user".to_string())
        .await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Browser window commands
// ---------------------------------------------------------------------------

/// Open a visual browser window for an instance.
///
/// First navigates the headless pardus-browser via CDP, then opens a
/// companion webview showing the same URL. All interactions in the
/// webview are forwarded to the headless browser for processing.
#[tauri::command]
pub async fn open_browser_window(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    instance_id: String,
    url: Option<String>,
) -> Result<(), String> {
    // Verify the instance exists
    {
        let instances = state
            .instances
            .lock()
            .map_err(|e| format!("lock poisoned: {}", e))?;
        instances
            .get(&instance_id)
            .ok_or_else(|| format!("instance '{}' not found", instance_id))?;
    }

    let target_url = url.unwrap_or_else(|| "https://example.com".to_string());

    // Navigate the headless pardus-browser via CDP first
    let nav_result = state
        .cdp_bridge
        .send_command(
            &instance_id,
            "Page.navigate".to_string(),
            serde_json::json!({ "url": target_url }),
        )
        .await;

    match nav_result {
        Ok(resp) => {
            tracing::info!(
                instance_id = %instance_id,
                url = %target_url,
                response = %resp,
                "pardus-browser navigated via CDP"
            );
        }
        Err(e) => {
            tracing::warn!(
                instance_id = %instance_id,
                error = %e,
                "CDP navigation failed, opening webview anyway"
            );
        }
    }

    // Open the companion visual webview
    let label = crate::browser_window::open_browser_window(&app, &instance_id, &target_url)?;

    let mut instances = state
        .instances
        .lock()
        .map_err(|e| format!("lock poisoned: {}", e))?;
    if let Some(inst) = instances.get_mut(&instance_id) {
        inst.browser_window_label = Some(label);
        inst.current_url = Some(target_url);
    }

    Ok(())
}

/// Navigate the browser window for an instance.
#[tauri::command]
pub async fn navigate_browser_window(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    instance_id: String,
    url: String,
) -> Result<(), String> {
    let label = {
        let instances = state
            .instances
            .lock()
            .map_err(|e| format!("lock poisoned: {}", e))?;
        instances
            .get(&instance_id)
            .and_then(|i| i.browser_window_label.clone())
            .ok_or_else(|| format!("no browser window for instance '{}'", instance_id))?
    };

    let parsed_url: url::Url = url.parse().map_err(|e: url::ParseError| e.to_string())?;

    if let Some(window) = app.get_webview_window(&label) {
        // Navigate by closing and reopening (Tauri 2 webview navigation)
        let _ = window.close();
    }

    let new_label = crate::browser_window::open_browser_window(&app, &instance_id, url.as_str())?;

    let mut instances = state
        .instances
        .lock()
        .map_err(|e| format!("lock poisoned: {}", e))?;
    if let Some(inst) = instances.get_mut(&instance_id) {
        inst.browser_window_label = Some(new_label);
        inst.current_url = Some(url);
    }

    Ok(())
}

/// Close the browser window for an instance.
#[tauri::command]
pub async fn close_browser_window(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<(), String> {
    crate::browser_window::close_browser_window(&app, &instance_id)?;

    let mut instances = state
        .instances
        .lock()
        .map_err(|e| format!("lock poisoned: {}", e))?;
    if let Some(inst) = instances.get_mut(&instance_id) {
        inst.browser_window_label = None;
        inst.current_url = None;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// CDP bridge commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn connect_instance(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<(), String> {
    let port = {
        let instances = state
            .instances
            .lock()
            .map_err(|e| format!("lock poisoned: {}", e))?;
        instances
            .get(&instance_id)
            .ok_or_else(|| format!("instance '{}' not found", instance_id))?
            .port
    };

    state
        .cdp_bridge
        .connect(instance_id.clone(), port, app)
        .await;

    Ok(())
}

#[tauri::command]
pub async fn disconnect_instance(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<(), String> {
    state.cdp_bridge.disconnect(&instance_id).await;

    let mut instances = state
        .instances
        .lock()
        .map_err(|e| format!("lock poisoned: {}", e))?;
    if let Some(inst) = instances.get_mut(&instance_id) {
        inst.agent_status = "idle".to_string();
    }

    Ok(())
}

#[tauri::command]
pub async fn execute_cdp(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    method: String,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let resp = state
        .cdp_bridge
        .send_command(&instance_id, method, params)
        .await?;

    // CDP responses wrap the payload in {"id":N,"result":{...}} — extract inner result
    if let Some(result) = resp.get("result").cloned() {
        Ok(result)
    } else if resp.get("error").is_some() {
        Err(resp["error"]["message"]
            .as_str()
            .unwrap_or("CDP error")
            .to_string())
    } else {
        Ok(resp)
    }
}

#[tauri::command]
pub async fn get_semantic_tree(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<serde_json::Value, String> {
    let resp = state
        .cdp_bridge
        .send_command(
            &instance_id,
            "Pardus.semanticTree".to_string(),
            serde_json::json!({}),
        )
        .await?;

    // CDP responses wrap the payload in {"id":N,"result":{...}} — extract inner result
    if let Some(result) = resp.get("result").cloned() {
        Ok(result)
    } else if resp.get("error").is_some() {
        Err(resp["error"]["message"]
            .as_str()
            .unwrap_or("CDP error")
            .to_string())
    } else {
        Ok(resp)
    }
}

#[tauri::command]
pub async fn get_instance_events(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    limit: Option<usize>,
    since: Option<i64>,
) -> Result<Vec<CdpEventRecord>, String> {
    Ok(state
        .cdp_bridge
        .get_events(&instance_id, limit.unwrap_or(100), since)
        .await)
}

#[tauri::command]
pub async fn get_bridge_status(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<BridgeStatus, String> {
    state
        .cdp_bridge
        .get_status(&instance_id)
        .await
        .ok_or_else(|| format!("no bridge for instance '{}'", instance_id))
}

// ---------------------------------------------------------------------------
// Agent status commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn set_agent_status(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    status: String,
) -> Result<(), String> {
    let valid = [
        "idle",
        "connected",
        "running",
        "paused",
        "waiting-challenge",
        "error",
    ];
    if !valid.contains(&status.as_str()) {
        return Err(format!(
            "invalid status '{}'. expected: {}",
            status,
            valid.join(", ")
        ));
    }

    let mut instances = state
        .instances
        .lock()
        .map_err(|e| format!("lock poisoned: {}", e))?;
    if let Some(inst) = instances.get_mut(&instance_id) {
        let old = inst.agent_status.clone();
        inst.agent_status = status.clone();

        drop(instances);

        if let Some(handle) = state
            .app_handle
            .lock()
            .map_err(|e| format!("lock poisoned: {}", e))?
            .as_ref()
        {
            let _ = handle.emit(
                "agent-status-changed",
                serde_json::json!({
                    "instance_id": instance_id,
                    "old_status": old,
                    "new_status": status,
                }),
            );
        }
    } else {
        return Err(format!("instance '{}' not found", instance_id));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// AI Agent commands
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfigPayload {
    pub api_key: String,
    pub model: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_max_rounds")]
    pub max_rounds: u32,
}

fn default_temperature() -> f64 { 0.7 }

fn default_max_tokens() -> u32 { 4000 }

fn default_max_rounds() -> u32 { 50 }

#[tauri::command]
pub async fn start_agent(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    instance_id: String,
    config: AgentConfigPayload,
) -> Result<(), String> {
    {
        let instances = state
            .instances
            .lock()
            .map_err(|e| format!("lock poisoned: {}", e))?;
        instances
            .get(&instance_id)
            .ok_or_else(|| format!("instance '{}' not found", instance_id))?;
    }

    let agent_config = AgentConfig {
        api_key: config.api_key,
        model: config.model,
        base_url: config.base_url,
        temperature: config.temperature,
        max_tokens: config.max_tokens,
        max_rounds: config.max_rounds,
    };

    state
        .agent_bridge
        .spawn(&instance_id, agent_config, app)
        .await?;

    {
        let mut instances = state
            .instances
            .lock()
            .map_err(|e| format!("lock poisoned: {}", e))?;
        if let Some(inst) = instances.get_mut(&instance_id) {
            inst.agent_status = "connected".to_string();
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn send_agent_message(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    message: String,
) -> Result<serde_json::Value, String> {
    let result = state
        .agent_bridge
        .send_message(&instance_id, &message)
        .await?;

    if let Some(error) = result.get("error").and_then(|e| e.as_str()) {
        Err(error.to_string())
    } else {
        Ok(result)
    }
}

#[tauri::command]
pub async fn stop_agent(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<(), String> {
    state.agent_bridge.stop(&instance_id).await?;

    let mut instances = state
        .instances
        .lock()
        .map_err(|e| format!("lock poisoned: {}", e))?;
    if let Some(inst) = instances.get_mut(&instance_id) {
        inst.agent_status = "connected".to_string();
    }

    Ok(())
}

#[tauri::command]
pub async fn clear_agent_history(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<(), String> {
    state.agent_bridge.clear_history(&instance_id).await?;
    Ok(())
}

#[tauri::command]
pub async fn shutdown_agent(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<(), String> {
    state.agent_bridge.shutdown(&instance_id).await?;

    let mut instances = state
        .instances
        .lock()
        .map_err(|e| format!("lock poisoned: {}", e))?;
    if let Some(inst) = instances.get_mut(&instance_id) {
        inst.agent_status = "idle".to_string();
    }

    if let Some(handle) = state
        .app_handle
        .lock()
        .map_err(|e| format!("lock poisoned: {}", e))?
        .as_ref()
    {
        let _ = handle.emit(
            "agent-status-changed",
            serde_json::json!({
                "instance_id": instance_id,
                "old_status": "running",
                "new_status": "idle",
            }),
        );
    }

    let _ = app.emit(
        "agent-shutdown",
        serde_json::json!({ "instance_id": instance_id }),
    );

    Ok(())
}

#[tauri::command]
pub async fn get_agent_status(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<Option<String>, String> {
    Ok(state.agent_bridge.get_status(&instance_id).await)
}

#[tauri::command]
pub async fn is_agent_running(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<bool, String> {
    Ok(state.agent_bridge.is_running(&instance_id).await)
}

#[tauri::command]
pub async fn resume_agent(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    message: String,
) -> Result<serde_json::Value, String> {
    {
        let instances = state.instances.lock().unwrap();
        instances
            .get(&instance_id)
            .ok_or_else(|| format!("instance '{}' not found", instance_id))?;
    }

    let result = state
        .agent_bridge
        .send_message(&instance_id, &message)
        .await?;

    {
        let mut instances = state.instances.lock().unwrap();
        if let Some(inst) = instances.get_mut(&instance_id) {
            let old = inst.agent_status.clone();
            inst.agent_status = "running".to_string();
            drop(instances);
            if let Some(handle) = state.app_handle.lock().unwrap().as_ref() {
                let _ = handle.emit(
                    "agent-status-changed",
                    serde_json::json!({
                        "instance_id": instance_id,
                        "old_status": old,
                        "new_status": "running",
                    }),
                );
            }
        }
    }

    Ok(result)
}
