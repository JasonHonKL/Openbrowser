use async_trait::async_trait;
use serde_json::Value;

use crate::domain::{method_not_found, CdpDomainHandler, DomainContext, HandleResult};
use crate::error::SERVER_ERROR;
use crate::protocol::message::CdpErrorResponse;
use crate::protocol::target::CdpSession;

pub struct PardusDomain;

fn resolve_target_id(session: &CdpSession) -> &str {
    session.target_id.as_deref().unwrap_or("default")
}

async fn get_page_data(ctx: &DomainContext, target_id: &str) -> Option<(String, String)> {
    let html = ctx.get_html(target_id).await?;
    let url = ctx.get_url(target_id).await.unwrap_or_default();
    Some((html, url))
}

#[async_trait(?Send)]
impl CdpDomainHandler for PardusDomain {
    fn domain_name(&self) -> &'static str {
        "Pardus"
    }

    async fn handle(
        &self,
        method: &str,
        params: Value,
        session: &mut CdpSession,
        ctx: &DomainContext,
    ) -> HandleResult {
        let target_id = resolve_target_id(session);

        match method {
            "enable" => {
                session.enable_domain("Pardus");
                HandleResult::Ack
            }
            "disable" => {
                session.disable_domain("Pardus");
                HandleResult::Ack
            }
            "semanticTree" => {
                match get_page_data(ctx, target_id).await {
                    Some((html_str, url)) => {
                        let frame_tree_json = ctx.get_frame_tree_json(target_id).await;
                        let page = if let Some(ft_json) = frame_tree_json {
                            match serde_json::from_str::<pardus_core::FrameTree>(&ft_json) {
                                Ok(ft) => pardus_core::Page::from_html_with_frame_tree(&html_str, &url, ft),
                                Err(_) => pardus_core::Page::from_html(&html_str, &url),
                            }
                        } else {
                            pardus_core::Page::from_html(&html_str, &url)
                        };
                        let tree = page.semantic_tree();
                        let result = serde_json::to_value(&tree).unwrap_or(serde_json::json!({
                            "error": "Failed to serialize semantic tree"
                        }));
                        HandleResult::Success(serde_json::json!({
                            "semanticTree": result
                        }))
                    }
                    None => HandleResult::Error(CdpErrorResponse {
                        id: 0,
                        error: crate::error::CdpErrorBody {
                            code: SERVER_ERROR,
                            message: "No active page".to_string(),
                        },
                        session_id: None,
                    }),
                }
            }
            "interact" => {
                let action = params["action"].as_str().unwrap_or("").to_string();
                let selector = params["selector"].as_str().unwrap_or("").to_string();
                let value = params["value"].as_str().unwrap_or("").to_string();
                let fields_param = params.get("fields").cloned();

                let result = handle_interact(&action, &selector, &value, target_id, &fields_param, ctx).await;
                HandleResult::Success(result)
            }
            "getNavigationGraph" => {
                match get_page_data(ctx, target_id).await {
                    Some((html_str, url)) => {
                        let page = pardus_core::Page::from_html(&html_str, &url);
                        let graph = page.navigation_graph();
                        let result = serde_json::to_value(&graph).unwrap_or(serde_json::json!({
                            "error": "Failed to serialize navigation graph"
                        }));
                        HandleResult::Success(serde_json::json!({
                            "navigationGraph": result
                        }))
                    }
                    None => HandleResult::Error(CdpErrorResponse {
                        id: 0,
                        error: crate::error::CdpErrorBody {
                            code: SERVER_ERROR,
                            message: "No active page".to_string(),
                        },
                        session_id: None,
                    }),
                }
            }
            "detectActions" => {
                match get_page_data(ctx, target_id).await {
                    Some((html_str, url)) => {
                        let frame_tree_json = ctx.get_frame_tree_json(target_id).await;
                        let page = if let Some(ft_json) = frame_tree_json {
                            match serde_json::from_str::<pardus_core::FrameTree>(&ft_json) {
                                Ok(ft) => pardus_core::Page::from_html_with_frame_tree(&html_str, &url, ft),
                                Err(_) => pardus_core::Page::from_html(&html_str, &url),
                            }
                        } else {
                            pardus_core::Page::from_html(&html_str, &url)
                        };
                        let tree = page.semantic_tree();
                        let mut actions = Vec::new();
                        collect_interactive_nodes(&tree.root, &mut actions);
                        HandleResult::Success(serde_json::json!({
                            "actions": actions
                        }))
                    }
                    None => HandleResult::Error(CdpErrorResponse {
                        id: 0,
                        error: crate::error::CdpErrorBody {
                            code: SERVER_ERROR,
                            message: "No active page".to_string(),
                        },
                        session_id: None,
                    }),
                }
            }
            "getActionPlan" => {
                match get_page_data(ctx, target_id).await {
                    Some((html_str, url)) => {
                        let page = pardus_core::Page::from_html(&html_str, &url);
                        let tree = page.semantic_tree();
                        let nav = page.navigation_graph();
                        let plan = pardus_core::interact::ActionPlan::analyze(&url, &tree, Some(&nav));
                        let result = serde_json::to_value(&plan).unwrap_or(serde_json::json!({
                            "error": "Failed to serialize action plan"
                        }));
                        HandleResult::Success(serde_json::json!({
                            "actionPlan": result
                        }))
                    }
                    None => HandleResult::Error(CdpErrorResponse {
                        id: 0,
                        error: crate::error::CdpErrorBody {
                            code: SERVER_ERROR,
                            message: "No active page".to_string(),
                        },
                        session_id: None,
                    }),
                }
            }
            "autoFill" => {
                let fields = match params.get("fields") {
                    Some(f) if f.is_object() => f.as_object().unwrap().clone(),
                    _ => serde_json::Map::new(),
                };

                let mut values = pardus_core::interact::AutoFillValues::new();
                for (key, val) in &fields {
                    if let Some(v) = val.as_str() {
                        values = values.set(key, v);
                    }
                }

                match get_page_data(ctx, target_id).await {
                    Some((html_str, url)) => {
                        let page = pardus_core::Page::from_html(&html_str, &url);
                        let result = pardus_core::interact::auto_fill::auto_fill(&values, &page);
                        let json = serde_json::to_value(&result).unwrap_or(serde_json::json!({
                            "error": "Failed to serialize auto-fill result"
                        }));
                        HandleResult::Success(json)
                    }
                    None => HandleResult::Error(CdpErrorResponse {
                        id: 0,
                        error: crate::error::CdpErrorBody {
                            code: SERVER_ERROR,
                            message: "No active page".to_string(),
                        },
                        session_id: None,
                    }),
                }
            }
            "getCoverage" => {
                match get_page_data(ctx, target_id).await {
                    Some((html_str, url)) => {
                        let html = scraper::Html::parse_document(&html_str);
                        let css_sources = pardus_debug::coverage::extract_inline_styles(&html);
                        let log = ctx.app.network_log.lock().unwrap_or_else(|e| e.into_inner());
                        let report = pardus_debug::coverage::CoverageReport::build(
                            &url, &html, &css_sources, &log,
                        );
                        let result = serde_json::to_value(&report)
                            .unwrap_or(serde_json::json!({"error": "serialization failed"}));
                        HandleResult::Success(result)
                    }
                    None => HandleResult::Error(CdpErrorResponse {
                        id: 0,
                        error: crate::error::CdpErrorBody {
                            code: SERVER_ERROR,
                            message: "No active page".to_string(),
                        },
                        session_id: None,
                    }),
                }
            }
            "wait" => {
                let condition_str = params["condition"].as_str().unwrap_or("");
                let condition = match condition_str {
                    "contentLoaded" => pardus_core::interact::WaitCondition::ContentLoaded,
                    "contentStable" => pardus_core::interact::WaitCondition::ContentStable,
                    "networkIdle" => pardus_core::interact::WaitCondition::NetworkIdle,
                    "minInteractive" => {
                        let min_count = params["minCount"].as_u64().unwrap_or(1) as usize;
                        pardus_core::interact::WaitCondition::MinInteractiveElements(min_count)
                    }
                    "selector" => {
                        let selector = params["selector"].as_str().unwrap_or("");
                        pardus_core::interact::WaitCondition::Selector(selector.to_string())
                    }
                    _ => {
                        return HandleResult::Error(CdpErrorResponse {
                            id: 0,
                            error: crate::error::CdpErrorBody {
                                code: crate::error::INVALID_PARAMS,
                                message: format!(
                                    "Unknown wait condition '{}'. Expected: contentLoaded, contentStable, networkIdle, minInteractive, selector",
                                    condition_str
                                ),
                            },
                            session_id: None,
                        });
                    }
                };

                let timeout_ms = params["timeoutMs"].as_u64().unwrap_or(10000) as u32;
                let interval_ms = params["intervalMs"].as_u64().unwrap_or(500) as u32;

                match get_page_data(ctx, target_id).await {
                    Some((html_str, url)) => {
                        let page = pardus_core::Page::from_html(&html_str, &url);
                        match pardus_core::interact::wait_smart(
                            &ctx.app,
                            &page,
                            &condition,
                            timeout_ms,
                            interval_ms,
                        ).await {
                            Ok(result) => {
                                let (satisfied, reason) = match result {
                                    pardus_core::interact::InteractionResult::WaitSatisfied { selector, found } => {
                                        (found, selector)
                                    }
                                    _ => (false, "unknown".to_string()),
                                };
                                HandleResult::Success(serde_json::json!({
                                    "satisfied": satisfied,
                                    "condition": condition_str,
                                    "reason": reason,
                                }))
                            }
                            Err(e) => HandleResult::Success(serde_json::json!({
                                "satisfied": false,
                                "condition": condition_str,
                                "reason": format!("error: {}", e),
                            })),
                        }
                    }
                    None => HandleResult::Error(CdpErrorResponse {
                        id: 0,
                        error: crate::error::CdpErrorBody {
                            code: SERVER_ERROR,
                            message: "No active page".to_string(),
                        },
                        session_id: None,
                    }),
                }
            }
            _ => method_not_found("Pardus", method),
        }
    }
}

