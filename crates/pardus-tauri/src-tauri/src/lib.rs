mod challenge;
mod commands;
mod instance;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use tauri::{Listener, Manager};

pub struct AppState {
    pub instances: Mutex<HashMap<String, instance::ManagedInstance>>,
    pub next_id: Mutex<u32>,
    pub resolver: Mutex<Option<Arc<challenge::TauriChallengeResolver>>>,
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            instances: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
            resolver: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_instances,
            commands::spawn_instance,
            commands::kill_instance,
            commands::kill_all_instances,
            commands::open_challenge_window,
            commands::submit_challenge_resolution,
            commands::cancel_challenge,
        ])
        .setup(|app| {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info".into()),
                )
                .init();

            let app_handle = app.handle().clone();
            let resolver = Arc::new(challenge::TauriChallengeResolver::new(app_handle));

            // Store resolver in state
            let state = app.state::<AppState>();
            *state.resolver.lock().unwrap() = Some(resolver.clone());

            // Listen for cookie events from challenge webviews
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

            // Listen for timeout events from challenge webviews
            let r_timeout = resolver.clone();
            app.listen("challenge-timeout", move |event| {
                let payload = event.payload();
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(payload) {
                    let url = data["url"].as_str().unwrap_or("").to_string();
                    let r = r_timeout.clone();
                    tauri::async_runtime::spawn(async move {
                        r.handle_failed(url, "challenge timed out (5 minutes)".to_string()).await;
                    });
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
