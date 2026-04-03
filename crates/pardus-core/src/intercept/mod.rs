//! Network request interception.
//!
//! Provides a layer for intercepting, blocking, modifying, redirecting,
//! and mocking HTTP requests and responses before they reach the network.
//!
//! ## Pause / Human-in-the-Loop
//!
//! Interceptors can request a **pause** by returning a [`PauseHandle`] from
//! [`Interceptor::check_pause`] (before-request) or
//! [`Interceptor::check_pause_response`] (after-response).
//!
//! When a pause is active the pipeline suspends the caller's future until the
//! resolver sends a resume decision through the oneshot channel inside the
//! handle.  This enables human-in-the-loop workflows such as CAPTCHA solving
//! without any knowledge of CAPTCHAs inside `pardus-core`.

pub mod builtins;
pub mod rules;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::oneshot;
use pardus_debug::{Initiator, ResourceType};

/// What an interceptor decides to do with a request or response.
#[derive(Debug, Clone)]
pub enum InterceptAction {
    /// Allow the request/response through unmodified.
    Continue,
    /// Modify before proceeding.
    Modify(ModifiedRequest),
    /// Drop the request/response entirely.
    Block,
    /// Redirect to a different URL (request phase only).
    Redirect(String),
    /// Return a synthetic response without making the real HTTP call.
    Mock(MockResponse),
}

/// Handle returned by an interceptor that wants to pause the pipeline.
///
/// The holder (typically a Tauri frontend or any async resolver) shows the
/// challenge to a human, then sends the desired [`InterceptAction`] through
/// the embedded [`oneshot::Sender`].  If the sender is dropped without sending
/// the pipeline treats the request as blocked.
#[derive(Debug)]
pub struct PauseHandle {
    /// URL being paused (for display / logging).
    pub url: String,
    /// The caller awaits this receiver; the resolver sends the resume action.
    pub resume_rx: oneshot::Receiver<InterceptAction>,
}

impl std::fmt::Display for PauseHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pause({})", self.url)
    }
}

/// Modifications to apply to a request.
#[derive(Debug, Clone, Default)]
pub struct ModifiedRequest {
    /// Replace the URL entirely.
    pub url: Option<String>,
    /// Add or replace headers (name -> value).
    pub headers: HashMap<String, String>,
    /// Remove these header names.
    pub remove_headers: Vec<String>,
    /// Replace the request body.
    pub body: Option<Vec<u8>>,
}

/// A synthetic response returned by a mock interceptor.
#[derive(Debug, Clone)]
pub struct MockResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

/// Which phase the interceptor runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterceptorPhase {
    BeforeRequest,
    AfterResponse,
}

/// Context describing an outgoing request, passed to interceptors.
#[derive(Debug, Clone)]
pub struct RequestContext {
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub resource_type: ResourceType,
    pub initiator: Initiator,
    pub is_navigation: bool,
}

/// Context describing a received response, passed to response-phase interceptors.
#[derive(Debug, Clone)]
pub struct ResponseContext {
    pub url: String,
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub resource_type: ResourceType,
}

/// Trait for request/response interceptors.
#[async_trait]
pub trait Interceptor: Send + Sync {
    /// Which phase this interceptor runs in.
    fn phase(&self) -> InterceptorPhase {
        InterceptorPhase::BeforeRequest
    }

    /// Whether this interceptor applies to the given request context.
    fn matches(&self, ctx: &RequestContext) -> bool {
        let _ = ctx;
        true
    }

    /// Called before a request is sent (when phase is BeforeRequest).
    async fn intercept_request(&self, ctx: &mut RequestContext) -> InterceptAction {
        let _ = ctx;
        InterceptAction::Continue
    }

    /// Called after a response is received (when phase is AfterResponse).
    async fn intercept_response(&self, ctx: &mut ResponseContext) -> InterceptAction {
        let _ = ctx;
        InterceptAction::Continue
    }

    /// Optional hook to pause the *request* before it is sent.
    ///
    /// Return `Some(PauseHandle)` to suspend the pipeline.  The pipeline will
    /// await `handle.resume_rx` and then return whatever action the resolver
    /// sends (typically `Continue` or `Modify` with extra headers such as
    /// cookies obtained from a solved CAPTCHA).
    ///
    /// Default: no pause.
    fn check_pause(&self, _ctx: &RequestContext) -> Option<PauseHandle> {
        None
    }

    /// Optional hook to pause after a *response* is received.
    ///
    /// Return `Some(PauseHandle)` to suspend the pipeline.  The resolver may
    /// send `Continue` to proceed with the current response, or `Block` to
    /// discard it and trigger a retry by the caller.
    ///
    /// Default: no pause.
    fn check_pause_response(&self, _ctx: &ResponseContext) -> Option<PauseHandle> {
        None
    }
}

/// Manages a list of interceptors. Cheaply cloneable via `Arc`.
pub struct InterceptorManager {
    interceptors: Arc<Mutex<Vec<Box<dyn Interceptor>>>>,
}