async fn handle_interact(
    action: &str,
    selector: &str,
    value: &str,
    target_id: &str,
    fields_param: &Option<Value>,
    ctx: &DomainContext,
) -> Value {
    match action {
        "click" => {
            let page_data = get_page_data(ctx, target_id).await;
            let href = page_data.as_ref().and_then(|(html_str, url)| {
                let page = pardus_core::Page::from_html(&html_str, &url);
                page.query(selector).and_then(|el| el.href.clone())
            });

            if let Some(href) = href {
                match ctx.navigate(target_id, &href).await {
                    Ok(()) => serde_json::json!({ "success": true, "action": "click", "selector": selector }),
                    Err(e) => serde_json::json!({ "success": false, "error": e.to_string() }),
                }
            } else if let Some((html_str, url)) = page_data {
                let page = pardus_core::Page::from_html(&html_str, &url);
                let exists = page.query(selector).is_some();
                if exists {
                    serde_json::json!({ "success": true, "action": "click", "selector": selector, "note": "Element exists but is not a link" })
                } else {
                    serde_json::json!({ "success": false, "error": "Element not found" })
                }
            } else {
                serde_json::json!({ "success": false, "error": "No active page" })
            }
        }
        "type" => {
            match get_page_data(ctx, target_id).await {
                Some((html_str, url)) => {
                    let page = pardus_core::Page::from_html(&html_str, &url);
                    match page.query(selector) {
                        Some(handle) => {
                            match pardus_core::interact::actions::type_text(&page, &handle, value) {
                                Ok(_) => serde_json::json!({ "success": true, "action": "type", "selector": selector }),
                                Err(e) => serde_json::json!({ "success": false, "error": e.to_string() }),
                            }
                        }
                        None => serde_json::json!({ "success": false, "error": "Element not found" }),
                    }
                }
                None => serde_json::json!({ "success": false, "error": "No active page" }),
            }
        }
        "submit" => {
            let form_found = get_page_data(ctx, target_id).await
                .map(|(html_str, url)| {
                    let page = pardus_core::Page::from_html(&html_str, &url);
                    page.query(selector).is_some()
                })
                .unwrap_or(false);

            if form_found {
                let _ = fields_param;
                serde_json::json!({ "success": true, "action": "submit", "selector": selector, "note": "Form element found" })
            } else {
                serde_json::json!({ "success": false, "error": "Form not found" })
            }
        }
        _ => serde_json::json!({
            "success": false,
            "error": format!("Unknown action '{}'", action)
        }),
    }
}

fn collect_interactive_nodes(node: &pardus_core::SemanticNode, out: &mut Vec<Value>) {
    if node.is_interactive {
        out.push(serde_json::json!({
            "element_id": node.element_id,
            "selector": node.selector,
            "role": node.role.role_str(),
            "tag": node.tag,
            "name": node.name,
            "action": node.action,
            "href": node.href,
            "input_type": node.input_type,
            "disabled": node.is_disabled,
        }));
    }
    for child in &node.children {
        collect_interactive_nodes(child, out);
    }
}
