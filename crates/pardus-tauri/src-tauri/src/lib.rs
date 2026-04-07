mod agent_bridge;
mod browser_window;
mod cdp_bridge;
mod challenge;
mod commands;
mod cookie_bridge;
mod instance;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tauri::{Emitter, Listener, Manager};

pub struct AppState {
    pub instances: Mutex<HashMap<String, instance::ManagedInstance>>,
    pub next_id: Mutex<u32>,
    pub resolver: Mutex<Option<Arc<challenge::TauriChallengeResolver>>>,
    pub cdp_bridge: cdp_bridge::CdpBridge,
    pub agent_bridge: agent_bridge::AgentBridge,
    pub app_handle: Mutex<Option<tauri::AppHandle>>,
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            instances: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
            resolver: Mutex::new(None),
            cdp_bridge: cdp_bridge::CdpBridge::new(),
            agent_bridge: agent_bridge::AgentBridge::new(),
            app_handle: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_instances,
            commands::spawn_instance,
            commands::kill_instance,
            commands::kill_all_instances,
            commands::open_challenge_window,
            commands::submit_challenge_resolution,
            commands::cancel_challenge,
            commands::open_browser_window,
            commands::navigate_browser_window,
            commands::close_browser_window,
            commands::connect_instance,
            commands::disconnect_instance,
            commands::execute_cdp,
            commands::get_semantic_tree,
            commands::get_instance_events,
            commands::get_bridge_status,
            commands::set_agent_status,
            commands::start_agent,
            commands::send_agent_message,
            commands::stop_agent,
            commands::clear_agent_history,
            commands::shutdown_agent,
            commands::get_agent_status,
            commands::is_agent_running,
            commands::resume_agent,
        ])
        .setup(|app| {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info".into()),
                )
                .init();

            let app_handle = app.handle().clone();

            {
                let state = app.state::<AppState>();
                *state.app_handle.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(app_handle.clone());
            }

            let resolver = Arc::new(challenge::TauriChallengeResolver::new(app_handle));

            let state = app.state::<AppState>();
            *state.resolver.lock().unwrap_or_else(|e| e.into_inner()) = Some(resolver.clone());

            let r_cookies = resolver.clone();
            app.listen("challenge-cookies", move |event| {
                let payload = event.payload();
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(payload) {
                    let url = data["url"].as_str().unwrap_or("").to_string();
                    let cookies = data["cookies"].as_str().unwrap_or("").to_string();
                    let r = r_cookies.clone();
                    tauri::async_runtime::spawn(async move {
                        r.handle_cookies(url, cookies).await;
                    });
                }
            });

            let r_timeout = resolver.clone();
            app.listen("challenge-timeout", move |event| {
                let payload = event.payload();
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(payload) {
                    let url = data["url"].as_str().unwrap_or("").to_string();
                    let r = r_timeout.clone();
                    tauri::async_runtime::spawn(async move {
                        r.handle_failed(url, "challenge timed out (5 minutes)".to_string())
                            .await;
                    });
                }
            });

            let nav_handle = app.handle().clone();
            app.listen("browser-navigate", move |event| {
                let payload = event.payload();
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(payload) {
                    let instance_id = data["instance_id"].as_str().unwrap_or("").to_string();
                    let url = data["url"].as_str().unwrap_or("").to_string();
                    let h = nav_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let label = format!("browser-{}", instance_id);
                        if let Some(window) = h.get_webview_window(&label) {
                            let _ = window.close();
                        }
                        if let Ok(_new_label) =
                            browser_window::open_browser_window(&h, &instance_id, &url)
                        {
                            let state = h.state::<AppState>();
                            let mut instances =
                                state.instances.lock().unwrap_or_else(|e| e.into_inner());
                            if let Some(inst) = instances.get_mut(&instance_id) {
                                inst.current_url = Some(url);
                            }
                        }
                    });
                }
            });

            let url_handle = app.handle().clone();
            app.listen("browser-url-changed", move |event| {
                let payload = event.payload();
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(payload) {
                    let instance_id = data["instance_id"].as_str().unwrap_or("").to_string();
                    let url = data["url"].as_str().unwrap_or("").to_string();
                    let h = url_handle.clone();
                    let state = h.state::<AppState>();
                    let mut instances = state.instances.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(inst) = instances.get_mut(&instance_id) {
                        inst.current_url = Some(url.to_string());
                    }
                }
            });

            app.listen("challenge-detected", {
                let h = app.handle().clone();
                move |event| {
                    let payload = event.payload();
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(payload) {
                        let challenge_url = data["url"].as_str().unwrap_or("").to_string();
                        let h = h.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = h.state::<AppState>();
                            let instances =
                                state.instances.lock().unwrap_or_else(|e| e.into_inner());
                            for (inst_id, inst) in instances.iter() {
                                if inst.current_url.as_deref() == Some(challenge_url.as_str()) {
                                    let inst_id = inst_id.clone();
                                    drop(instances);
                                    let state = h.state::<AppState>();
                                    let mut instances =
                                        state.instances.lock().unwrap_or_else(|e| e.into_inner());
                                    if let Some(inst) = instances.get_mut(&inst_id) {
                                        if inst.agent_status != "waiting-challenge" {
                                            inst.agent_status = "waiting-challenge".to_string();
                                            let _ = h.emit(
                                                "agent-status-changed",
                                                serde_json::json!({
                                                    "instance_id": inst_id,
                                                    "old_status": "running",
                                                    "new_status": "waiting-challenge",
                                                }),
                                            );
                                        }
                                    }
                                    return;
                                }
                            }
                        });
                    }
                }
            });

            app.listen("challenge-solved", {
                let h = app.handle().clone();
                move |event| {
                    let payload = event.payload();
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(payload) {
                        let solved_url = data["url"].as_str().unwrap_or("").to_string();
                        let h = h.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = h.state::<AppState>();
                            let instances =
                                state.instances.lock().unwrap_or_else(|e| e.into_inner());
                            for (inst_id, inst) in instances.iter() {
                                if inst.agent_status == "waiting-challenge"
                                    && inst.current_url.as_deref() == Some(solved_url.as_str())
                                {
                                    let inst_id = inst_id.clone();
                                    drop(instances);
                                    let state = h.state::<AppState>();
                                    let mut instances =
                                        state.instances.lock().unwrap_or_else(|e| e.into_inner());
                                    if let Some(inst) = instances.get_mut(&inst_id) {
                                        inst.agent_status = "running".to_string();
                                        let _ = h.emit(
                                            "agent-status-changed",
                                            serde_json::json!({
                                                "instance_id": inst_id,
                                                "old_status": "waiting-challenge",
                                                "new_status": "running",
                                            }),
                                        );
                                    }
                                    return;
                                }
                            }
                        });
                    }
                }
            });

            app.listen("challenge-failed", {
                let h = app.handle().clone();
                move |event| {
                    let payload = event.payload();
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(payload) {
                        let failed_url = data["challenge_url"].as_str().unwrap_or("").to_string();
                        let h = h.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = h.state::<AppState>();
                            let instances =
                                state.instances.lock().unwrap_or_else(|e| e.into_inner());
                            for (inst_id, inst) in instances.iter() {
                                if inst.agent_status == "waiting-challenge"
                                    && inst.current_url.as_deref() == Some(failed_url.as_str())
                                {
                                    let inst_id = inst_id.clone();
                                    drop(instances);
                                    let state = h.state::<AppState>();
                                    let mut instances =
                                        state.instances.lock().unwrap_or_else(|e| e.into_inner());
                                    if let Some(inst) = instances.get_mut(&inst_id) {
                                        inst.agent_status = "error".to_string();
                                        let _ = h.emit(
                                            "agent-status-changed",
                                            serde_json::json!({
                                                "instance_id": inst_id,
                                                "old_status": "waiting-challenge",
                                                "new_status": "error",
                                            }),
                                        );
                                    }
                                    return;
                                }
                            }
                        });
                    }
                }
            });

            app.listen("cdp-bridge-connected", {
                let h = app.handle().clone();
                move |event| {
                    let payload = event.payload();
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(payload) {
                        let inst_id = data["instance_id"].as_str().unwrap_or("").to_string();
                        let h = h.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = h.state::<AppState>();
                            let mut instances =
                                state.instances.lock().unwrap_or_else(|e| e.into_inner());
                            if let Some(inst) = instances.get_mut(&inst_id) {
                                if inst.agent_status == "idle" {
                                    inst.agent_status = "connected".to_string();
                                    let _ = h.emit(
                                        "agent-status-changed",
                                        serde_json::json!({
                                            "instance_id": inst_id,
                                            "old_status": "idle",
                                            "new_status": "connected",
                                        }),
                                    );
                                }
                            }
                        });
                    }
                }
            });

            app.listen("cdp-event", {
                let h = app.handle().clone();
                move |event| {
                    let payload = event.payload();
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(payload) {
                        let instance_id = data["instance_id"].as_str().unwrap_or("").to_string();
                        let method = data["method"].as_str().unwrap_or("").to_string();

                        let is_action_event = method.starts_with("Pardus.action");
                        let is_navigation = method == "Page.frameNavigated";

                        if !is_action_event && !is_navigation {
                            return;
                        }

                        if is_navigation {
                            let has_parent = data["params"]["frame"].get("parentId").is_some();
                            if !has_parent {
                                if let Some(url) = data["params"]["frame"]["url"].as_str() {
                                    let h = h.clone();
                                    let inst_id = instance_id.clone();
                                    let url = url.to_string();
                                    tauri::async_runtime::spawn(async move {
                                        let state = h.state::<AppState>();
                                        let mut instances = state
                                            .instances
                                            .lock()
                                            .unwrap_or_else(|e| e.into_inner());
                                        if let Some(inst) = instances.get_mut(&inst_id) {
                                            inst.current_url = Some(url);
                                            if inst.agent_status == "connected"
                                                || inst.agent_status == "idle"
                                            {
                                                inst.agent_status = "running".to_string();
                                                let _ = h.emit(
                                                    "agent-status-changed",
                                                    serde_json::json!({
                                                        "instance_id": inst_id,
                                                        "old_status": inst.agent_status.clone(),
                                                        "new_status": "running",
                                                    }),
                                                );
                                            }
                                        }
                                    });
                                }
                            }
                        }

                        if is_action_event && method == "Pardus.actionStarted" {
                            let h = h.clone();
                            let inst_id = instance_id.clone();
                            tauri::async_runtime::spawn(async move {
                                let state = h.state::<AppState>();
                                let mut instances =
                                    state.instances.lock().unwrap_or_else(|e| e.into_inner());
                                if let Some(inst) = instances.get_mut(&inst_id) {
                                    if inst.agent_status == "connected"
                                        || inst.agent_status == "idle"
                                    {
                                        inst.agent_status = "running".to_string();
                                    }
                                }
                            });
                        }
                    }
                }
            });

            // -----------------------------------------------------------------
            // Browser interaction listener — forwards OS webview actions to
            // the Pardus headless browser via CDP
            // -----------------------------------------------------------------
            app.listen("browser-interaction", {
                let h = app.handle().clone();
                move |event| {
                    let payload = event.payload();
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(payload) {
                        let instance_id = data["instance_id"].as_str().unwrap_or("").to_string();
                        let action = data["action"].as_str().unwrap_or("click").to_string();
                        let selector = data["selector"].as_str().unwrap_or("").to_string();
                        let value = data["value"].as_str().unwrap_or("").to_string();
                        let href = data["href"].as_str().unwrap_or("").to_string();

                        if instance_id.is_empty() || selector.is_empty() {
                            return;
                        }

                        let h = h.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = h.state::<AppState>();

                            // Check that a CDP bridge exists for this instance
                            let bridge_status = state.cdp_bridge.get_status(&instance_id).await;
                            if bridge_status.is_none() {
                                tracing::warn!(
                                    instance_id = %instance_id,
                                    "browser-interaction ignored: no CDP bridge"
                                );
                                return;
                            }

                            // For links with href, use the href to navigate in the headless browser
                            // rather than relying on CSS selector matching
                            let cdp_action = if !href.is_empty() && action == "click" {
                                "click".to_string()
                            } else {
                                action.clone()
                            };

                            let mut cdp_params = serde_json::json!({
                                "action": cdp_action,
                                "selector": selector,
                            });

                            if !value.is_empty() {
                                cdp_params["value"] = serde_json::json!(value);
                            }

                            // For links, pass the href so the headless browser can navigate
                            // directly
                            if !href.is_empty() && action == "click" {
                                cdp_params["href"] = serde_json::json!(href);
                            }

                            tracing::info!(
                                instance_id = %instance_id,
                                action = %cdp_action,
                                selector = %selector,
                                "forwarding webview interaction to headless browser"
                            );

                            let result = state
                                .cdp_bridge
                                .send_command(
                                    &instance_id,
                                    "Pardus.interact".to_string(),
                                    cdp_params,
                                )
                                .await;

                            match result {
                                Ok(resp) => {
                                    let success = resp["success"].as_bool().unwrap_or(false);
                                    let navigated = resp["navigated"].as_bool().unwrap_or(false);
                                    let new_url = resp["url"].as_str().unwrap_or("");

                                    tracing::info!(
                                        instance_id = %instance_id,
                                        success = success,
                                        navigated = navigated,
                                        new_url = %new_url,
                                        "headless browser interaction result"
                                    );

                                    // Sync webview to new URL if the headless browser navigated
                                    if success && navigated && !new_url.is_empty() {
                                        // Update instance state
                                        {
                                            let mut instances = state
                                                .instances
                                                .lock()
                                                .unwrap_or_else(|e| e.into_inner());
                                            if let Some(inst) = instances.get_mut(&instance_id) {
                                                inst.current_url = Some(new_url.to_string());
                                            }
                                        }

                                        // Navigate the OS webview to the new URL
                                        let label = format!("browser-{}", instance_id);
                                        if let Some(window) = h.get_webview_window(&label) {
                                            // Use eval to navigate the webview
                                            let js = format!(
                                                "window.__pardusInterceptor = false; \
                                                 window.location.href = {};",
                                                serde_json::to_string(&new_url).unwrap_or_default()
                                            );
                                            let _ = window.eval(&js);
                                        }

                                        // Notify frontend about URL change and semantic tree update
                                        let _ = h.emit(
                                            "browser-url-changed",
                                            serde_json::json!({
                                                "instance_id": instance_id,
                                                "url": new_url,
                                            }),
                                        );
                                    }

                                    // Emit action log event for the frontend
                                    let _ = h.emit(
                                        "webview-action-log",
                                        serde_json::json!({
                                            "instance_id": instance_id,
                                            "action": cdp_action,
                                            "selector": selector,
                                            "success": success,
                                            "navigated": navigated,
                                            "url": new_url,
                                        }),
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        instance_id = %instance_id,
                                        error = %e,
                                        "headless browser interaction failed"
                                    );
                                }
                            }
                        });
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
