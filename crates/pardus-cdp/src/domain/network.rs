use async_trait::async_trait;
use serde_json::Value;

use crate::domain::{method_not_found, CdpDomainHandler, DomainContext, HandleResult};
use crate::protocol::message::CdpEvent;
use crate::protocol::target::CdpSession;

pub struct NetworkDomain;

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
impl CdpDomainHandler for NetworkDomain {
    fn domain_name(&self) -> &'static str {
        "Network"
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
                session.enable_domain("Network");

                let target_id = resolve_target_id(session).to_string();
                let url = {
                    let targets = ctx.targets.lock().await;
                    targets.get(&target_id).map(|t| t.url.clone()).unwrap_or_default()
                };

                if !url.is_empty() {
                    let log = ctx.app.network_log.lock().unwrap_or_else(|e| e.into_inner());
                    for record in &log.records {
                        emit_request_events(session, record, &ctx.event_bus);
                    }
                }

                HandleResult::Ack
            }
            "disable" => {
                session.disable_domain("Network");
                HandleResult::Ack
            }
            "getCookies" => {
                let cookies = extract_cookies_from_target(session, ctx).await;
                HandleResult::Success(serde_json::json!({
                    "cookies": cookies
                }))
            }
            "getAllCookies" => {
                let cookies = extract_cookies_from_target(session, ctx).await;
                HandleResult::Success(serde_json::json!({
                    "cookies": cookies
                }))
            }
            "setCookie" => {
                let result = set_cookie_from_params(&params, session, ctx).await;
                HandleResult::Success(serde_json::json!({
                    "success": result
                }))
            }
            "deleteCookies" => {
                let name = params["name"].as_str().unwrap_or("");
                let domain = params["domain"].as_str().unwrap_or("");
                let path = params["path"].as_str().unwrap_or("/");
                if !name.is_empty() {
                    tracing::debug!(name, domain, path, "CDP deleteCookies requested");
                }
                HandleResult::Ack
            }
            "clearBrowserCookies" => {
                tracing::debug!("CDP clearBrowserCookies requested");
                HandleResult::Ack
            }
            "setExtraHTTPHeaders" => HandleResult::Ack,
            "emulateNetworkConditions" => HandleResult::Ack,
            "setCacheDisabled" => HandleResult::Ack,
            "getResponseBody" => {
                let _request_id = params["requestId"].as_str().unwrap_or("");
                HandleResult::Success(serde_json::json!({
                    "body": "",
                    "base64Encoded": false,
                }))
            }
            "setRequestInterception" => HandleResult::Ack,
            "continueInterceptedRequest" => HandleResult::Ack,
            "failInterceptedRequest" => HandleResult::Ack,
            "fulfillInterceptedRequest" => HandleResult::Ack,
            "getPostData" => {
                let _request_id = params["requestId"].as_str().unwrap_or("");
                HandleResult::Success(serde_json::json!({ "postData": "" }))
            }
            "setCertificatePinning" => {
                let result = set_cert_pinning(&params, ctx);
                HandleResult::Success(serde_json::json!({ "success": result }))
            }
            "getCertificatePinning" => {
                let pins = get_cert_pinning(ctx);
                HandleResult::Success(serde_json::json!({ "certificatePinning": pins }))
            }
            "clearCertificatePinning" => {
                let result = clear_cert_pinning(ctx);
                HandleResult::Success(serde_json::json!({ "success": result }))
            }
            "getCookiesByUrls" => HandleResult::Success(serde_json::json!({ "cookies": [] })),
            "getNavigationHistory" => {
                HandleResult::Success(serde_json::json!({
                    "currentIndex": 0,
                    "entries": []
                }))
            }
            "canEmulateNetworkConditions" => HandleResult::Success(serde_json::json!({ "result": false })),
            "setBypassServiceWorker" => HandleResult::Ack,
            "setBlockedURLs" => HandleResult::Ack,
            "enableReportingApi" => HandleResult::Ack,
            "canClearBrowserCache" => HandleResult::Success(serde_json::json!({ "result": true })),
            "canClearBrowserCookies" => HandleResult::Success(serde_json::json!({ "result": true })),
            "clearAcceptedEncodingsOverride" => HandleResult::Ack,
            "searchInResponseBody" => HandleResult::Success(serde_json::json!({ "result": [] })),
            "changeUserAgentOverride" => HandleResult::Ack,
            "getSecurityIsolationStatus" => HandleResult::Success(serde_json::json!({})),
            "getResponseBodyForInterception" => {
                HandleResult::Success(serde_json::json!({
                    "body": "",
                    "base64Encoded": false,
                }))
            }
            "takeResponseBodyForInterception" => {
                HandleResult::Success(serde_json::json!({
                    "body": "",
                    "base64Encoded": false,
                }))
            }
            "getHAR" => {
                let har = {
                    let log = ctx.app.network_log.lock().unwrap_or_else(|e| e.into_inner());
                    pardus_debug::har::HarFile::from_network_log(&log)
                };
                let har_value = serde_json::to_value(&har).unwrap_or(serde_json::json!({}));
                HandleResult::Success(serde_json::json!({ "log": har_value.get("log").cloned().unwrap_or(serde_json::json!({})) }))
            }
            _ => method_not_found("Network", method),
        }
    }
}

