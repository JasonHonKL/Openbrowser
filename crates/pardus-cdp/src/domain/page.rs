use async_trait::async_trait;
use serde_json::Value;

use crate::domain::{method_not_found, CdpDomainHandler, DomainContext, HandleResult};
use crate::error::{CdpError, CdpErrorBody};
use crate::protocol::message::{CdpErrorResponse, CdpEvent};
use crate::protocol::target::CdpSession;

pub struct PageDomain;

fn invalid_params(msg: &str) -> HandleResult {
    HandleResult::Error(CdpErrorResponse {
        id: 0,
        error: CdpErrorBody::from(&CdpError::InvalidParams(msg.to_string())),
        session_id: None,
    })
}

fn server_error(msg: impl std::fmt::Display) -> HandleResult {
    HandleResult::Error(CdpErrorResponse {
        id: 0,
        error: CdpErrorBody::from(&CdpError::ServerError(msg.to_string())),
        session_id: None,
    })
}

fn now_timestamp() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        * 1000.0
}

fn resolve_target_id(session: &CdpSession) -> &str {
    session.target_id.as_deref().unwrap_or("default")
}

#[async_trait(?Send)]
impl CdpDomainHandler for PageDomain {
    fn domain_name(&self) -> &'static str {
        "Page"
    }

    async fn handle(
        &self,
        method: &str,
        params: Value,
        session: &mut CdpSession,
        ctx: &DomainContext,
    ) -> HandleResult {
        match method {
            "enable" => {
                session.enable_domain("Page");
                HandleResult::Ack
            }
            "disable" => {
                session.disable_domain("Page");
                HandleResult::Ack
            }
            "navigate" => {
                let url = params["url"].as_str().unwrap_or("");
                if url.is_empty() {
                    return invalid_params("Missing url parameter");
                }
                let _transition_type = params["transitionType"].as_str();
                let _frame_id = params["frameId"].as_str();
                let target_id = resolve_target_id(session).to_string();

                match ctx.navigate(&target_id, url).await {
                    Ok(()) => {
                        let final_url = ctx.get_url(&target_id).await.unwrap_or_else(|| url.to_string());
                        let title = ctx.get_title(&target_id).await;

                        let _ = ctx.event_bus.send(CdpEvent {
                            method: "Page.frameNavigated".to_string(),
                            params: serde_json::json!({
                                "frame": {
                                    "id": target_id,
                                    "loaderId": target_id,
                                    "url": final_url,
                                    "mimeType": "text/html",
                                    "securityOrigin": final_url.strip_prefix("https://").unwrap_or(&final_url).strip_prefix("http://").unwrap_or(&final_url),
                                }
                            }),
                            session_id: Some(session.session_id.clone()),
                        });
                        let _ = ctx.event_bus.send(CdpEvent {
                            method: "Page.domContentEventFired".to_string(),
                            params: serde_json::json!({ "timestamp": now_timestamp() }),
                            session_id: Some(session.session_id.clone()),
                        });
                        let _ = ctx.event_bus.send(CdpEvent {
                            method: "Page.loadEventFired".to_string(),
                            params: serde_json::json!({ "timestamp": now_timestamp() }),
                            session_id: Some(session.session_id.clone()),
                        });

                        let mut result = serde_json::json!({
                            "frameId": target_id,
                            "loaderId": target_id,
                        });
                        if let Some(t) = title {
                            result["navigationHistoryEntry"] = serde_json::json!({
                                "id": format!("nav-{}", uuid::Uuid::new_v4()),
                                "url": final_url,
                                "title": t,
                                "documentSequence": 1,
                            });
                        }

                        HandleResult::Success(result)
                    }
                    Err(e) => server_error(e),
                }
            }
            "reload" => {
                let target_id = resolve_target_id(session).to_string();
                let url = {
                    let targets = ctx.targets.lock().await;
                    targets.get(&target_id).map(|t| t.url.clone()).unwrap_or_else(|| "about:blank".to_string())
                };
                match ctx.navigate(&target_id, &url).await {
                    Ok(()) => {
                        let _ = ctx.event_bus.send(CdpEvent {
                            method: "Page.frameNavigated".to_string(),
                            params: serde_json::json!({
                                "frame": { "id": target_id, "url": url, "mimeType": "text/html" }
                            }),
                            session_id: Some(session.session_id.clone()),
                        });
                        let _ = ctx.event_bus.send(CdpEvent {
                            method: "Page.loadEventFired".to_string(),
                            params: serde_json::json!({ "timestamp": now_timestamp() }),
                            session_id: Some(session.session_id.clone()),
                        });
                        HandleResult::Ack
                    }
                    Err(e) => server_error(e),
                }
            }
            "goBack" => {
                let target_id = resolve_target_id(session).to_string();
                let mut targets = ctx.targets.lock().await;
                if let Some(entry) = targets.get_mut(&target_id) {
                    entry.js_enabled = true;
                }
                HandleResult::Ack
            }
            "goForward" => {
                let target_id = resolve_target_id(session).to_string();
                let mut targets = ctx.targets.lock().await;
                if let Some(entry) = targets.get_mut(&target_id) {
                    entry.js_enabled = true;
                }
                HandleResult::Ack
            }
            "getFrameTree" => {
                let target_id = resolve_target_id(session).to_string();
                let targets = ctx.targets.lock().await;
                let entry = targets.get(&target_id);
                let (frame_id, url, _title) = entry
                    .map(|t| (target_id.clone(), t.url.clone(), t.title.clone().unwrap_or_default()))
                    .unwrap_or_else(|| ("main".to_string(), "about:blank".to_string(), String::new()));
                let frame_tree_json = entry.and_then(|e| e.frame_tree_json.clone());

                let child_frames = if let Some(json_str) = &frame_tree_json {
                    parse_child_frames(json_str)
                } else {
                    Vec::new()
                };

                HandleResult::Success(serde_json::json!({
                    "frameTree": {
                        "frame": {
                            "id": frame_id,
                            "loaderId": frame_id,
                            "url": url,
                            "mimeType": "text/html",
                            "securityOrigin": "",
                            "unreachableUrl": Value::Null,
                        },
                        "childFrames": child_frames,
                    }
                }))
            }
            "addScriptToEvaluateOnNewDocument" => {
                let _source = params["source"].as_str().unwrap_or("");
                let world_name = params["worldName"].as_str().unwrap_or("");
                let include_command_line_api = params["includeCommandLineAPI"].as_bool().unwrap_or(false);
                let _ = (world_name, include_command_line_api);

                HandleResult::Success(serde_json::json!({
                    "identifier": format!("script-{}", uuid::Uuid::new_v4()),
                }))
            }
            "removeScriptToEvaluateOnNewDocument" => {
                let _identifier = params["identifier"].as_str().unwrap_or("");
                HandleResult::Ack
            }
            "setBypassCachingEnabled" => {
                let _bypass = params["enabled"].as_bool().unwrap_or(false);
                HandleResult::Ack
            }
            "getResourceTree" => {
                let target_id = resolve_target_id(session).to_string();
                let targets = ctx.targets.lock().await;
                let entry = targets.get(&target_id);
                let (url, _title) = entry
                    .map(|t| (t.url.clone(), t.title.clone().unwrap_or_default()))
                    .unwrap_or_else(|| ("about:blank".to_string(), String::new()));
                let frame_tree_json = entry.and_then(|e| e.frame_tree_json.clone());
                let child_frames = if let Some(json_str) = &frame_tree_json {
                    parse_child_frames(json_str)
                } else {
                    Vec::new()
                };
                HandleResult::Success(serde_json::json!({
                    "frameTree": {
                        "frame": {
                            "id": target_id,
                            "loaderId": target_id,
                            "url": url,
                            "mimeType": "text/html",
                            "securityOrigin": "",
                        },
                        "resources": [],
                        "childFrames": child_frames,
                    }
                }))
            }
            "getResourceContent" => {
                let _frame_id = params["frameId"].as_str().unwrap_or("");
                let url = params["url"].as_str().unwrap_or("");

                let html = ctx.get_html(resolve_target_id(session)).await
                    .unwrap_or_default();
                let content = if url == ctx.get_url(resolve_target_id(session)).await.unwrap_or_default() {
                    html
                } else {
                    String::new()
                };

                HandleResult::Success(serde_json::json!({
                    "content": content,
                    "base64Encoded": false,
                }))
            }
            "captureScreenshot" => {
                #[cfg(feature = "screenshot")]
                {
                    let target_id = resolve_target_id(session).to_string();
                    let url = match ctx.get_url(&target_id).await {
                        Some(u) => u,
                        None => return server_error("No page loaded — navigate to a URL first"),
                    };

                    let format_str = params["format"].as_str().unwrap_or("png");
                    let quality = params["quality"].as_u64().map(|q| q as u8);
                    let has_clip = !params["clip"].is_null();
                    let full_page = params["captureBeyondViewport"].as_bool()
                        .unwrap_or(has_clip);

                    let screenshot_format = match format_str {
                        "jpeg" => {
                            pardus_core::screenshot::ScreenshotFormat::Jpeg {
                                quality: quality.unwrap_or(85),
                            }
                        }
                        _ => pardus_core::screenshot::ScreenshotFormat::Png,
                    };

                    let opts = pardus_core::screenshot::ScreenshotOptions {
                        viewport_width: 1280,
                        viewport_height: 720,
                        format: screenshot_format,
                        full_page,
                        timeout_ms: 10_000,
                    };

                    match ctx.screenshot_handle.capture_page(&url, &opts).await {
                        Ok(bytes) => {
                            use base64::Engine;
                            let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
                            HandleResult::Success(serde_json::json!({
                                "data": encoded,
                                "metadata": {
                                    "pageWidth": opts.viewport_width,
                                    "pageHeight": opts.viewport_height,
                                }
                            }))
                        }
                        Err(e) => server_error(format!("Screenshot capture failed: {}", e)),
                    }
                }
                #[cfg(not(feature = "screenshot"))]
                {
                    HandleResult::Error(CdpErrorResponse {
                        id: 0,
                        error: CdpErrorBody {
                            code: crate::error::SERVER_ERROR,
                            message: "Screenshots not supported. PardusBrowser is a semantic-only browser (no rendering engine). Rebuild with --features screenshot to enable.".to_string(),
                        },
                        session_id: None,
                    })
                }
            }
            "printToPDF" => {
                HandleResult::Error(CdpErrorResponse {
                    id: 0,
                    error: CdpErrorBody {
                        code: crate::error::SERVER_ERROR,
                        message: "PDF generation not supported. PardusBrowser is a semantic-only browser (no rendering engine).".to_string(),
                    },
                    session_id: None,
                })
            }
            "startScreencast" => HandleResult::Ack,
            "stopScreencast" => HandleResult::Ack,
            "screencastFrameAck" => HandleResult::Ack,
            "bringToFront" => HandleResult::Ack,
            "setDownloadBehavior" => {
                let _behavior = params["behavior"].as_str().unwrap_or("deny");
                HandleResult::Ack
            }
            "getFileChooser" => {
                HandleResult::Error(CdpErrorResponse {
                    id: 0,
                    error: CdpErrorBody {
                        code: crate::error::SERVER_ERROR,
                        message: "File chooser not supported".to_string(),
                    },
                    session_id: None,
                })
            }
            "getInstallabilityError" => HandleResult::Success(serde_json::json!({ "installabilityErrors": [] })),
            "getAppManifest" => {
                HandleResult::Success(serde_json::json!({
                    "url": Value::Null,
                    "errors": [],
                }))
            }
            "getOriginTrialInfo" => HandleResult::Success(serde_json::json!({ "origins": [] })),
            "setInterceptFileChooserDialog" => HandleResult::Ack,
            "toggleInterceptFileChooserDialog" => HandleResult::Ack,
            "stopLoading" => HandleResult::Ack,
            "close" => HandleResult::Ack,
            "setAutoAttachToCreatedPages" => HandleResult::Ack,
            "generateTestReport" => HandleResult::Ack,
            "resetNavigationHistory" => HandleResult::Ack,
            "createIsolatedWorld" => {
                let frame_id = params["frameId"].as_str().unwrap_or("main");
                let _world_name = params["worldName"].as_str().unwrap_or("");
                let _grant_univeral_access = params["grantUniveralAccess"].as_bool().unwrap_or(false);
                let ctx_id = session.create_execution_context(frame_id.to_string(), "".to_string());
                HandleResult::Success(serde_json::json!({
                    "executionContextId": ctx_id,
                }))
            }
            "addCompilationCache" => HandleResult::Ack,
            "clearCompilationCache" => HandleResult::Ack,
            "setViewportSize" => {
                let _width = params["width"].as_u64().unwrap_or(1280);
                let _height = params["height"].as_u64().unwrap_or(720);
                HandleResult::Ack
            }
            "getFrameResource" => {
                HandleResult::Success(serde_json::json!({
                    "content": "",
                    "mimeType": "",
                    "statusCode": 200,
                }))
            }
            "getFrameResourceTree" => {
                let target_id = resolve_target_id(session).to_string();
                let targets = ctx.targets.lock().await;
                let entry = targets.get(&target_id);
                let url = entry.map(|t| t.url.clone()).unwrap_or_default();
                let frame_tree_json = entry.and_then(|e| e.frame_tree_json.clone());
                let child_frames = if let Some(json_str) = &frame_tree_json {
                    parse_child_frames(json_str)
                } else {
                    Vec::new()
                };
                HandleResult::Success(serde_json::json!({
                    "frameTree": {
                        "frame": {
                            "id": target_id,
                            "url": url,
                            "mimeType": "text/html",
                        },
                        "resources": [],
                        "childFrames": child_frames,
                    }
                }))
            }
            "searchInResource" => {
                HandleResult::Success(serde_json::json!({ "result": [] }))
            }
            "setWebLifecycleState" => HandleResult::Ack,
            "enableLifecycleEvents" => HandleResult::Ack,
            "setPrerenderingAllowed" => HandleResult::Ack,
            "getBackForwardCache" => HandleResult::Success(serde_json::json!({ "prerenderInfo": [] })),
            "registerNonTrackedLoadEventFired" => HandleResult::Ack,
            "attemptNavigation" => HandleResult::Ack,
            _ => method_not_found("Page", method),
        }
    }
}

