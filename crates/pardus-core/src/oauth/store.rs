//! OAuth session state management.
//!
//! Tracks multiple concurrent OAuth sessions (one per provider) including
//! PKCE state, tokens, and session lifecycle.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::pkce::PkcePair;
use super::token::OAuthTokenSet;

/// OAuth provider configuration (manually provided or discovered via OIDC).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthProviderConfig {
    /// Logical name for this provider (e.g., "google", "github").
    pub name: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scopes: Vec<String>,
    pub redirect_uri: String,
    /// OIDC issuer URL (for validation and discovery).
    pub issuer: Option<String>,
    /// Userinfo endpoint (from OIDC discovery).
    pub userinfo_endpoint: Option<String>,
}

/// Status of an OAuth session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthSessionStatus {
    /// Provider registered, no flow started.
    Idle,
    /// Authorization URL generated, waiting for callback.
    AuthorizationPending,
    /// Tokens obtained, session active.
    Active,
    /// Access token expired (may have refresh token).
    Expired,
    /// Flow failed with an error.
    Failed(String),
}

impl std::fmt::Display for OAuthSessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::AuthorizationPending => write!(f, "authorization_pending"),
            Self::Active => write!(f, "active"),
            Self::Expired => write!(f, "expired"),
            Self::Failed(e) => write!(f, "failed: {e}"),
        }
    }
}

/// A single OAuth session for one provider.
pub struct OAuthSession {
    pub provider: OAuthProviderConfig,
    pub pkce: Option<PkcePair>,
    /// Anti-CSRF state parameter.
    pub state: Option<String>,
    /// OIDC nonce for ID token validation.
    pub nonce: Option<String>,
    pub tokens: Option<OAuthTokenSet>,
    pub status: OAuthSessionStatus,
    /// Captured authorization code from redirect.
    pub pending_code: Option<String>,
}

/// Summary of a session (for listing without exposing sensitive data).
#[derive(Debug, Clone, Serialize)]
pub struct SessionSummary {
    pub provider: String,
    pub status: String,
    pub has_access_token: bool,
    pub has_refresh_token: bool,
    pub expires_at: Option<i64>,
    pub scopes: Option<String>,
}

/// Manages multiple OAuth sessions keyed by provider name.
pub struct OAuthSessionManager {
    sessions: HashMap<String, OAuthSession>,
}

