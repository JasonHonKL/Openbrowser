//! Trait for resolving CAPTCHA / bot-challenge pauses.
//!
//! The host application (e.g. a Tauri desktop app) implements this trait to
//! present a challenge to a human and return the result.

use crate::detector::ChallengeInfo;

/// Outcome of a human-in-the-loop challenge resolution attempt.
#[derive(Debug, Clone)]
pub enum Resolution {
    /// The human solved the challenge — proceed with the current response.
    Continue,

    /// The human solved the challenge — inject these extra headers / cookies
    /// into subsequent requests.
    ModifyHeaders {
        /// Additional headers to include (e.g. custom auth headers).
        headers: std::collections::HashMap<String, String>,
        /// Raw `Cookie` header value obtained after solving.
        /// If `Some`, it will be merged into the request's Cookie header.
        cookies: Option<String>,
    },

    /// The human could not solve the challenge or gave up.
    Blocked(String),
}

/// Implemented by the host application to resolve challenges.
///
/// The implementation typically:
/// 1. Opens a webview window with the challenge URL
/// 2. Waits for the human to solve the CAPTCHA
/// 3. Extracts cookies from the webview
/// 4. Returns the cookies as a [`Resolution`]
///
/// # Example (Tauri)
///
/// ```ignore
/// struct TauriChallengeResolver {
///     app_handle: tauri::AppHandle,
/// }
///
/// #[async_trait]
/// impl ChallengeResolver for TauriChallengeResolver {
///     async fn resolve(&self, info: ChallengeInfo) -> Resolution {
///         // Open a webview window, wait for user, extract cookies
///         let cookies = open_captcha_window(&self.app_handle, &info.url).await;
///         Resolution::ModifyHeaders {
///             headers: HashMap::new(),
///             cookies: Some(cookies),
///         }
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait ChallengeResolver: Send + Sync {
    /// Present the challenge to a human and return the resolution.
    ///
    /// This method is called from a background tokio task, so it may block
    /// for an arbitrary duration while waiting for human input.
    async fn resolve(&self, info: ChallengeInfo) -> Resolution;
}