fn frame_data_to_cdp(frame: &serde_json::Value) -> serde_json::Value {
    let id = frame["id"].as_str().unwrap_or_default();
    let url = frame["url"].as_str().unwrap_or_default();
    let has_error = frame.get("load_error").and_then(|e| e.as_str()).map_or(false, |s| !s.is_empty());
    let child_frames: Vec<Value> = frame.get("child_frames")
        .and_then(|c| c.as_array())
        .map(|arr| arr.iter().map(|f| frame_data_to_cdp(f)).collect())
        .unwrap_or_default();

    let mut obj = serde_json::json!({
        "id": id,
        "loaderId": id,
        "url": url,
        "mimeType": "text/html",
        "securityOrigin": "",
        "unreachableUrl": Value::Null,
    });
    if has_error {
        obj["unreachableUrl"] = Value::String(url.to_string());
    }
    if !child_frames.is_empty() {
        obj["childFrames"] = Value::Array(child_frames);
    }
    obj
}

fn parse_child_frames(frame_tree_json: &str) -> Vec<Value> {
    let tree: Value = match serde_json::from_str(frame_tree_json) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let root = match tree.get("root") {
        Some(r) => r,
        None => return Vec::new(),
    };
    let child_frames = match root.get("child_frames").and_then(|c| c.as_array()) {
        Some(arr) => arr,
        None => return Vec::new(),
    };
    child_frames.iter().map(|f| frame_data_to_cdp(f)).collect()
}