impl OAuthSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// Register a provider configuration (creates an Idle session).
    pub fn register_provider(&mut self, config: OAuthProviderConfig) {
        let name = config.name.clone();
        self.sessions.insert(
            name,
            OAuthSession {
                provider: config,
                pkce: None,
                state: None,
                nonce: None,
                tokens: None,
                status: OAuthSessionStatus::Idle,
                pending_code: None,
            },
        );
    }

    /// Start a new authorization flow: generate PKCE, state, nonce, and store them.
    /// Returns (authorization_url, state) — the caller should navigate to the URL.
    pub fn start_flow(
        &mut self,
        provider_name: &str,
        state: String,
        nonce: String,
        pkce: PkcePair,
        authorization_url: String,
    ) -> anyhow::Result<()> {
        let session = self
            .sessions
            .get_mut(provider_name)
            .ok_or_else(|| anyhow::anyhow!("provider '{}' not registered", provider_name))?;

        session.pkce = Some(pkce);
        session.state = Some(state);
        session.nonce = Some(nonce);
        session.status = OAuthSessionStatus::AuthorizationPending;
        session.pending_code = None;

        // Store the auth URL temporarily in state for retrieval
        let _ = authorization_url; // caller already has it
        Ok(())
    }

    /// Store the captured authorization code from the redirect callback.
    pub fn set_pending_code(&mut self, provider_name: &str, code: String) -> anyhow::Result<()> {
        let session = self
            .sessions
            .get_mut(provider_name)
            .ok_or_else(|| anyhow::anyhow!("provider '{}' not registered", provider_name))?;
        session.pending_code = Some(code);
        Ok(())
    }

    /// Get the PKCE code_verifier for a pending session (needed for token exchange).
    pub fn get_code_verifier(&self, provider_name: &str) -> anyhow::Result<String> {
        let session = self
            .sessions
            .get(provider_name)
            .ok_or_else(|| anyhow::anyhow!("provider '{}' not registered", provider_name))?;
        session
            .pkce
            .as_ref()
            .map(|p| p.code_verifier.clone())
            .ok_or_else(|| anyhow::anyhow!("no PKCE pair for provider '{}'", provider_name))
    }

    /// Get the pending authorization code (if captured from redirect).
    pub fn get_pending_code(&self, provider_name: &str) -> Option<String> {
        self.sessions
            .get(provider_name)
            .and_then(|s| s.pending_code.clone())
    }

    /// Get the stored state parameter for CSRF validation.
    pub fn get_state(&self, provider_name: &str) -> Option<String> {
        self.sessions
            .get(provider_name)
            .and_then(|s| s.state.clone())
    }

    /// Complete the flow by storing the obtained tokens.
    pub fn complete_flow(&mut self, provider_name: &str, tokens: OAuthTokenSet) {
        if let Some(session) = self.sessions.get_mut(provider_name) {
            session.tokens = Some(tokens);
            session.status = OAuthSessionStatus::Active;
            session.pending_code = None;
        }
    }

    /// Mark a flow as failed.
    pub fn fail_flow(&mut self, provider_name: &str, error: String) {
        if let Some(session) = self.sessions.get_mut(provider_name) {
            session.status = OAuthSessionStatus::Failed(error);
        }
    }

    /// Get the tokens for a provider (if any).
    pub fn get_tokens(&self, provider_name: &str) -> Option<&OAuthTokenSet> {
        self.sessions.get(provider_name).and_then(|s| s.tokens.as_ref())
    }

    /// Get mutable tokens for a provider (for refresh).
    pub fn get_tokens_mut(&mut self, provider_name: &str) -> Option<&mut OAuthTokenSet> {
        self.sessions
            .get_mut(provider_name)
            .and_then(|s| s.tokens.as_mut())
    }

    /// Get the provider config for a provider.
    pub fn get_provider(&self, provider_name: &str) -> Option<&OAuthProviderConfig> {
        self.sessions.get(provider_name).map(|s| &s.provider)
    }

    /// Find a session whose provider's issuer or token_endpoint domain matches the given URL.
    /// Used for auto-injection of Authorization headers.
    pub fn find_matching_session(&self, url: &str) -> Option<(&str, &OAuthTokenSet)> {
        let parsed = match url::Url::parse(url) {
            Ok(u) => u,
            Err(_) => return None,
        };
        let host = parsed.host_str().unwrap_or("");

        for (name, session) in &self.sessions {
            if session.tokens.is_none() {
                continue;
            }
            // Check if the URL host matches the provider's issuer or token endpoint domain
            let provider_domains = [
                session.provider.issuer.as_deref(),
                Some(&session.provider.token_endpoint),
                Some(&session.provider.authorization_endpoint),
            ];
            for domain in provider_domains.iter().flatten() {
                if let Ok(provider_url) = url::Url::parse(domain) {
                    if provider_url.host_str() == Some(host) {
                        return Some((name, session.tokens.as_ref().unwrap()));
                    }
                }
            }
        }
        None
    }

    /// List all sessions as summaries.
    pub fn get_all_sessions(&self) -> Vec<SessionSummary> {
        self.sessions
            .iter()
            .map(|(name, session)| SessionSummary {
                provider: name.clone(),
                status: session.status.to_string(),
                has_access_token: session.tokens.is_some(),
                has_refresh_token: session
                    .tokens
                    .as_ref()
                    .and_then(|t| t.refresh_token.clone())
                    .is_some(),
                expires_at: session.tokens.as_ref().and_then(|t| t.expires_at),
                scopes: session
                    .tokens
                    .as_ref()
                    .and_then(|t| t.scope.clone())
                    .or_else(|| {
                        if session.provider.scopes.is_empty() {
                            None
                        } else {
                            Some(session.provider.scopes.join(" "))
                        }
                    }),
            })
            .collect()
    }

    /// Remove a session.
    pub fn remove_session(&mut self, provider_name: &str) -> bool {
        self.sessions.remove(provider_name).is_some()
    }

    /// Check if a provider is registered.
    pub fn has_provider(&self, provider_name: &str) -> bool {
        self.sessions.contains_key(provider_name)
    }
}