fn emit_request_events(
    session: &CdpSession,
    record: &pardus_debug::NetworkRecord,
    event_bus: &std::sync::Arc<crate::protocol::event_bus::EventBus>,
) {
    let request_id = &record.id;
    let _ = event_bus.send(CdpEvent {
        method: "Network.requestWillBeSent".to_string(),
        params: serde_json::json!({
            "requestId": request_id,
            "loaderId": "main",
            "documentURL": record.url,
            "request": {
                "url": record.url,
                "method": record.method,
                "headers": header_map_to_value(&record.request_headers),
                "postData": record.description.clone(),
                "initialPriority": "High",
                "referrerPolicy": "strict-origin-when-cross-origin",
            },
            "timestamp": now_timestamp(),
            "wallTime": now_timestamp() / 1000.0,
            "initiator": { "type": "other" },
            "type": "Document",
        }),
        session_id: Some(session.session_id.clone()),
    });

    let status = record.status.unwrap_or(200);
    let _ = event_bus.send(CdpEvent {
        method: "Network.responseReceived".to_string(),
        params: serde_json::json!({
            "requestId": request_id,
            "loaderId": "main",
            "timestamp": now_timestamp(),
            "type": "Document",
            "response": {
                "url": record.url,
                "status": status,
                "statusText": record.status_text.as_deref().unwrap_or_else(|| status_to_text(status)),
                "headers": header_map_to_value(&record.response_headers),
                "mimeType": record.content_type.as_deref().unwrap_or("text/html"),
                "connectionReused": true,
                "connectionId": 1,
                "encodedDataLength": 0,
                "responseTime": now_timestamp(),
            },
        }),
        session_id: Some(session.session_id.clone()),
    });

    let _ = event_bus.send(CdpEvent {
        method: "Network.loadingFinished".to_string(),
        params: serde_json::json!({
            "requestId": request_id,
            "timestamp": now_timestamp(),
            "encodedDataLength": 0,
        }),
        session_id: Some(session.session_id.clone()),
    });
}

fn header_map_to_value(headers: &[(String, String)]) -> Value {
    let mut map = serde_json::Map::new();
    for (k, v) in headers {
        map.insert(k.clone(), Value::String(v.clone()));
    }
    Value::Object(map)
}

fn status_to_text(status: u16) -> &'static str {
    match status {
        200..=299 => "OK",
        300..=399 => "Redirect",
        400..=499 => "Client Error",
        500..=599 => "Server Error",
        _ => "Unknown",
    }
}

fn set_cert_pinning(params: &Value, ctx: &DomainContext) -> bool {
    let pins = match params["pins"].as_array() {
        Some(p) => p,
        None => return false,
    };

    if pins.is_empty() {
        return false;
    }

    let mut pin_config = pardus_core::CertificatePinningConfig::new();

    if let Some(policy) = params["policy"].as_str() {
        match policy {
            "require-any" => pin_config.policy = pardus_core::PinMatchPolicy::RequireAny,
            "require-all" => pin_config.policy = pardus_core::PinMatchPolicy::RequireAll,
            _ => {}
        }
    }

    if let Some(enforce) = params["enforce"].as_bool() {
        pin_config.enforce = enforce;
    }

    for pin_value in pins {
        let pin = if let Some(hash) = pin_value["sha256"].as_str() {
            pardus_core::CertPin::spki_sha256(hash)
        } else if let Some(hash) = pin_value["sha384"].as_str() {
            pardus_core::CertPin::spki_sha384(hash)
        } else if let Some(hash) = pin_value["sha512"].as_str() {
            pardus_core::CertPin::spki_sha512(hash)
        } else if let Some(der) = pin_value["ca"].as_str() {
            let subject = pin_value["subject"].as_str().map(|s| s.to_string());
            pardus_core::CertPin::ca_cert(der, subject)
        } else {
            continue;
        };

        if let Some(host) = pin_value["host"].as_str() {
            pin_config.pins.entry(host.to_lowercase()).or_default().push(pin);
        } else {
            pin_config.default_pins.push(pin);
        }
    }

    let mut config = ctx.app.config.write();
    config.cert_pinning = Some(pin_config);
    true
}

