use async_trait::async_trait;
use serde_json::Value;

use crate::domain::{method_not_found, CdpDomainHandler, DomainContext, HandleResult};
use crate::protocol::target::CdpSession;

pub struct ConsoleDomain;

#[async_trait(?Send)]
impl CdpDomainHandler for ConsoleDomain {
    fn domain_name(&self) -> &'static str {
        "Console"
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
                session.enable_domain("Console");
                HandleResult::Ack
            }
            "disable" => {
                session.disable_domain("Console");
                HandleResult::Ack
            }
            "clearMessages" => HandleResult::Ack,
            _ => method_not_found("Console", method),
        }
    }
}