impl std::fmt::Debug for InterceptorManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self
            .interceptors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len();
        f.debug_struct("InterceptorManager")
            .field("count", &count)
            .finish()
    }
}

impl Clone for InterceptorManager {
    fn clone(&self) -> Self {
        Self {
            interceptors: self.interceptors.clone(),
        }
    }
}

impl Default for InterceptorManager {
    fn default() -> Self {
        Self::new()
    }
}

impl InterceptorManager {
    /// Create an empty manager (no interceptors).
    pub fn new() -> Self {
        Self {
            interceptors: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Register an interceptor.
    pub fn add(&self, interceptor: Box<dyn Interceptor>) {
        self.interceptors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(interceptor);
    }

    /// Number of registered interceptors.
    pub fn len(&self) -> usize {
        self.interceptors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    /// Whether any interceptors are registered.
    pub fn is_empty(&self) -> bool {
        self.interceptors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    }

    /// Run all before-request interceptors.
    ///
    /// First non-Continue action wins: Block > Redirect > Mock > Modify.
    /// Modifications are accumulated and applied to the context in-place.
    ///
    /// If any interceptor returns a [`PauseHandle`] via
    /// [`Interceptor::check_pause`], the pipeline suspends until the resolver
    /// sends a resume decision.
    pub async fn run_before_request(&self, ctx: &mut RequestContext) -> InterceptAction {
        // --- Pause phase ---
        if let Some(action) = self.run_pause_check(ctx).await {
            return action;
        }

        // --- Normal interception phase ---
        let interceptors = self
            .interceptors
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if interceptors.is_empty() {
            return InterceptAction::Continue;
        }

        let mut combined = ModifiedRequest::default();
        let mut had_modifications = false;
        let mut pending_redirect: Option<String> = None;
        let mut pending_mock: Option<MockResponse> = None;

        for interceptor in interceptors.iter() {
            if interceptor.phase() != InterceptorPhase::BeforeRequest {
                continue;
            }
            if !interceptor.matches(ctx) {
                continue;
            }
            match interceptor.intercept_request(ctx).await {
                InterceptAction::Block => return InterceptAction::Block,
                InterceptAction::Redirect(url) => {
                    pending_redirect = Some(url);
                }
                InterceptAction::Mock(mock) => {
                    pending_mock = Some(mock);
                }
                InterceptAction::Modify(mods) => {
                    if let Some(url) = mods.url {
                        combined.url = Some(url);
                    }
                    combined.headers.extend(mods.headers);
                    combined.remove_headers.extend(mods.remove_headers);
                    if mods.body.is_some() {
                        combined.body = mods.body;
                    }
                    had_modifications = true;
                }
                InterceptAction::Continue => {}
            }
        }

        // Apply combined modifications to context in-place
        if had_modifications {
            if let Some(url) = combined.url {
                ctx.url = url;
            }
            for (k, v) in &combined.headers {
                ctx.headers.insert(k.clone(), v.clone());
            }
            for k in &combined.remove_headers {
                ctx.headers.remove(k);
            }
            if combined.body.is_some() {
                ctx.body = combined.body;
            }
        }

        if let Some(url) = pending_redirect {
            return InterceptAction::Redirect(url);
        }
        if let Some(mock) = pending_mock {
            return InterceptAction::Mock(mock);
        }

        InterceptAction::Continue
    }

    /// Run after-response interceptors with pause support.
    ///
    /// If any interceptor returns a [`PauseHandle`] via
    /// [`Interceptor::check_pause_response`], the pipeline suspends until the
    /// resolver sends a resume decision.
    pub async fn run_after_response(&self, ctx: &mut ResponseContext) -> InterceptAction {
        // --- Pause phase ---
        if let Some(action) = self.run_pause_check_response(ctx).await {
            return action;
        }

        // --- Normal interception phase ---
        let interceptors = self
            .interceptors
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if interceptors.is_empty() {
            return InterceptAction::Continue;
        }

        for interceptor in interceptors.iter() {
            if interceptor.phase() != InterceptorPhase::AfterResponse {
                continue;
            }
            let request_ctx = RequestContext {
                url: ctx.url.clone(),
                method: String::new(),
                headers: ctx.headers.clone(),
                body: None,
                resource_type: ctx.resource_type.clone(),
                initiator: Initiator::Other,
                is_navigation: false,
            };
            if !interceptor.matches(&request_ctx) {
                continue;
            }
            match interceptor.intercept_response(ctx).await {
                InterceptAction::Block => return InterceptAction::Block,
                InterceptAction::Continue => {}
                InterceptAction::Modify(_) => {}
                other => return other,
            }
        }

        InterceptAction::Continue
    }

    /// Check all interceptors for a before-request pause.
    ///
    /// Returns `Some(resume_action)` if a pause was triggered and resolved,
    /// or `None` if no interceptor requested a pause.
    async fn run_pause_check(&self, ctx: &RequestContext) -> Option<InterceptAction> {
        let interceptors = self
            .interceptors
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for interceptor in interceptors.iter() {
            if interceptor.phase() != InterceptorPhase::BeforeRequest {
                continue;
            }
            if !interceptor.matches(ctx) {
                continue;
            }
            if let Some(handle) = interceptor.check_pause(ctx) {
                tracing::info!(url = %handle.url, "request paused by interceptor");
                drop(interceptors);
                return Some(match handle.resume_rx.await {
                    Ok(action) => action,
                    Err(_) => InterceptAction::Block,
                });
            }
        }
        None
    }

    /// Check all interceptors for an after-response pause.
    async fn run_pause_check_response(&self, ctx: &ResponseContext) -> Option<InterceptAction> {
        let interceptors = self
            .interceptors
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for interceptor in interceptors.iter() {
            if interceptor.phase() != InterceptorPhase::AfterResponse {
                continue;
            }
            let request_ctx = RequestContext {
                url: ctx.url.clone(),
                method: String::new(),
                headers: ctx.headers.clone(),
                body: None,
                resource_type: ctx.resource_type.clone(),
                initiator: Initiator::Other,
                is_navigation: false,
            };
            if !interceptor.matches(&request_ctx) {
                continue;
            }
            if let Some(handle) = interceptor.check_pause_response(ctx) {
                tracing::info!(url = %handle.url, "response paused by interceptor");
                drop(interceptors);
                return Some(match handle.resume_rx.await {
                    Ok(action) => action,
                    Err(_) => InterceptAction::Block,
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AlwaysBlock;
    #[async_trait]
    impl Interceptor for AlwaysBlock {
        fn phase(&self) -> InterceptorPhase {
            InterceptorPhase::BeforeRequest
        }
        fn matches(&self, _ctx: &RequestContext) -> bool {
            true
        }
        async fn intercept_request(&self, _ctx: &mut RequestContext) -> InterceptAction {
            InterceptAction::Block
        }
    }

    struct AlwaysRedirect;
    #[async_trait]
    impl Interceptor for AlwaysRedirect {
        fn phase(&self) -> InterceptorPhase {
            InterceptorPhase::BeforeRequest
        }
        fn matches(&self, _ctx: &RequestContext) -> bool {
            true
        }
        async fn intercept_request(&self, _ctx: &mut RequestContext) -> InterceptAction {
            InterceptAction::Redirect("https://redirected.com".to_string())
        }
    }

    struct AddHeader;
    #[async_trait]
    impl Interceptor for AddHeader {
        fn phase(&self) -> InterceptorPhase {
            InterceptorPhase::BeforeRequest
        }
        fn matches(&self, _ctx: &RequestContext) -> bool {
            true
        }
        async fn intercept_request(&self, _ctx: &mut RequestContext) -> InterceptAction {
            InterceptAction::Modify(ModifiedRequest {
                url: None,
                headers: HashMap::from([("X-Test".to_string(), "yes".to_string())]),
                remove_headers: vec![],
                body: None,
            })
        }
    }

    fn test_ctx() -> RequestContext {
        RequestContext {
            url: "https://example.com".to_string(),
            method: "GET".to_string(),
            headers: HashMap::new(),
            body: None,
            resource_type: ResourceType::Document,
            initiator: Initiator::Navigation,
            is_navigation: true,
        }
    }

    #[tokio::test]
    async fn test_empty_manager_returns_continue() {
        let mgr = InterceptorManager::new();
        let mut ctx = test_ctx();
        let action = mgr.run_before_request(&mut ctx).await;
        assert!(matches!(action, InterceptAction::Continue));
    }

    #[tokio::test]
    async fn test_block_wins_over_redirect() {
        let mgr = InterceptorManager::new();
        mgr.add(Box::new(AlwaysRedirect));
        mgr.add(Box::new(AlwaysBlock));
        let mut ctx = test_ctx();
        let action = mgr.run_before_request(&mut ctx).await;
        assert!(matches!(action, InterceptAction::Block));
    }

    #[tokio::test]
    async fn test_modify_adds_header() {
        let mgr = InterceptorManager::new();
        mgr.add(Box::new(AddHeader));
        let mut ctx = test_ctx();
        let action = mgr.run_before_request(&mut ctx).await;
        assert!(matches!(action, InterceptAction::Continue));
        assert_eq!(ctx.headers.get("X-Test").unwrap(), "yes");
    }

    #[tokio::test]
    async fn test_block_prevents_modify() {
        let mgr = InterceptorManager::new();
        mgr.add(Box::new(AlwaysBlock));
        mgr.add(Box::new(AddHeader));
        let mut ctx = test_ctx();
        let action = mgr.run_before_request(&mut ctx).await;
        assert!(matches!(action, InterceptAction::Block));
        assert!(!ctx.headers.contains_key("X-Test"));
    }

    #[tokio::test]
    async fn test_clone_shares_interceptors() {
        let mgr = InterceptorManager::new();
        mgr.add(Box::new(AlwaysBlock));
        let cloned = mgr.clone();
        assert_eq!(cloned.len(), 1);
        let mut ctx = test_ctx();
        let action = cloned.run_before_request(&mut ctx).await;
        assert!(matches!(action, InterceptAction::Block));
    }
}
