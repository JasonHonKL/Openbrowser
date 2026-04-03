use async_trait::async_trait;
use serde_json::Value;

use crate::domain::{method_not_found, CdpDomainHandler, DomainContext, HandleResult};
use crate::protocol::target::CdpSession;

pub struct LogDomain;

#[async_trait(?Send)]
impl CdpDomainHandler for LogDomain {
    fn domain_name(&self) -> &'static str {
        "Log"
    }

    async fn handle(
        &self,
        method: &str,
        _params: Value,
        session: &mut CdpSession,
        _ctx: &DomainContext,
    ) -> HandleResult {
        match method {
            "enable" => {
                session.enable_domain("Log");
                HandleResult::Ack
            }
            "disable" => {
                session.disable_domain("Log");
                HandleResult::Ack
            }
            _ => method_not_found("Log", method),
        }
    }
}