fn get_cert_pinning(ctx: &DomainContext) -> Value {
    let config = ctx.app.config.read();
    match &config.cert_pinning {
        Some(pin_config) => {
            let mut pins = Vec::new();

            for (host, host_pins) in &pin_config.pins {
                for pin in host_pins {
                    let mut pin_obj = serde_json::json!({ "host": host });
                    match pin {
                        pardus_core::CertPin::SpkiHash { algorithm, hash } => {
                            pin_obj[algorithm.to_string()] = Value::String(hash.clone());
                        }
                        pardus_core::CertPin::CaCertificate { der_base64, subject } => {
                            pin_obj["ca"] = Value::String(der_base64.clone());
                            if let Some(subj) = subject {
                                pin_obj["subject"] = Value::String(subj.clone());
                            }
                        }
                    }
                    pins.push(pin_obj);
                }
            }

            for pin in &pin_config.default_pins {
                let mut pin_obj = serde_json::Map::new();
                match pin {
                    pardus_core::CertPin::SpkiHash { algorithm, hash } => {
                        pin_obj.insert(algorithm.to_string(), Value::String(hash.clone()));
                    }
                    pardus_core::CertPin::CaCertificate { der_base64, subject } => {
                        pin_obj.insert("ca".to_string(), Value::String(der_base64.clone()));
                        if let Some(subj) = subject {
                            pin_obj.insert("subject".to_string(), Value::String(subj.clone()));
                        }
                    }
                }
                pins.push(Value::Object(pin_obj));
            }

            serde_json::json!({
                "pins": pins,
                "policy": match pin_config.policy {
                    pardus_core::PinMatchPolicy::RequireAny => "require-any",
                    pardus_core::PinMatchPolicy::RequireAll => "require-all",
                },
                "enforce": pin_config.enforce,
            })
        }
        None => serde_json::json!({
            "pins": [],
            "policy": "require-any",
            "enforce": true,
        }),
    }
}

fn clear_cert_pinning(ctx: &DomainContext) -> bool {
    let mut config = ctx.app.config.write();
    config.cert_pinning = None;
    true
}

async fn extract_cookies_from_target(
    session: &CdpSession,
    ctx: &DomainContext,
) -> Vec<Value> {
    let target_id = session.target_id.as_deref().unwrap_or("default");
    let url = {
        let targets = ctx.targets.lock().await;
        targets.get(target_id).map(|t| t.url.clone()).unwrap_or_default()
    };

    if url.is_empty() {
        return vec![];
    }

    let parsed_url = match url::Url::parse(&url) {
        Ok(u) => u,
        Err(_) => return vec![],
    };

    let domain = parsed_url.host_str().unwrap_or("").to_string();

    ctx.app
        .network_log
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .records
        .iter()
        .flat_map(|record| {
            record
                .response_headers
                .iter()
                .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
                .filter_map(|(_, v)| parse_set_cookie_header(v, &domain))
        })
        .collect()
}

fn parse_set_cookie_header(header: &str, domain: &str) -> Option<Value> {
    let mut name = String::new();
    let mut value = String::new();
    let mut cookie_domain = String::new();
    let mut path = "/".to_string();
    let mut http_only = false;
    let mut secure = false;
    let mut same_site = Value::String("NotSet".to_string());

    for (i, part) in header.split(';').enumerate() {
        let part = part.trim();
        if i == 0 {
            if let Some(eq_pos) = part.find('=') {
                name = part[..eq_pos].trim().to_string();
                value = part[eq_pos + 1..].trim().to_string();
            }
        } else {
            let lower = part.to_lowercase();
            if lower.starts_with("domain=") {
                cookie_domain = part[7..].trim().trim_start_matches('.').to_string();
            } else if lower.starts_with("path=") {
                path = part[5..].trim().to_string();
            } else if lower.starts_with("httponly") {
                http_only = true;
            } else if lower.starts_with("secure") {
                secure = true;
            } else if lower.starts_with("samesite=") {
                let sv = part[9..].trim().to_string();
                same_site = match sv.as_str() {
                    "Strict" => Value::String("Strict".to_string()),
                    "Lax" => Value::String("Lax".to_string()),
                    "None" => Value::String("None".to_string()),
                    _ => Value::String("NotSet".to_string()),
                };
            }
        }
    }

    if name.is_empty() {
        return None;
    }

    let effective_domain = if cookie_domain.is_empty() {
        domain.to_string()
    } else {
        cookie_domain
    };

    Some(serde_json::json!({
        "name": name,
        "value": value,
        "domain": effective_domain,
        "path": path,
        "httpOnly": http_only,
        "secure": secure,
        "sameSite": same_site,
        "size": name.len() + value.len(),
        "session": true,
    }))
}

async fn set_cookie_from_params(
    params: &Value,
    _session: &CdpSession,
    _ctx: &DomainContext,
) -> bool {
    let name = match params["name"].as_str() {
        Some(n) if !n.is_empty() => n,
        _ => return false,
    };
    let _value = params["value"].as_str().unwrap_or("");
    tracing::debug!(name, "CDP setCookie requested");
    true
}
