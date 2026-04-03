use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{broadcast, mpsc, oneshot};

use pardus_core::{
    Browser, BrowserConfig, CookieEntry, ElementHandle, FormState, PageSnapshot, ScrollDirection,
};
use pardus_debug::NetworkRecord;

use crate::events::ServerEvent;

/// Serializable data returned from browser commands.
#[derive(Debug)]
pub enum BrowserResponse {
    Ok { ok: bool },
    PageSnapshot(PageSnapshot),
    Html { html: String },
    Tabs { tabs: Vec<serde_json::Value> },
    TabId { id: u64 },
    SemanticTree(serde_json::Value),
    Element(Option<serde_json::Value>),
    Stats(serde_json::Value),
    InteractiveElements { elements: Vec<ElementHandle> },
    NetworkRecords { requests: Vec<NetworkRecord> },
    Har(serde_json::Value),
    Cookies { cookies: Vec<CookieEntry> },
}

/// Commands sent from HTTP handlers to the browser task.
pub enum BrowserCmd {
    Navigate {
        url: String,
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    Reload {
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    CurrentPage {
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    Html {
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    ListTabs {
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    OpenTab {
        url: String,
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    CloseTab {
        id: u64,
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    ActivateTab {
        id: u64,
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    SemanticTree {
        flat: bool,
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    SemanticElement {
        id: usize,
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    SemanticStats {
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    Click {
        element_id: Option<usize>,
        selector: Option<String>,
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    TypeText {
        element_id: Option<usize>,
        selector: Option<String>,
        value: String,
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    Submit {
        form_selector: String,
        fields: HashMap<String, String>,
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    Scroll {
        direction: String,
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    InteractiveElements {
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    NetworkRequests {
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    ClearNetworkRequests {
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    NetworkHar {
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    GetCookies {
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    SetCookie {
        name: String,
        value: String,
        domain: String,
        path: String,
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    DeleteCookie {
        name: String,
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
    ClearCookies {
        reply: oneshot::Sender<Result<BrowserResponse>>,
    },
}

/// Shared server state -- fully Send + Sync.
pub struct ServerState {
    pub cmd_tx: mpsc::Sender<BrowserCmd>,
    pub event_tx: broadcast::Sender<ServerEvent>,
}

/// Create the server state by spawning the browser task on a dedicated thread.
pub fn create_state() -> Result<Arc<ServerState>> {
    let (cmd_tx, cmd_rx) = mpsc::channel(256);
    let (event_tx, _) = broadcast::channel(128);

    let thread_event_tx = event_tx.clone();
    std::thread::spawn(move || {
        browser_task(cmd_rx, thread_event_tx);
    });

    Ok(Arc::new(ServerState { cmd_tx, event_tx }))
}

/// The browser task runs on a dedicated OS thread with its own single-threaded
/// tokio runtime. The Browser is !Send, so it must stay on one thread.
fn browser_task(cmd_rx: mpsc::Receiver<BrowserCmd>, event_tx: broadcast::Sender<ServerEvent>) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build browser task runtime");

    rt.block_on(async move {
        let mut browser = match Browser::new(BrowserConfig::default()) {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("Failed to create Browser: {}", e);
                return;
            }
        };

        tracing::info!("Browser task started");

        let mut cmd_rx = cmd_rx;

        while let Some(cmd) = cmd_rx.recv().await {
            handle_cmd(&mut browser, &event_tx, cmd).await;
        }

        tracing::info!("Browser task shutting down");
    });
}

async fn handle_cmd(
    browser: &mut Browser,
    event_tx: &broadcast::Sender<ServerEvent>,
    cmd: BrowserCmd,
) {
    match cmd {
        BrowserCmd::Navigate { url, reply } => {
            let _ = event_tx.send(ServerEvent::NavigationStarted {
                tab_id: browser.active_tab().map(|t| t.id.as_u64()).unwrap_or(0),
                url: url.clone(),
            });
            match browser.navigate(&url).await {
                Ok(_) => {
                    let resp = match browser.current_page() {
                        Some(p) => BrowserResponse::PageSnapshot(p.snapshot()),
                        None => BrowserResponse::Ok { ok: true },
                    };
                    let tab_id = browser.active_tab().map(|t| t.id.as_u64()).unwrap_or(0);
                    let status = browser.current_page().map(|p| p.status).unwrap_or(0);
                    let _ = event_tx.send(ServerEvent::NavigationCompleted {
                        tab_id,
                        status,
                        url,
                    });
                    let _ = reply.send(Ok(resp));
                }
                Err(e) => {
                    let tab_id = browser.active_tab().map(|t| t.id.as_u64()).unwrap_or(0);
                    let _ = event_tx.send(ServerEvent::NavigationFailed {
                        tab_id,
                        error: e.to_string(),
                    });
                    let _ = reply.send(Err(e));
                }
            }
        }
        BrowserCmd::Reload { reply } => {
            let result = browser.reload().await;
            match result {
                Ok(_) => {
                    let resp = match browser.current_page() {
                        Some(p) => BrowserResponse::PageSnapshot(p.snapshot()),
                        None => BrowserResponse::Ok { ok: true },
                    };
                    let _ = reply.send(Ok(resp));
                    let _ = event_tx.send(ServerEvent::Reloaded);
                }
                Err(e) => {
                    let _ = reply.send(Err(e));
                }
            }
        }
        BrowserCmd::CurrentPage { reply } => {
            match browser.current_page() {
                Some(p) => {
                    let _ = reply.send(Ok(BrowserResponse::PageSnapshot(p.snapshot())));
                }
                None => {
                    let _ = reply.send(Err(anyhow::anyhow!("No page loaded")));
                }
            }
        }
        BrowserCmd::Html { reply } => {
            match browser.current_page() {
                Some(p) => {
                    let _ = reply.send(Ok(BrowserResponse::Html {
                        html: p.html.html(),
                    }));
                }
                None => {
                    let _ = reply.send(Err(anyhow::anyhow!("No page loaded")));
                }
            }
        }
        BrowserCmd::ListTabs { reply } => {
            let tabs: Vec<serde_json::Value> = browser
                .list_tabs()
                .iter()
                .map(|t| serde_json::to_value(t.info()).unwrap_or_default())
                .collect();
            let _ = reply.send(Ok(BrowserResponse::Tabs { tabs }));
        }
        BrowserCmd::OpenTab { url, reply } => {
            match browser.open_tab(&url).await {
                Ok(tab) => {
                    let id = tab.id.as_u64();
                    let _ = event_tx.send(ServerEvent::TabOpened {
                        url: url.clone(),
                    });
                    let _ = reply.send(Ok(BrowserResponse::TabId { id }));
                }
                Err(e) => {
                    let _ = reply.send(Err(e));
                }
            }
        }
        BrowserCmd::CloseTab { id, reply } => {
            let tab_id = pardus_core::TabId::from_u64(id);
            browser.close_tab(tab_id);
            let _ = event_tx.send(ServerEvent::TabClosed { id });
            let _ = reply.send(Ok(BrowserResponse::Ok { ok: true }));
        }
        BrowserCmd::ActivateTab { id, reply } => {
            let tab_id = pardus_core::TabId::from_u64(id);
            match browser.switch_to(tab_id).await {
                Ok(tab) => {
                    let _ = event_tx.send(ServerEvent::TabActivated { id });
                    let _ = reply.send(Ok(BrowserResponse::TabId {
                        id: tab.id.as_u64(),
                    }));
                }
                Err(e) => {
                    let _ = reply.send(Err(e));
                }
            }
        }
        BrowserCmd::SemanticTree { flat, reply } => {
            match browser.current_page() {
                Some(p) => {
                    let tree = p.semantic_tree();
                    if flat {
                        let nodes = collect_interactive_flat(&tree.root);
                        let _ = reply.send(Ok(BrowserResponse::SemanticTree(
                            serde_json::to_value(nodes).unwrap_or_default(),
                        )));
                    } else {
                        let _ = reply.send(Ok(BrowserResponse::SemanticTree(
                            serde_json::to_value(&tree).unwrap_or_default(),
                        )));
                    }
                }
                None => {
                    let _ = reply.send(Err(anyhow::anyhow!("No page loaded")));
                }
            }
        }
        BrowserCmd::SemanticElement { id, reply } => {
            match browser.current_page() {
                Some(p) => {
                    let tree = p.semantic_tree();
                    match find_element(&tree.root, id) {
                        Some(node) => {
                            let _ = reply.send(Ok(BrowserResponse::Element(Some(
                                serde_json::to_value(node).unwrap_or_default(),
                            ))));
                        }
                        None => {
                            let _ = reply.send(Ok(BrowserResponse::Element(None)));
                        }
                    }
                }
                None => {
                    let _ = reply.send(Err(anyhow::anyhow!("No page loaded")));
                }
            }
        }
        BrowserCmd::SemanticStats { reply } => {
            match browser.current_page() {
                Some(p) => {
                    let tree = p.semantic_tree();
                    let _ = reply.send(Ok(BrowserResponse::Stats(
                        serde_json::to_value(&tree.stats).unwrap_or_default(),
                    )));
                }
                None => {
                    let _ = reply.send(Err(anyhow::anyhow!("No page loaded")));
                }
            }
        }
        BrowserCmd::Click {
            element_id,
            selector,
            reply,
        } => {
            let result = if let Some(id) = element_id {
                browser.click_by_id(id).await
            } else if let Some(ref sel) = selector {
                browser.click(sel).await
            } else {
                Err(anyhow::anyhow!("Must provide element_id or selector"))
            };
            let _ = reply.send(result.map(|_| BrowserResponse::Ok { ok: true }));
        }
        BrowserCmd::TypeText {
            element_id,
            selector,
            value,
            reply,
        } => {
            let result = if let Some(id) = element_id {
                browser.type_by_id(id, &value).await
            } else if let Some(ref sel) = selector {
                browser.type_text(sel, &value).await
            } else {
                Err(anyhow::anyhow!("Must provide element_id or selector"))
            };
            let _ = reply.send(result.map(|_| BrowserResponse::Ok { ok: true }));
        }
        BrowserCmd::Submit {
            form_selector,
            fields,
            reply,
        } => {
            let mut fs = FormState::new();
            for (k, v) in &fields {
                fs.set(k, v);
            }
            let result = browser.submit(&form_selector, &fs).await;
            let _ = reply.send(result.map(|_| BrowserResponse::Ok { ok: true }));
        }
        BrowserCmd::Scroll { direction, reply } => {
            let dir = match direction.to_lowercase().as_str() {
                "up" => ScrollDirection::Up,
                _ => ScrollDirection::Down,
            };
            let result = browser.scroll(dir).await;
            let _ = reply.send(result.map(|_| BrowserResponse::Ok { ok: true }));
        }
        BrowserCmd::InteractiveElements { reply } => {
            match browser.current_page() {
                Some(p) => {
                    let _ = reply.send(Ok(BrowserResponse::InteractiveElements {
                        elements: p.interactive_elements(),
                    }));
                }
                None => {
                    let _ = reply.send(Err(anyhow::anyhow!("No page loaded")));
                }
            }
        }
        BrowserCmd::NetworkRequests { reply } => {
            let log = browser.network_log.lock().unwrap();
            let records = log.records.clone();
            let _ = reply.send(Ok(BrowserResponse::NetworkRecords {
                requests: records,
            }));
        }
        BrowserCmd::ClearNetworkRequests { reply } => {
            let mut log = browser.network_log.lock().unwrap();
            log.records.clear();
            let _ = reply.send(Ok(BrowserResponse::Ok { ok: true }));
        }
        BrowserCmd::NetworkHar { reply } => {
            let log = browser.network_log.lock().unwrap();
            let har = pardus_debug::har::HarFile::from_network_log(&log);
            let val = serde_json::to_value(&har).unwrap_or_default();
            let _ = reply.send(Ok(BrowserResponse::Har(val)));
        }
        BrowserCmd::GetCookies { reply } => {
            let cookies = browser.all_cookies();
            let _ = reply.send(Ok(BrowserResponse::Cookies { cookies }));
        }
        BrowserCmd::SetCookie {
            name,
            value,
            domain,
            path,
            reply,
        } => {
            browser.set_cookie(&name, &value, &domain, &path);
            let _ = reply.send(Ok(BrowserResponse::Ok { ok: true }));
        }
        BrowserCmd::DeleteCookie { name, reply } => {
            browser.delete_cookie(&name, "", "");
            let _ = reply.send(Ok(BrowserResponse::Ok { ok: true }));
        }
        BrowserCmd::ClearCookies { reply } => {
            browser.clear_cookies();
            let _ = reply.send(Ok(BrowserResponse::Ok { ok: true }));
        }
    }
}

/// Collect all interactive nodes as flat JSON values.
fn collect_interactive_flat(node: &pardus_core::SemanticNode) -> Vec<serde_json::Value> {
    let mut result = Vec::new();
    if node.is_interactive {
        result.push(serde_json::to_value(node).unwrap_or_default());
    }
    for child in &node.children {
        result.extend(collect_interactive_flat(child));
    }
    result
}

/// Find a semantic node by element_id.
fn find_element<'a>(
    node: &'a pardus_core::SemanticNode,
    id: usize,
) -> Option<&'a pardus_core::SemanticNode> {
    if node.element_id == Some(id) {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_element(child, id) {
            return Some(found);
        }
    }
    None
}
