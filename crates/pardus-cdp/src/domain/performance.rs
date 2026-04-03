use async_trait::async_trait;
use serde_json::Value;

use crate::domain::{method_not_found, CdpDomainHandler, DomainContext, HandleResult};
use crate::protocol::target::CdpSession;

pub struct PerformanceDomain;

fn now_timestamp() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        * 1000.0
}

#[async_trait(?Send)]
impl CdpDomainHandler for PerformanceDomain {
    fn domain_name(&self) -> &'static str {
        "Performance"
    }

    async fn handle(
        &self,
        method: &str,
        _params: Value,
        session: &mut CdpSession,
        ctx: &DomainContext,
    ) -> HandleResult {
        match method {
            "enable" => {
                session.enable_domain("Performance");
                HandleResult::Ack
            }
            "disable" => {
                session.disable_domain("Performance");
                HandleResult::Ack
            }
            "getMetrics" => {
                let total_time = {
                    let log = ctx.app.network_log.lock().unwrap_or_else(|e| e.into_inner());
                    log.total_time_ms()
                };
                HandleResult::Success(serde_json::json!({
                    "metrics": [
                        { "name": "TotalTime", "value": total_time as f64 },
                        { "name": "Timestamp", "value": now_timestamp() },
                    ]
                }))
            }
            _ => method_not_found("Performance", method),
        }
    }
}
