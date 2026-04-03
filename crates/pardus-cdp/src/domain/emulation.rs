use async_trait::async_trait;
use serde_json::Value;

use crate::domain::{method_not_found, CdpDomainHandler, DomainContext, HandleResult};
use crate::protocol::target::CdpSession;

pub struct EmulationDomain;

#[async_trait(?Send)]
impl CdpDomainHandler for EmulationDomain {
    fn domain_name(&self) -> &'static str {
        "Emulation"
    }

    async fn handle(
        &self,
        method: &str,
        params: Value,
        _session: &mut CdpSession,
        ctx: &DomainContext,
    ) -> HandleResult {
        match method {
            "setDeviceMetricsOverride" => {
                let width = params["width"].as_u64().unwrap_or(1280);
                let height = params["height"].as_u64().unwrap_or(720);
                let _scale = params["deviceScaleFactor"].as_f64().unwrap_or(1.0);
                let _mobile = params["mobile"].as_bool().unwrap_or(false);

                let mut config = ctx.app.config.write();
                config.viewport_width = width as u32;
                config.viewport_height = height as u32;

                HandleResult::Ack
            }
            "clearDeviceMetricsOverride" => {
                let mut config = ctx.app.config.write();
                config.viewport_width = 1280;
                config.viewport_height = 720;

                HandleResult::Ack
            }
            "setUserAgentOverride" => {
                let ua = params["userAgent"].as_str().unwrap_or("");
                if !ua.is_empty() {
                    let mut config = ctx.app.config.write();
                    config.user_agent = ua.to_string();
                    tracing::debug!(ua, "User agent override set via CDP");
                }
                HandleResult::Ack
            }
            "setTouchEmulationEnabled" => HandleResult::Ack,
            "setGeolocationOverride" => HandleResult::Ack,
            "clearGeolocationOverride" => HandleResult::Ack,
            "setDeviceOrientationOverride" => HandleResult::Ack,
            "clearDeviceOrientationOverride" => HandleResult::Ack,
            "setIdleOverride" => HandleResult::Ack,
            "clearIdleOverride" => HandleResult::Ack,
            "setCPUThrottlingRate" => HandleResult::Ack,
            "setColorScheme" => {
                let _scheme = params["colorScheme"].as_str().unwrap_or("light");
                HandleResult::Ack
            }
            "setEmulatedMedia" => {
                let _media = params["media"].as_str().unwrap_or("");
                HandleResult::Ack
            }
            "setScrollPosition" => {
                let _x = params["xOffset"].as_f64().unwrap_or(0.0);
                let _y = params["yOffset"].as_f64().unwrap_or(0.0);
                HandleResult::Ack
            }
            "setScriptExecutionDisabled" => HandleResult::Ack,
            "setAutomationOverride" => HandleResult::Ack,
            "setNavigatorOverrides" => HandleResult::Ack,
            "setScreenOrientationOverride" => HandleResult::Ack,
            "setVisibleSize" => HandleResult::Ack,
            "setHardwareConcurrencyOverride" => HandleResult::Ack,
            "setMediaFeatureOverride" => HandleResult::Ack,
            "setLanguageOverride" => {
                let _lang = params["language"].as_str().unwrap_or("");
                HandleResult::Ack
            }
            "setTimezoneOverride" => HandleResult::Ack,
            "setVirtualTimePolicy" => {
                HandleResult::Success(serde_json::json!({
                    "virtualTimeBudgetsLeft": [],
                }))
            }
            _ => method_not_found("Emulation", method),
        }
    }
}
