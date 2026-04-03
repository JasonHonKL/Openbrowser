//! Challenge interceptor that plugs into pardus-core's interceptor pipeline.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::oneshot;
use pardus_core::intercept::{
    InterceptAction, Interceptor, InterceptorPhase, ModifiedRequest, PauseHandle,
    RequestContext, ResponseContext,
};

use crate::detector::ChallengeDetector;
use crate::resolver::ChallengeResolver;

/// An interceptor that pauses the pipeline when a CAPTCHA or bot-challenge
/// is detected and delegates resolution to a [`ChallengeResolver`].
///
/// Register this on an `InterceptorManager` (either on `App` or `Browser`):
///
/// ```ignore
/// let resolver: Arc<dyn ChallengeResolver> = ...;
/// app.interceptors.add(Box::new(
///     ChallengeInterceptor::with_defaults(resolver)
/// ));
/// ```
pub struct ChallengeInterceptor {
    detector: ChallengeDetector,
    resolver: Arc<dyn ChallengeResolver>,
}

impl std::fmt::Debug for ChallengeInterceptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChallengeInterceptor")
            .field("threshold", &self.detector.threshold)
            .finish()
    }
}

impl ChallengeInterceptor {
    pub fn new(detector: ChallengeDetector, resolver: Arc<dyn ChallengeResolver>) -> Self {
        Self { detector, resolver }
    }

    pub fn with_defaults(resolver: Arc<dyn ChallengeResolver>) -> Self {
        Self {
            detector: ChallengeDetector::default(),
            resolver,
        }
    }
}

#[async_trait]
impl Interceptor for ChallengeInterceptor {
    fn phase(&self) -> InterceptorPhase {
        InterceptorPhase::AfterResponse
    }

    fn matches(&self, ctx: &RequestContext) -> bool {
        ctx.is_navigation
    }

    async fn intercept_response(&self, _ctx: &mut ResponseContext) -> InterceptAction {
        // Detection + pause is handled by check_pause_response.
        // This method is a no-op; the pipeline calls check_pause_response
        // before calling intercept_response.
        InterceptAction::Continue
    }

    fn check_pause_response(&self, ctx: &ResponseContext) -> Option<PauseHandle> {
        let info = self.detector.detect_from_response(&ctx.url, ctx.status, &ctx.headers)?;

        tracing::info!(
            url = %info.url,
            kinds = ?info.kinds,
            score = info.risk_score,
            "challenge detected — pausing for human resolution"
        );

        let (tx, rx) = oneshot::channel();
        let resolver = self.resolver.clone();

        tokio::spawn(async move {
            let resolution = resolver.resolve(info).await;
            let action = match resolution {
                crate::resolver::Resolution::Continue => InterceptAction::Continue,
                crate::resolver::Resolution::ModifyHeaders { headers, cookies } => {
                    let mut mods = ModifiedRequest::default();
                    mods.headers = headers;
                    if let Some(cookie_str) = cookies {
                        mods.headers.insert("Cookie".to_string(), cookie_str);
                    }
                    InterceptAction::Modify(mods)
                }
                crate::resolver::Resolution::Blocked(reason) => {
                    tracing::warn!(reason = %reason, "challenge resolution failed — blocking");
                    InterceptAction::Block
                }
            };
            let _ = tx.send(action);
        });

        Some(PauseHandle {
            url: ctx.url.clone(),
            resume_rx: rx,
        })
    }
}