impl Default for OAuthSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_provider(name: &str) -> OAuthProviderConfig {
        OAuthProviderConfig {
            name: name.to_string(),
            authorization_endpoint: format!("https://{name}.example.com/auth"),
            token_endpoint: format!("https://{name}.example.com/token"),
            client_id: "test-client".to_string(),
            client_secret: None,
            scopes: vec!["openid".to_string(), "profile".to_string()],
            redirect_uri: "http://localhost:8080/callback".to_string(),
            issuer: Some(format!("https://{name}.example.com")),
            userinfo_endpoint: None,
        }
    }

    fn test_tokens() -> OAuthTokenSet {
        OAuthTokenSet {
            access_token: "access-123".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: Some(chrono::Utc::now().timestamp() + 3600),
            refresh_token: Some("refresh-456".to_string()),
            id_token: None,
            scope: Some("openid profile".to_string()),
        }
    }

    #[test]
    fn register_and_check() {
        let mut mgr = OAuthSessionManager::new();
        mgr.register_provider(test_provider("google"));
        assert!(mgr.has_provider("google"));
        assert!(!mgr.has_provider("github"));
    }

    #[test]
    fn full_lifecycle() {
        let mut mgr = OAuthSessionManager::new();
        mgr.register_provider(test_provider("google"));

        let pkce = PkcePair::generate();
        let state = "test-state".to_string();
        let nonce = "test-nonce".to_string();
        let auth_url = "https://google.example.com/auth?...".to_string();

        mgr.start_flow("google", state.clone(), nonce, pkce, auth_url).unwrap();
        assert_eq!(
            mgr.sessions.get("google").unwrap().status,
            OAuthSessionStatus::AuthorizationPending
        );

        mgr.set_pending_code("google", "auth-code-123".to_string()).unwrap();
        assert_eq!(
            mgr.get_pending_code("google"),
            Some("auth-code-123".to_string())
        );

        let verifier = mgr.get_code_verifier("google").unwrap();
        assert!(!verifier.is_empty());

        mgr.complete_flow("google", test_tokens());
        assert_eq!(
            mgr.sessions.get("google").unwrap().status,
            OAuthSessionStatus::Active
        );
        assert!(mgr.get_tokens("google").is_some());
    }

    #[test]
    fn find_matching_session() {
        let mut mgr = OAuthSessionManager::new();
        mgr.register_provider(test_provider("google"));
        mgr.complete_flow("google", test_tokens());

        let result = mgr.find_matching_session("https://google.example.com/api/user");
        assert!(result.is_some());
        let (name, tokens) = result.unwrap();
        assert_eq!(name, "google");
        assert_eq!(tokens.access_token, "access-123");
    }

    #[test]
    fn find_no_match() {
        let mut mgr = OAuthSessionManager::new();
        mgr.register_provider(test_provider("google"));
        mgr.complete_flow("google", test_tokens());

        assert!(mgr
            .find_matching_session("https://other.example.com/api")
            .is_none());
    }

    #[test]
    fn list_sessions() {
        let mut mgr = OAuthSessionManager::new();
        mgr.register_provider(test_provider("google"));
        mgr.register_provider(test_provider("github"));

        let sessions = mgr.get_all_sessions();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn remove_session() {
        let mut mgr = OAuthSessionManager::new();
        mgr.register_provider(test_provider("google"));
        assert!(mgr.remove_session("google"));
        assert!(!mgr.has_provider("google"));
    }
}
