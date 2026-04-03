use async_trait::async_trait;
use serde_json::Value;

use crate::domain::{method_not_found, CdpDomainHandler, DomainContext, HandleResult};
use crate::protocol::target::CdpSession;

pub struct BrowserDomain;

#[async_trait(?Send)]
impl CdpDomainHandler for BrowserDomain {
    fn domain_name(&self) -> &'static str {
        "Browser"
    }

    async fn handle(
        &self,
        method: &str,
        _params: Value,
        _session: &mut CdpSession,
        _ctx: &DomainContext,
    ) -> HandleResult {
        match method {
            "getVersion" => {
                HandleResult::Success(serde_json::json!({
                    "protocolVersion": "1.3",
                    "product": "PardusBrowser/0.1.0",
                    "revision": "1",
                    "userAgent": "PardusBrowser/0.1.0",
                    "jsVersion": "deno"
                }))
            }
            "close" => HandleResult::Ack,
            "getBrowserCommandLine" => {
                HandleResult::Success(serde_json::json!({ "arguments": [] }))
            }
            _ => method_not_found("Browser", method),
        }
    }
}
