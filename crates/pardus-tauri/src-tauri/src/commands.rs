use tauri::{AppHandle, WebviewUrl, WebviewWindowBuilder};
use serde::Serialize;

use crate::AppState;

// ---------------------------------------------------------------------------
// Instance management commands
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct InstanceInfo {
    pub id: String,
    pub port: u16,
    pub ws_url: String,
    pub running: bool,
}

#[tauri::command]
pub async fn list_instances(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<InstanceInfo>, String> {
    let instances = state.instances.lock().unwrap();
    let list: Vec<InstanceInfo> = instances
        .values()
        .map(|inst| InstanceInfo {
            id: inst.id.clone(),
            port: inst.port,
            ws_url: inst.ws_url.clone(),
            running: true,
        })
        .collect();
    Ok(list)
}

#[tauri::command]
pub async fn spawn_instance(
    state: tauri::State<'_, AppState>,
) -> Result<InstanceInfo, String> {
    let port = crate::instance::find_free_port(9222);
    let mut child = crate::instance::spawn_browser_process(port)
        .map_err(|e| format!("failed to spawn pardus-browser: {}", e))?;

    if !crate::instance::wait_for_ready(port, 10_000).await {
        let _ = child.kill();
        return Err("pardus-browser failed to start within 10s".to_string());
    }

    let id = {
        let mut next = state.next_id.lock().unwrap();
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
    };

    let managed = crate::instance::ManagedInstance {
        id: id.clone(),
        port,
        process: child,
        ws_url,
    };

    state.instances.lock().unwrap().insert(id.clone(), managed);
    Ok(info)
}

#[tauri::command]
pub async fn kill_instance(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let mut instances = state.instances.lock().unwrap();
    if let Some(mut inst) = instances.remove(&id) {
        let _ = inst.process.kill();
        Ok(())
    } else {
        Err(format!("instance '{}' not found", id))
    }
}

#[tauri::command]
pub async fn kill_all_instances(
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut instances = state.instances.lock().unwrap();
    for (_, mut inst) in instances.drain() {
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
    let sanitized: String = url.chars().take(30).map(|c| {
        if c.is_alphanumeric() { c } else { '-' }
    }).collect();
    let label = format!("challenge-{}", sanitized);

    let parsed_url: url::Url = url.parse().map_err(|e: url::ParseError| e.to_string())?;
    let window_title = title.unwrap_or_else(|| "Solve Challenge".to_string());

    WebviewWindowBuilder::new(
        &app,
        &label,
        WebviewUrl::External(parsed_url),
    )
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
        let resolver_lock = state.resolver.lock().unwrap();
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
        let resolver_lock = state.resolver.lock().unwrap();
        resolver_lock
            .as_ref()
            .ok_or("challenge resolver not initialized")?
            .clone()
    };
    resolver.handle_failed(challenge_url, "cancelled by user".to_string()).await;
    Ok(())
}
