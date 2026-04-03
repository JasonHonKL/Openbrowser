use async_trait::async_trait;
use serde_json::Value;

use crate::domain::{method_not_found, CdpDomainHandler, DomainContext, HandleResult, TargetEntry};
use crate::error::{CdpError, CdpErrorBody};
use crate::protocol::message::{CdpErrorResponse, CdpEvent};
use crate::protocol::target::CdpSession;

pub struct TargetDomain;

fn invalid_params(msg: &str) -> HandleResult {
    HandleResult::Error(CdpErrorResponse {
        id: 0,
        error: CdpErrorBody::from(&CdpError::InvalidParams(msg.to_string())),
        session_id: None,
    })
}

#[async_trait(?Send)]
impl CdpDomainHandler for TargetDomain {
    fn domain_name(&self) -> &'static str {
        "Target"
    }

    async fn handle(
        &self,
        method: &str,
        params: Value,
        session: &mut CdpSession,
        ctx: &DomainContext,
    ) -> HandleResult {
        match method {
            "setDiscoverTargets" => HandleResult::Ack,
            "createTarget" => {
                let url = params["url"].as_str().unwrap_or("about:blank");
                let target_id = format!("target-{}", uuid::Uuid::new_v4());
                let mut targets = ctx.targets.lock().await;
                targets.insert(target_id.clone(), TargetEntry {
                    url: url.to_string(),
                    html: None,
                    title: None,
                    js_enabled: false,
                    frame_tree_json: None,
                });

                let _ = ctx.event_bus.send(CdpEvent {
                    method: "Target.targetCreated".to_string(),
                    params: serde_json::json!({
                        "targetInfo": {
                            "targetId": target_id,
                            "type": "page",
                            "title": "",
                            "url": url,
                            "attached": false,
                            "browserContextId": "default",
                        }
                    }),
                    session_id: None,
                });

                HandleResult::Success(serde_json::json!({ "targetId": target_id }))
            }
            "attachToTarget" => {
                let target_id = params["targetId"].as_str().unwrap_or("");
                if target_id.is_empty() {
                    return invalid_params("Missing targetId parameter");
                }
                session.target_id = Some(target_id.to_string());

                let _ = ctx.event_bus.send(CdpEvent {
                    method: "Target.attachedToTarget".to_string(),
                    params: serde_json::json!({
                        "sessionId": session.session_id,
                        "targetInfo": {
                            "targetId": target_id,
                            "type": "page",
                            "title": "",
                            "attached": true,
                            "browserContextId": "default",
                        }
                    }),
                    session_id: Some(session.session_id.clone()),
                });

                HandleResult::Success(serde_json::json!({ "sessionId": session.session_id }))
            }
            "detachFromTarget" => {
                let _ = ctx.event_bus.send(CdpEvent {
                    method: "Target.detachedFromTarget".to_string(),
                    params: serde_json::json!({
                        "sessionId": session.session_id,
                    }),
                    session_id: Some(session.session_id.clone()),
                });

                session.target_id = None;
                HandleResult::Ack
            }
            "getTargetInfo" => {
                let target_id = params["targetId"].as_str()
                    .or(session.target_id.as_deref())
                    .unwrap_or("default")
                    .to_string();
                let targets = ctx.targets.lock().await;
                let entry = targets.get(&target_id);
                HandleResult::Success(build_target_info(entry, &target_id))
            }
            "setAutoAttach" => HandleResult::Ack,
            "getTargets" => {
                let targets = ctx.targets.lock().await;
                let target_infos: Vec<Value> = targets.iter().map(|(id, entry)| {
                    serde_json::json!({
                        "targetId": id,
                        "type": "page",
                        "title": entry.title.as_deref().unwrap_or(""),
                        "url": entry.url,
                        "attached": false,
                        "browserContextId": "default",
                    })
                }).collect();
                HandleResult::Success(serde_json::json!({ "targetInfos": target_infos }))
            }
            "createBrowserContext" => {
                HandleResult::Success(serde_json::json!({ "browserContextId": "default" }))
            }
            "disposeBrowserContext" => HandleResult::Ack,
            "closeTarget" => {
                let target_id = params["targetId"].as_str().unwrap_or("");
                if !target_id.is_empty() {
                    let mut targets = ctx.targets.lock().await;
                    if targets.remove(target_id).is_some() {
                        let _ = ctx.event_bus.send(CdpEvent {
                            method: "Target.targetDestroyed".to_string(),
                            params: serde_json::json!({
                                "targetId": target_id,
                            }),
                            session_id: None,
                        });
                        return HandleResult::Success(serde_json::json!({ "success": true }));
                    }
                }
                HandleResult::Success(serde_json::json!({ "success": false }))
            }
            _ => method_not_found("Target", method),
        }
    }
}

fn build_target_info(entry: Option<&TargetEntry>, target_id: &str) -> Value {
    if let Some(e) = entry {
        serde_json::json!({
            "targetInfo": {
                "targetId": target_id,
                "type": "page",
                "title": e.title.as_deref().unwrap_or(""),
                "url": e.url,
                "attached": false,
                "browserContextId": "default",
                "canAccessOpener": false,
            }
        })
    } else {
        serde_json::json!({
            "targetInfo": {
                "targetId": target_id,
                "type": "page",
                "title": "",
                "url": "",
                "attached": false,
            }
        })
    }
}
