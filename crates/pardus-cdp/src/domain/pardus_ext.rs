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
                        let result = serde_json::to_value(&*tree).unwrap_or(serde_json::json!({
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
                let href = params.get("href").and_then(|v| v.as_str()).unwrap_or("").to_string();

                let result = if !action.is_empty() {
                    let session_id = session.session_id.clone();
                    emit_action_started(ctx, &action, &selector, &value, &session_id);

                    let res = handle_interact(&action, &selector, &value, target_id, &fields_param, ctx).await;

                    // Fallback: if click failed but we have an href, navigate directly
                    let res = if res["success"].as_bool() == Some(false) && !href.is_empty() {
                        match ctx.navigate(target_id, &href).await {
                            Ok(()) => {
                                let new_url = ctx.get_url(target_id).await.unwrap_or_else(|| href.clone());

                                // Emit Page.frameNavigated so CDP bridge/frontend sync
                                ctx.event_bus.send(crate::protocol::message::CdpEvent {
                                    method: "Page.frameNavigated".to_string(),
                                    params: serde_json::json!({
                                        "frame": {
                                            "id": target_id,
                                            "url": new_url,
                                            "mimeType": "text/html",
                                        }
                                    }),
                                    session_id: Some(session_id.clone()),
                                });

                                serde_json::json!({ "success": true, "action": "click", "selector": selector, "navigated": true, "url": new_url, "fallback": "href" })
                            }
                            Err(e) => serde_json::json!({ "success": false, "error": format!("href fallback navigation failed: {}", e), "selector": selector })
                        }
                    } else {
                        res
                    };

                    emit_action_completed(ctx, &action, &selector, &res, &session_id);
                    res
                } else {
                    handle_interact(&action, &selector, &value, target_id, &fields_param, ctx).await
                };

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
    // Resolve selector: if it looks like #N (element_id from semantic tree),
    // use find_by_element_id. Otherwise treat as CSS selector.
    let page_data = match get_page_data(ctx, target_id).await {
        Some(d) => d,
        None => return serde_json::json!({ "success": false, "error": "No active page" }),
    };
    let (html_str, url) = &page_data;
    let page = pardus_core::Page::from_html(html_str, url);

    // Try element_id lookup first when selector is "#N"
    let handle = if let Some(num) = selector.strip_prefix('#') {
        if let Ok(id) = num.parse::<usize>() {
            page.find_by_element_id(id)
        } else {
            page.query(selector)
        }
    } else {
        page.query(selector)
    };

    match action {
        "click" => {
            let Some(h) = handle else {
                return serde_json::json!({ "success": false, "error": format!("Element {} not found", selector) });
            };

            // Build form state from accumulated typed values + any provided fields
            let mut form_state = pardus_core::interact::FormState::new();
            {
                let targets = ctx.targets.lock().await;
                if let Some(entry) = targets.get(target_id) {
                    for (name, val) in &entry.form_state {
                        form_state.set(name, val);
                    }
                }
            }
            merge_fields_into_form_state(&mut form_state, fields_param);

            match pardus_core::interact::actions::click(&ctx.app, &page, &h, &form_state).await {
                Ok(result) => match result {
                    pardus_core::interact::InteractionResult::Navigated(new_page) => {
                        update_target_from_page(ctx, target_id, &new_page).await;
                        serde_json::json!({ "success": true, "action": "click", "selector": selector, "navigated": true, "url": new_page.url })
                    }
                    pardus_core::interact::InteractionResult::ElementNotFound { selector: sel, reason } => {
                        serde_json::json!({ "success": false, "error": reason, "selector": sel })
                    }
                    _ => {
                        serde_json::json!({ "success": true, "action": "click", "selector": selector })
                    }
                },
                Err(e) => serde_json::json!({ "success": false, "error": e.to_string() }),
            }
        }
        "type" => {
            let Some(h) = handle else {
                return serde_json::json!({ "success": false, "error": format!("Element {} not found", selector) });
            };

            // Store the typed value in the target's form state so it can be
            // used when the form is later submitted via click or submit.
            if let Some(name) = &h.name {
                let mut targets = ctx.targets.lock().await;
                if let Some(entry) = targets.get_mut(target_id) {
                    entry.form_state.insert(name.clone(), value.to_string());
                }
            }

            serde_json::json!({ "success": true, "action": "type", "selector": selector, "value": value })
        }
        "submit" => {
            // Build form state from accumulated typed values + provided fields
            let mut form_state = pardus_core::interact::FormState::new();
            {
                let targets = ctx.targets.lock().await;
                if let Some(entry) = targets.get(target_id) {
                    for (name, val) in &entry.form_state {
                        form_state.set(name, val);
                    }
                }
            }
            merge_fields_into_form_state(&mut form_state, fields_param);

            // The selector should target a <form> element directly, or we try to
            // find a form associated with the selected element.
            let form_selector = if handle.is_some() {
                // Check if the selected element is a form itself or inside a form
                let h = handle.as_ref().unwrap();
                if h.tag == "form" {
                    selector.to_string()
                } else {
                    // Try to find enclosing form via the element's CSS selector
                    find_enclosing_form(&page, &h.selector).unwrap_or_else(|| selector.to_string())
                }
            } else {
                // Fallback: try the selector as a form selector
                selector.to_string()
            };

            match pardus_core::interact::form::submit_form(&ctx.app, &page, &form_selector, &form_state).await {
                Ok(result) => match result {
                    pardus_core::interact::InteractionResult::Navigated(new_page) => {
                        update_target_from_page(ctx, target_id, &new_page).await;
                        serde_json::json!({ "success": true, "action": "submit", "selector": selector, "navigated": true, "url": new_page.url })
                    }
                    pardus_core::interact::InteractionResult::ElementNotFound { selector: sel, reason } => {
                        serde_json::json!({ "success": false, "error": reason, "selector": sel })
                    }
                    _ => {
                        serde_json::json!({ "success": true, "action": "submit", "selector": selector })
                    }
                },
                Err(e) => serde_json::json!({ "success": false, "error": e.to_string() }),
            }
        }
        "scroll" => {
            // Scroll is handled client-side; just acknowledge
            serde_json::json!({ "success": true, "action": "scroll" })
        }
        "toggle" => {
            let Some(h) = handle else {
                return serde_json::json!({ "success": false, "error": format!("Element {} not found", selector) });
            };
            match pardus_core::interact::actions::toggle(&page, &h) {
                Ok(pardus_core::interact::InteractionResult::Toggled { checked, .. }) => {
                    // Record toggle state in form_state
                    if let Some(name) = &h.name {
                        let toggle_val = h.value.as_deref().unwrap_or("on");
                        let mut targets = ctx.targets.lock().await;
                        if let Some(entry) = targets.get_mut(target_id) {
                            if checked {
                                entry.form_state.insert(name.clone(), toggle_val.to_string());
                            } else {
                                entry.form_state.remove(name);
                            }
                        }
                    }
                    serde_json::json!({ "success": true, "action": "toggle", "selector": selector, "checked": checked })
                }
                Ok(pardus_core::interact::InteractionResult::ElementNotFound { reason, .. }) => {
                    serde_json::json!({ "success": false, "error": reason })
                }
                Ok(other) => serde_json::json!({ "success": true, "action": "toggle", "selector": selector, "note": format!("{:?}", other) }),
                Err(e) => serde_json::json!({ "success": false, "error": e.to_string() }),
            }
        }
        "select" => {
            let Some(h) = handle else {
                return serde_json::json!({ "success": false, "error": format!("Element {} not found", selector) });
            };
            match pardus_core::interact::actions::select_option(&page, &h, value) {
                Ok(pardus_core::interact::InteractionResult::Selected { value: selected_val, .. }) => {
                    if let Some(name) = &h.name {
                        let mut targets = ctx.targets.lock().await;
                        if let Some(entry) = targets.get_mut(target_id) {
                            entry.form_state.insert(name.clone(), selected_val.clone());
                        }
                    }
                    serde_json::json!({ "success": true, "action": "select", "selector": selector, "value": selected_val })
                }
                Ok(pardus_core::interact::InteractionResult::ElementNotFound { reason, .. }) => {
                    serde_json::json!({ "success": false, "error": reason })
                }
                Ok(other) => serde_json::json!({ "success": true, "action": "select", "selector": selector, "note": format!("{:?}", other) }),
                Err(e) => serde_json::json!({ "success": false, "error": e.to_string() }),
            }
        }
        _ => serde_json::json!({
            "success": false,
            "error": format!("Unknown action '{}'", action)
        }),
    }
}

/// Update target store after a successful page navigation or form submission.
async fn update_target_from_page(ctx: &DomainContext, target_id: &str, new_page: &pardus_core::Page) {
    let html_str = new_page.html.html().to_string();
    let url = new_page.url.clone();
    let title = new_page.title();
    let frame_tree_json = new_page.frame_tree.as_ref()
        .and_then(|ft| serde_json::to_string(ft).ok());

    let mut targets = ctx.targets.lock().await;
    if let Some(entry) = targets.get_mut(target_id) {
        entry.url = url;
        entry.html = Some(html_str);
        entry.title = title;
        entry.frame_tree_json = frame_tree_json;
        entry.form_state.clear();
    }
}

/// Merge fields from a JSON object into a FormState.
fn merge_fields_into_form_state(form_state: &mut pardus_core::interact::FormState, fields_param: &Option<Value>) {
    if let Some(fields) = fields_param {
        if let Some(obj) = fields.as_object() {
            for (key, val) in obj {
                if let Some(v) = val.as_str() {
                    form_state.set(key, v);
                }
            }
        }
    }
}

/// Walk up the DOM from the element matching `element_selector` to find an
/// enclosing `<form>`. Returns a CSS selector for the form.
fn find_enclosing_form(page: &pardus_core::Page, element_selector: &str) -> Option<String> {
    use scraper::{Selector, ElementRef};

    let sel = Selector::parse(element_selector).ok()?;
    let el = page.html.select(&sel).next()?;

    let mut current = el.parent().and_then(ElementRef::wrap);
    while let Some(parent) = current {
        if parent.value().name() == "form" {
            let form_sel = if let Some(id) = parent.value().attr("id") {
                format!("#{}", id)
            } else if let Some(action) = parent.value().attr("action") {
                format!("form[action=\"{}\"]", action)
            } else {
                "form".to_string()
            };
            return Some(form_sel);
        }
        current = parent.parent().and_then(ElementRef::wrap);
    }
    None
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

fn emit_action_started(ctx: &DomainContext, action: &str, selector: &str, value: &str, session_id: &str) {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let mut target: Value = serde_json::json!({ "selector": selector });
    if !value.is_empty() {
        target["value"] = serde_json::json!(value);
    }
    ctx.event_bus.send(crate::protocol::message::CdpEvent {
        method: "Pardus.actionStarted".to_string(),
        params: serde_json::json!({
            "action": action,
            "target": target,
            "timestamp": timestamp,
        }),
        session_id: Some(session_id.to_string()),
    });
}

fn emit_action_completed(ctx: &DomainContext, action: &str, selector: &str, result: &Value, session_id: &str) {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let success = result["success"].as_bool().unwrap_or(false);
    let event_method = if success {
        "Pardus.actionCompleted"
    } else {
        "Pardus.actionFailed"
    };
    ctx.event_bus.send(crate::protocol::message::CdpEvent {
        method: event_method.to_string(),
        params: serde_json::json!({
            "action": action,
            "selector": selector,
            "result": result,
            "timestamp": timestamp,
        }),
        session_id: Some(session_id.to_string()),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use pardus_core::Page;

    fn page_from(html: &str) -> Page {
        Page::from_html(html, "https://example.com/page")
    }

    // -----------------------------------------------------------------------
    // find_enclosing_form
    // -----------------------------------------------------------------------

    #[test]
    fn test_find_enclosing_form_by_id() {
        let html = r#"<html><body>
            <form id="login-form" action="/login" method="POST">
                <button type="submit" name="go">Login</button>
            </form>
        </body></html>"#;
        let page = page_from(html);
        let result = find_enclosing_form(&page, r#"button[name="go"]"#);
        assert_eq!(result, Some("#login-form".to_string()));
    }

    #[test]
    fn test_find_enclosing_form_by_action() {
        let html = r#"<html><body>
            <form action="/search">
                <input type="text" name="q">
                <button type="submit">Go</button>
            </form>
        </body></html>"#;
        let page = page_from(html);
        let result = find_enclosing_form(&page, r#"button[type="submit"]"#);
        assert_eq!(result, Some(r#"form[action="/search"]"#.to_string()));
    }

    #[test]
    fn test_find_enclosing_form_fallback() {
        let html = r#"<html><body>
            <form>
                <input type="text" name="q">
                <button type="submit">Go</button>
            </form>
        </body></html>"#;
        let page = page_from(html);
        let result = find_enclosing_form(&page, r#"button[type="submit"]"#);
        assert_eq!(result, Some("form".to_string()));
    }

    #[test]
    fn test_find_enclosing_form_no_parent() {
        let html = r#"<html><body>
            <button type="button">Standalone</button>
        </body></html>"#;
        let page = page_from(html);
        let result = find_enclosing_form(&page, "button");
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_enclosing_form_nested_deeply() {
        let html = r#"<html><body>
            <form id="deep-form">
                <div>
                    <div>
                        <div>
                            <input type="text" name="deep-input">
                        </div>
                    </div>
                </div>
            </form>
        </body></html>"#;
        let page = page_from(html);
        let result = find_enclosing_form(&page, r#"input[name="deep-input"]"#);
        assert_eq!(result, Some("#deep-form".to_string()));
    }

    // -----------------------------------------------------------------------
    // merge_fields_into_form_state
    // -----------------------------------------------------------------------

    #[test]
    fn test_merge_fields_into_form_state() {
        let mut form_state = pardus_core::interact::FormState::new();
        let fields = serde_json::json!({
            "username": "alice",
            "password": "secret123"
        });
        merge_fields_into_form_state(&mut form_state, &Some(fields));
        assert_eq!(form_state.get("username"), Some("alice"));
        assert_eq!(form_state.get("password"), Some("secret123"));
    }

    #[test]
    fn test_merge_fields_does_not_override_when_none() {
        let mut form_state = pardus_core::interact::FormState::new();
        form_state.set("existing", "value");
        merge_fields_into_form_state(&mut form_state, &None);
        assert_eq!(form_state.get("existing"), Some("value"));
    }

    #[test]
    fn test_merge_fields_overrides_existing() {
        let mut form_state = pardus_core::interact::FormState::new();
        form_state.set("username", "old");
        let fields = serde_json::json!({ "username": "new" });
        merge_fields_into_form_state(&mut form_state, &Some(fields));
        assert_eq!(form_state.get("username"), Some("new"));
    }
}
