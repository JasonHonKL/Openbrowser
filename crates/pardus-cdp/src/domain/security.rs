use async_trait::async_trait;
use serde_json::Value;

use crate::domain::{method_not_found, CdpDomainHandler, DomainContext, HandleResult};
use crate::protocol::target::CdpSession;

pub struct SecurityDomain;

#[async_trait(?Send)]
impl CdpDomainHandler for SecurityDomain {
    fn domain_name(&self) -> &'static str {
        "Security"
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
                session.enable_domain("Security");
                HandleResult::Ack
            }
            "disable" => {
                session.disable_domain("Security");
                HandleResult::Ack
            }
            "handleCertificateError" => HandleResult::Ack,
            "setOverrideCertificateErrors" => HandleResult::Ack,
            _ => method_not_found("Security", method),
        }
    }
}
