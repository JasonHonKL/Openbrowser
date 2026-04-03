use async_trait::async_trait;
use serde_json::Value;

use crate::domain::{method_not_found, CdpDomainHandler, DomainContext, HandleResult};
use crate::protocol::message::CdpEvent;
use crate::protocol::target::CdpSession;

pub struct RuntimeDomain;

fn resolve_target_id(session: &CdpSession) -> &str {
    session.target_id.as_deref().unwrap_or("default")
}

#[allow(dead_code)]
fn _now_timestamp() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        * 1000.0
}

#[async_trait(?Send)]
impl CdpDomainHandler for RuntimeDomain {
    fn domain_name(&self) -> &'static str {
        "Runtime"
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
                session.enable_domain("Runtime");
                let target_id = resolve_target_id(session);
                let origin = {
                    let targets = ctx.targets.lock().await;
                    targets.get(target_id).map(|t| t.url.clone()).unwrap_or_default()
                };
                let ctx_id = session.create_execution_context(origin, "".to_string());
                let _ = ctx.event_bus.send(CdpEvent {
                    method: "Runtime.executionContextCreated".to_string(),
                    params: serde_json::json!({
                        "context": {
                            "id": ctx_id,
                            "origin": "",
                            "name": "",
                            "auxData": { "isDefault": true, "type": "default" }
                        }
                    }),
                    session_id: Some(session.session_id.clone()),
                });
                HandleResult::Ack
            }
            "disable" => {
                for ec in &session.execution_contexts {
                    let _ = ctx.event_bus.send(CdpEvent {
                        method: "Runtime.executionContextDestroyed".to_string(),
                        params: serde_json::json!({ "executionContextId": ec.id }),
                        session_id: Some(session.session_id.clone()),
                    });
                }
                session.execution_contexts.clear();
                session.disable_domain("Runtime");
                HandleResult::Ack
            }
            "evaluate" => {
                let expression = params["expression"].as_str().unwrap_or("");
                let await_promise = params["returnByValue"].as_bool().unwrap_or(false);
                let _context_id = params["contextId"].as_u64();

                if expression.is_empty() {
                    return HandleResult::Success(serde_json::json!({
                        "result": { "type": "undefined" }
                    }));
                }

                let target_id = resolve_target_id(session).to_string();
                let (html, url, js_enabled) = {
                    let targets = ctx.targets.lock().await;
                    match targets.get(&target_id) {
                        Some(entry) => (
                            entry.html.clone().unwrap_or_default(),
                            entry.url.clone(),
                            entry.js_enabled,
                        ),
                        None => (String::new(), String::new(), false),
                    }
                };

                if !js_enabled {
                    return HandleResult::Success(serde_json::json!({
                        "result": {
                            "type": "undefined",
                            "description": "JS execution not enabled for this target"
                        }
                    }));
                }

                if html.is_empty() {
                    return HandleResult::Success(serde_json::json!({
                        "result": { "type": "undefined" }
                    }));
                }

                // Stub: JS evaluation not yet exposed via Browser API
                // Create a Browser instance and use its JS functionality if needed
                let _ = (html, url, expression, await_promise);
                let mut result = serde_json::json!({
                    "result": {
                        "type": "string",
                        "value": "[JS evaluation via Browser API coming soon]",
                    }
                });
                result["result"]["description"] = Value::String("JavaScript execution not yet fully integrated with CDP".to_string());

                HandleResult::Success(result)
            }
            "callFunctionOn" => {
                let function_declaration = params["functionDeclaration"].as_str().unwrap_or("");
                let _object_id = params["objectId"].as_str();
                let _arguments = params.get("arguments").and_then(|a| a.as_array());
                let await_promise = params["awaitPromise"].as_bool().unwrap_or(false);
                let _return_by_value = params["returnByValue"].as_bool().unwrap_or(false);

                if function_declaration.is_empty() {
                    return HandleResult::Success(serde_json::json!({
                        "result": { "type": "undefined" }
                    }));
                }

                let target_id = resolve_target_id(session).to_string();
                let (html, url, js_enabled) = {
                    let targets = ctx.targets.lock().await;
                    match targets.get(&target_id) {
                        Some(entry) => (
                            entry.html.clone().unwrap_or_default(),
                            entry.url.clone(),
                            entry.js_enabled,
                        ),
                        None => (String::new(), String::new(), false),
                    }
                };

                if !js_enabled || html.is_empty() {
                    return HandleResult::Success(serde_json::json!({
                        "result": { "type": "undefined" }
                    }));
                }

                let args_json = _arguments
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|a| {
                                let v = a.get("value")?;
                                Some(v.to_string())
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();

                let _ = (function_declaration, args_json, await_promise, html, url);
                
                // Stub: JS evaluation not yet exposed via Browser API
                let mut result = serde_json::json!({
                    "result": {
                        "type": "string",
                        "value": "[JS evaluation via Browser API coming soon]",
                    }
                });
                result["result"]["description"] = Value::String("JavaScript execution not yet fully integrated with CDP".to_string());

                HandleResult::Success(result)
            }
            "getProperties" => {
                let _object_id = params["objectId"].as_str().unwrap_or("");
                let _own_properties = params["ownProperties"].as_bool().unwrap_or(true);

                HandleResult::Success(serde_json::json!({
                    "result": [],
                    "internalProperties": []
                }))
            }
            "releaseObject" => HandleResult::Ack,
            "releaseObjectGroup" => HandleResult::Ack,
            "addBinding" => {
                let _name = params["name"].as_str().unwrap_or("");
                HandleResult::Ack
            }
            "removeBinding" => {
                let _name = params["name"].as_str().unwrap_or("");
                HandleResult::Ack
            }
            "runIfWaitingForDebugger" => HandleResult::Ack,
            "setAsyncCallStackDepth" => HandleResult::Ack,
            "setCustomObjectFormatterEnabled" => HandleResult::Ack,
            "discardConsoleEntries" => HandleResult::Ack,
            "compileScript" => {
                let _expression = params["expression"].as_str().unwrap_or("");
                HandleResult::Success(serde_json::json!({
                    "scriptId": format!("script-{}", uuid::Uuid::new_v4()),
                    "exceptionDetails": Value::Null,
                }))
            }
            _ => method_not_found("Runtime", method),
        }
    }
}
