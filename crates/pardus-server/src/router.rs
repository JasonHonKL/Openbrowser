use std::sync::Arc;

use axum::extract::Request;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::handlers::*;
use crate::state::ServerState;
use crate::static_files;

/// Build the axum router with all API routes and static file serving.
pub fn build_router(state: Arc<ServerState>, dev_mode: bool) -> Router {
    let api = Router::new()
        // Pages
        .route("/api/pages/navigate", post(pages_navigate))
        .route("/api/pages/reload", post(pages_reload))
        .route("/api/pages/current", get(pages_current))
        .route("/api/pages/html", get(pages_html))
        // Tabs
        .route("/api/tabs", get(tabs_list))
        .route("/api/tabs", post(tabs_create))
        .route("/api/tabs/{id}", delete(tabs_close))
        .route("/api/tabs/{id}/activate", post(tabs_activate))
        // Semantic
        .route("/api/semantic/tree", get(semantic_tree))
        .route("/api/semantic/element/{id}", get(semantic_element))
        .route("/api/semantic/stats", get(semantic_stats))
        // Interact
        .route("/api/interact/click", post(interact_click))
        .route("/api/interact/type", post(interact_type))
        .route("/api/interact/submit", post(interact_submit))
        .route("/api/interact/scroll", post(interact_scroll))
        .route("/api/interact/elements", get(interact_elements))
        // Network
        .route("/api/network/requests", get(network_requests))
        .route("/api/network/requests", delete(network_requests_clear))
        .route("/api/network/har", get(network_har))
        // Cookies
        .route("/api/cookies", get(cookies_list))
        .route("/api/cookies", post(cookies_set))
        .route("/api/cookies/{name}", delete(cookies_delete))
        .route("/api/cookies", delete(cookies_clear))
        // Health
        .route("/api/health", get(health))
        // WebSocket
        .route("/ws", get(crate::ws::ws_handler))
        .with_state(state);

    let cors = CorsLayer::permissive();

    if dev_mode {
        Router::new()
            .merge(api)
            .layer(TraceLayer::new_for_http())
            .layer(cors)
            .fallback(dev_static_handler)
    } else {
        Router::new()
            .merge(api)
            .layer(TraceLayer::new_for_http())
            .layer(cors)
            .fallback(embedded_static_handler)
    }
}

async fn embedded_static_handler(req: Request) -> impl IntoResponse {
    let path = req.uri().path().to_string();
    static_files::serve_embedded(&path).await
}

async fn dev_static_handler(req: Request) -> impl IntoResponse {
    let path = req.uri().path().to_string();
    static_files::serve_filesystem(&path, "web/dist").await
}
