use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use crate::state::{BrowserCmd, BrowserResponse, ServerState};

// ---------------------------------------------------------------------------
// Helper: send command and await response
// ---------------------------------------------------------------------------

async fn send_cmd(
    state: &Arc<ServerState>,
    make_cmd: impl FnOnce(tokio::sync::oneshot::Sender<anyhow::Result<BrowserResponse>>) -> BrowserCmd,
) -> Response {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let cmd = make_cmd(tx);
    if state.cmd_tx.send(cmd).await.is_err() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Browser task not available" })),
        )
            .into_response();
    }
    match rx.await {
        Ok(Ok(resp)) => response_from_browser(resp),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Browser task dropped response" })),
        )
            .into_response(),
    }
}

fn response_from_browser(resp: BrowserResponse) -> Response {
    match resp {
        BrowserResponse::Ok { ok } => Json(serde_json::json!({ "ok": ok })).into_response(),
        BrowserResponse::PageSnapshot(s) => Json(serde_json::to_value(s).unwrap_or_default())
            .into_response(),
        BrowserResponse::Html { html } => {
            Json(serde_json::json!({ "html": html })).into_response()
        }
        BrowserResponse::Tabs { tabs } => {
            Json(serde_json::json!({ "tabs": tabs })).into_response()
        }
        BrowserResponse::TabId { id } => {
            Json(serde_json::json!({ "id": id })).into_response()
        }
        BrowserResponse::SemanticTree(val) => Json(val).into_response(),
        BrowserResponse::Element(opt) => match opt {
            Some(val) => Json(val).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Element not found" })),
            )
                .into_response(),
        },
        BrowserResponse::Stats(val) => Json(val).into_response(),
        BrowserResponse::InteractiveElements { elements } => {
            Json(serde_json::json!({ "elements": elements })).into_response()
        }
        BrowserResponse::NetworkRecords { requests } => {
            Json(serde_json::json!({ "requests": requests })).into_response()
        }
        BrowserResponse::Har(val) => Json(val).into_response(),
        BrowserResponse::Cookies { cookies } => {
            Json(serde_json::json!({ "cookies": cookies })).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct NavigateBody {
    pub url: String,
}

pub async fn pages_navigate(
    State(state): State<Arc<ServerState>>,
    Json(body): Json<NavigateBody>,
) -> Response {
    send_cmd(&state, |reply| BrowserCmd::Navigate {
        url: body.url,
        reply,
    })
    .await
}

pub async fn pages_reload(State(state): State<Arc<ServerState>>) -> Response {
    send_cmd(&state, |reply| BrowserCmd::Reload { reply }).await
}

pub async fn pages_current(State(state): State<Arc<ServerState>>) -> Response {
    send_cmd(&state, |reply| BrowserCmd::CurrentPage { reply }).await
}

pub async fn pages_html(State(state): State<Arc<ServerState>>) -> Response {
    send_cmd(&state, |reply| BrowserCmd::Html { reply }).await
}

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

pub async fn tabs_list(State(state): State<Arc<ServerState>>) -> Response {
    send_cmd(&state, |reply| BrowserCmd::ListTabs { reply }).await
}

#[derive(Deserialize)]
pub struct CreateTabBody {
    pub url: String,
}

pub async fn tabs_create(
    State(state): State<Arc<ServerState>>,
    Json(body): Json<CreateTabBody>,
) -> Response {
    send_cmd(&state, |reply| BrowserCmd::OpenTab {
        url: body.url,
        reply,
    })
    .await
}

pub async fn tabs_close(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<u64>,
) -> Response {
    send_cmd(&state, |reply| BrowserCmd::CloseTab { id, reply }).await
}

pub async fn tabs_activate(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<u64>,
) -> Response {
    send_cmd(&state, |reply| BrowserCmd::ActivateTab { id, reply }).await
}

// ---------------------------------------------------------------------------
// Semantic
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SemanticTreeQuery {
    pub format: Option<String>,
}

pub async fn semantic_tree(
    State(state): State<Arc<ServerState>>,
    Query(query): Query<SemanticTreeQuery>,
) -> Response {
    let flat = query.format.as_deref() == Some("flat");
    send_cmd(&state, |reply| BrowserCmd::SemanticTree { flat, reply }).await
}

pub async fn semantic_element(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<usize>,
) -> Response {
    send_cmd(&state, |reply| BrowserCmd::SemanticElement { id, reply }).await
}

pub async fn semantic_stats(State(state): State<Arc<ServerState>>) -> Response {
    send_cmd(&state, |reply| BrowserCmd::SemanticStats { reply }).await
}

// ---------------------------------------------------------------------------
// Interact
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ClickBody {
    pub element_id: Option<usize>,
    pub selector: Option<String>,
}

pub async fn interact_click(
    State(state): State<Arc<ServerState>>,
    Json(body): Json<ClickBody>,
) -> Response {
    send_cmd(&state, |reply| BrowserCmd::Click {
        element_id: body.element_id,
        selector: body.selector,
        reply,
    })
    .await
}

#[derive(Deserialize)]
pub struct TypeBody {
    pub element_id: Option<usize>,
    pub selector: Option<String>,
    pub value: String,
}

pub async fn interact_type(
    State(state): State<Arc<ServerState>>,
    Json(body): Json<TypeBody>,
) -> Response {
    send_cmd(&state, |reply| BrowserCmd::TypeText {
        element_id: body.element_id,
        selector: body.selector,
        value: body.value,
        reply,
    })
    .await
}

#[derive(Deserialize)]
pub struct SubmitBody {
    pub form_selector: String,
    pub fields: HashMap<String, String>,
}

pub async fn interact_submit(
    State(state): State<Arc<ServerState>>,
    Json(body): Json<SubmitBody>,
) -> Response {
    send_cmd(&state, |reply| BrowserCmd::Submit {
        form_selector: body.form_selector,
        fields: body.fields,
        reply,
    })
    .await
}

#[derive(Deserialize)]
pub struct ScrollBody {
    pub direction: String,
}

pub async fn interact_scroll(
    State(state): State<Arc<ServerState>>,
    Json(body): Json<ScrollBody>,
) -> Response {
    send_cmd(&state, |reply| BrowserCmd::Scroll {
        direction: body.direction,
        reply,
    })
    .await
}

pub async fn interact_elements(State(state): State<Arc<ServerState>>) -> Response {
    send_cmd(&state, |reply| BrowserCmd::InteractiveElements { reply }).await
}

// ---------------------------------------------------------------------------
// Network
// ---------------------------------------------------------------------------

pub async fn network_requests(State(state): State<Arc<ServerState>>) -> Response {
    send_cmd(&state, |reply| BrowserCmd::NetworkRequests { reply }).await
}

pub async fn network_requests_clear(State(state): State<Arc<ServerState>>) -> Response {
    send_cmd(&state, |reply| BrowserCmd::ClearNetworkRequests { reply }).await
}

pub async fn network_har(State(state): State<Arc<ServerState>>) -> Response {
    send_cmd(&state, |reply| BrowserCmd::NetworkHar { reply }).await
}

// ---------------------------------------------------------------------------
// Cookies
// ---------------------------------------------------------------------------

pub async fn cookies_list(State(state): State<Arc<ServerState>>) -> Response {
    send_cmd(&state, |reply| BrowserCmd::GetCookies { reply }).await
}

#[derive(Deserialize)]
pub struct SetCookieBody {
    pub name: String,
    pub value: String,
    pub domain: String,
    #[serde(default = "default_path")]
    pub path: String,
}

fn default_path() -> String {
    "/".to_string()
}

pub async fn cookies_set(
    State(state): State<Arc<ServerState>>,
    Json(body): Json<SetCookieBody>,
) -> Response {
    send_cmd(&state, |reply| BrowserCmd::SetCookie {
        name: body.name,
        value: body.value,
        domain: body.domain,
        path: body.path,
        reply,
    })
    .await
}

pub async fn cookies_delete(
    State(state): State<Arc<ServerState>>,
    Path(name): Path<String>,
) -> Response {
    send_cmd(&state, |reply| BrowserCmd::DeleteCookie { name, reply }).await
}

pub async fn cookies_clear(State(state): State<Arc<ServerState>>) -> Response {
    send_cmd(&state, |reply| BrowserCmd::ClearCookies { reply }).await
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

pub async fn health() -> Response {
    Json(serde_json::json!({ "status": "ok" })).into_response()
}
