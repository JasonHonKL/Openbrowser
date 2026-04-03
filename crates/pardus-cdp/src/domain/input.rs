use async_trait::async_trait;
use serde_json::Value;

use crate::domain::{method_not_found, CdpDomainHandler, DomainContext, HandleResult};
use crate::protocol::message::CdpErrorResponse;
use crate::protocol::target::CdpSession;

pub struct InputDomain;

fn resolve_target_id(session: &CdpSession) -> &str {
    session.target_id.as_deref().unwrap_or("default")
}

#[async_trait(?Send)]
impl CdpDomainHandler for InputDomain {
    fn domain_name(&self) -> &'static str {
        "Input"
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
            "dispatchMouseEvent" => {
                let mouse_type = params["type"].as_str().unwrap_or("");
                let _x = params["x"].as_f64();
                let _y = params["y"].as_f64();
                let _button = params["button"].as_str().unwrap_or("none");
                let _click_count = params["clickCount"].as_u64().unwrap_or(1);

                if mouse_type == "mousePressed" || mouse_type == "mouseReleased" {
                    if let (Some(html_str), Some(url)) = (ctx.get_html(target_id).await, ctx.get_url(target_id).await) {
                        let page = pardus_core::Page::from_html(&html_str, &url);
                        if let Some(el) = page.query("a[href], button, [role='button'], input[type='submit']") {
                            let _href = el.href.as_deref().unwrap_or("");
                            // Element found but not used in this stub implementation
                        }
                    }
                }

                HandleResult::Ack
            }
            "dispatchKeyEvent" => {
                let key_type = params["type"].as_str().unwrap_or("");
                let key = params["key"].as_str().unwrap_or("");
                let _modifiers = params["modifiers"].as_i64().unwrap_or(0);
                let _windows_virtual_key_code = params["windowsVirtualKeyCode"].as_i64();
                let _native_virtual_key_code = params["nativeVirtualKeyCode"].as_i64();

                if key_type == "keyDown" || key_type == "char" {
                    if !key.is_empty() {
                        if let (Some(html_str), Some(url)) = (ctx.get_html(target_id).await, ctx.get_url(target_id).await) {
                            let page = pardus_core::Page::from_html(&html_str, &url);
                            if let Some(el) = page.query("input[type='text'], input:not([type]), textarea") {
                                let _ = pardus_core::interact::actions::type_text(&page, &el, key);
                            }
                        }
                    }
                }

                HandleResult::Ack
            }
            "insertText" => {
                let text = params["text"].as_str().unwrap_or("");
                if !text.is_empty() {
                    if let (Some(html_str), Some(url)) = (ctx.get_html(target_id).await, ctx.get_url(target_id).await) {
                        let page = pardus_core::Page::from_html(&html_str, &url);
                        if let Some(el) = page.query("input[type='text'], input:not([type]), textarea") {
                            if pardus_core::interact::actions::type_text(&page, &el, text).is_err() {
                                return HandleResult::Error(CdpErrorResponse {
                                    id: 0,
                                    error: crate::error::CdpErrorBody {
                                        code: crate::error::SERVER_ERROR,
                                        message: "Input.insertText failed: no matching element".to_string(),
                                    },
                                    session_id: None,
                                });
                            }
                        }
                    }
                }
                HandleResult::Ack
            }
            "dispatchTouchEvent" => HandleResult::Ack,
            "emulateTouchFromMouseEvent" => HandleResult::Ack,
            "setInterceptDrags" => HandleResult::Ack,
            "synthesizePinchGesture" => HandleResult::Ack,
            "synthesizeScrollGesture" => HandleResult::Ack,
            "releaseActions" => HandleResult::Ack,
            "setIgnoreInputEvents" => HandleResult::Ack,
            "cancelDragging" => HandleResult::Ack,
            "dragIntercepted" => HandleResult::Ack,
            _ => method_not_found("Input", method),
        }
    }
}
