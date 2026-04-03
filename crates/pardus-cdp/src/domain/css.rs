use async_trait::async_trait;
use serde_json::Value;

use crate::domain::{method_not_found, CdpDomainHandler, DomainContext, HandleResult};
use crate::protocol::target::CdpSession;

pub struct CssDomain;

#[async_trait(?Send)]
impl CdpDomainHandler for CssDomain {
    fn domain_name(&self) -> &'static str {
        "CSS"
    }

    async fn handle(
        &self,
        method: &str,
        params: Value,
        _session: &mut CdpSession,
        ctx: &DomainContext,
    ) -> HandleResult {
        match method {
            "enable" => HandleResult::Ack,
            "disable" => HandleResult::Ack,
            "getComputedStyleForNode" => {
                let node_id = params["nodeId"].as_i64().unwrap_or(-1);
                let nm = ctx.node_map.lock().await;
                let _selector = nm.get_selector(node_id).map(|s| s.to_string());
                drop(nm);

                HandleResult::Success(serde_json::json!({
                    "computedStyle": []
                }))
            }
            "getInlineStylesForNode" => {
                let node_id = params["nodeId"].as_i64().unwrap_or(-1);
                let nm = ctx.node_map.lock().await;
                let selector = nm.get_selector(node_id).map(|s| s.to_string());
                drop(nm);

                let mut properties = Vec::new();
                if let Some(sel) = selector {
                    let target_id = "default";
                    if let Some(html) = ctx.get_html(target_id).await {
                        if let Some(style) = extract_inline_style(&html, &sel) {
                            for (prop, val) in style {
                                properties.push(serde_json::json!({
                                    "name": prop,
                                    "value": val,
                                }));
                            }
                        }
                    }
                }

                HandleResult::Success(serde_json::json!({
                    "inlineStyle": {
                        "cssProperties": properties,
                        "shorthandEntries": [],
                        "styleSheetId": format!("inline-{}", node_id),
                    },
                    "attributesStyle": null,
                }))
            }
            "getMatchedStylesForNode" => {
                HandleResult::Success(serde_json::json!({
                    "matchedCSSRules": [],
                    "inherited": [],
                    "inlineStyle": null,
                    "attributesStyle": null,
                    "cssKeyframesRules": [],
                    "positionFallbackRules": [],
                    "propertyRules": [],
                    "pseudoElements": [],
                    "pseudoElementsMatches": [],
                    "relatedNodes": [],
                    "cssLayers": [],
                }))
            }
            "collectClassNames" => {
                let _style_sheet_id = params["styleSheetId"].as_str().unwrap_or("");
                HandleResult::Success(serde_json::json!({ "classNames": [] }))
            }
            "getStyleSheetText" => {
                let _style_sheet_id = params["styleSheetId"].as_str().unwrap_or("");
                HandleResult::Error(crate::protocol::message::CdpErrorResponse {
                    id: 0,
                    error: crate::error::CdpErrorBody {
                        code: crate::error::SERVER_ERROR,
                        message: "Stylesheet text not available".to_string(),
                    },
                    session_id: None,
                })
            }
            "setStyleSheetText" => HandleResult::Ack,
            "setStyleTexts" => HandleResult::Success(serde_json::json!({ "styles": [] })),
            "addRule" => HandleResult::Success(serde_json::json!({
                "rule": { "selectorList": { "selectors": [], "text": "" } },
            })),
            "removeRule" => HandleResult::Ack,
            "forcePseudoState" => HandleResult::Ack,
            "getMediaQueries" => HandleResult::Success(serde_json::json!({ "medias": [] })),
            "setEffectivePropertyValueForNode" => HandleResult::Ack,
            "getPlatformFontsForNode" => HandleResult::Success(serde_json::json!({ "fonts": [] })),
            "setKeyframeKey" => HandleResult::Ack,
            "setLocalFontsEnabled" => HandleResult::Ack,
            "getBackgroundColors" => HandleResult::Success(serde_json::json!({ "backgroundColors": [] })),
            "setContainerQueryVariableModified" => HandleResult::Ack,
            "setFontFamilies" => HandleResult::Ack,
            "setFontVariations" => HandleResult::Ack,
            "setRuleSelector" => HandleResult::Ack,
            "startRuleUsageTracking" => HandleResult::Ack,
            "stopRuleUsageTracking" => {
                let target_id = _session.target_id.as_deref().unwrap_or("default");
                let mut rule_usage = Vec::new();
                if let Some(html_str) = ctx.get_html(target_id).await {
                    let html = scraper::Html::parse_document(&html_str);
                    let url = ctx.get_url(target_id).await.unwrap_or_default();
                    let css_sources = pardus_debug::coverage::extract_inline_styles(&html);
                    let log = ctx.app.network_log.lock().unwrap_or_else(|e| e.into_inner());
                    let report = pardus_debug::coverage::CoverageReport::build(
                        &url, &html, &css_sources, &log,
                    );
                    for stylesheet in &report.css.stylesheets {
                        for rule in &stylesheet.rules {
                            rule_usage.push(serde_json::json!({
                                "styleSheetId": stylesheet.source,
                                "startOffset": 0,
                                "endOffset": 0,
                                "used": rule.status == "matched",
                            }));
                        }
                    }
                }
                HandleResult::Success(serde_json::json!({ "ruleUsage": rule_usage }))
            }
            "takeCoverageDelta" => HandleResult::Success(serde_json::json!({ "coverage": [] })),
            "takeComputedStyleUpdates" => HandleResult::Success(serde_json::json!({ "computedStyles": [] })),
            "locateNode" => HandleResult::Ack,
            "getLayersForNode" => HandleResult::Success(serde_json::json!({ "layers": [] })),
            "stopLayerPainting" => HandleResult::Ack,
            "startLayerPainting" => HandleResult::Ack,
            "buildIndexedStyleSheetSummary" => HandleResult::Success(serde_json::json!({})),
            "getStyleSheetRefCount" => HandleResult::Success(serde_json::json!({ "refCount": 0 })),
            _ => method_not_found("CSS", method),
        }
    }
}

fn extract_inline_style(html: &str, selector: &str) -> Option<Vec<(String, String)>> {
    let doc = scraper::Html::parse_document(html);
    let sel = scraper::Selector::parse(selector).ok()?;
    let el = doc.select(&sel).next()?;

    let style_str = el.value().attr("style")?;
    if style_str.is_empty() {
        return None;
    }

    let mut properties = Vec::new();
    for decl in style_str.split(';') {
        let decl = decl.trim();
        if let Some(colon_pos) = decl.find(':') {
            let prop = decl[..colon_pos].trim().to_string();
            let val = decl[colon_pos + 1..].trim().to_string();
            if !prop.is_empty() && !val.is_empty() {
                properties.push((prop, val));
            }
        }
    }

    if properties.is_empty() { None } else { Some(properties) }
}
