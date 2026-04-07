//! OAuth 2.0 authorization code flow with PKCE.
//!
//! Provides functions for constructing authorization URLs, exchanging
//! authorization codes for tokens, and refreshing tokens.

use std::collections::HashMap;

use super::pkce::PkcePair;
use super::store::OAuthProviderConfig;
use super::token::OAuthTokenSet;

/// Result of starting an authorization flow.
pub struct StartFlowResult {
    /// The full authorization URL to navigate to.
    pub authorization_url: String,
    /// Anti-CSRF state parameter.
    pub state: String,
    /// OIDC nonce for ID token validation.
    pub nonce: String,
    /// The PKCE pair (code_verifier stored for later exchange).
    pub pkce: PkcePair,
}

/// Construct the authorization URL with all required OAuth 2.0 + PKCE parameters.
///
/// The caller should:
/// 1. Store the `state`, `nonce`, and `pkce` for later use.
/// 2. Navigate the browser to `authorization_url`.
/// 3. Intercept the redirect to `redirect_uri` and extract the `code` parameter.
/// 4. Call `exchange_code` with the code and the stored `pkce.code_verifier`.
pub fn start_authorization(
    config: &OAuthProviderConfig,
    scopes: Option<&[String]>,
    extra_params: &HashMap<String, String>,
) -> StartFlowResult {
    let pkce = PkcePair::generate();
    let state = generate_random_string(32);
    let nonce = generate_random_string(32);

    let effective_scopes = scopes
        .map(|s| s.to_vec())
        .unwrap_or_else(|| config.scopes.clone());

    let mut url = url::Url::parse(&config.authorization_endpoint)
        .expect("invalid authorization_endpoint URL");

    {
        let mut query = url.query_pairs_mut();
        query.append_pair("response_type", "code");
        query.append_pair("client_id", &config.client_id);
        query.append_pair("redirect_uri", &config.redirect_uri);
        query.append_pair("scope", &effective_scopes.join(" "));
        query.append_pair("state", &state);
        query.append_pair("code_challenge", &pkce.code_challenge);
        query.append_pair("code_challenge_method", "S256");

        if !effective_scopes.iter().any(|s| s == "openid") {
            // Add nonce only for OIDC flows
        } else {
            query.append_pair("nonce", &nonce);
        }

        for (key, value) in extra_params {
            query.append_pair(key, value);
        }
    }

    StartFlowResult {
        authorization_url: url.to_string(),
        state,
        nonce,
        pkce,
    }
}

/// Exchange an authorization code for tokens at the token endpoint.
///
/// Sends a `grant_type=authorization_code` request with PKCE code_verifier.
pub async fn exchange_code(
    http_client: &rquest::Client,
    config: &OAuthProviderConfig,
    code: &str,
    code_verifier: &str,
) -> anyhow::Result<OAuthTokenSet> {
    let mut params = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("redirect_uri", config.redirect_uri.clone()),
        ("client_id", config.client_id.clone()),
        ("code_verifier", code_verifier.to_string()),
    ];

    if let Some(secret) = &config.client_secret {
        params.push(("client_secret", secret.clone()));
    }

    let response = http_client
        .post(&config.token_endpoint)
        .form(&params)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("token exchange request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("token exchange returned status {status}: {body}");
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("failed to parse token response: {e}"))?;

    OAuthTokenSet::from_token_response(&json)
}

/// Refresh an access token using a refresh token.
///
/// Sends a `grant_type=refresh_token` request to the token endpoint.
pub async fn refresh_tokens(
    http_client: &rquest::Client,
    config: &OAuthProviderConfig,
    refresh_token: &str,
) -> anyhow::Result<OAuthTokenSet> {
    let mut params = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_token.to_string()),
        ("client_id", config.client_id.clone()),
    ];

    if let Some(secret) = &config.client_secret {
        params.push(("client_secret", secret.clone()));
    }

    let response = http_client
        .post(&config.token_endpoint)
        .form(&params)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("token refresh request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("token refresh returned status {status}: {body}");
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("failed to parse refresh response: {e}"))?;

    OAuthTokenSet::from_token_response(&json)
}

/// Generate a random alphanumeric string of the given length using getrandom.
fn generate_random_string(len: usize) -> String {
    let charset = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut buf = vec![0u8; len];
    getrandom::fill(&mut buf).expect("failed to generate random bytes");
    buf.iter()
        .map(|b| charset[*b as usize % charset.len()] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> OAuthProviderConfig {
        OAuthProviderConfig {
            name: "test".to_string(),
            authorization_endpoint: "https://auth.example.com/authorize".to_string(),
            token_endpoint: "https://auth.example.com/token".to_string(),
            client_id: "my-client-id".to_string(),
            client_secret: None,
            scopes: vec!["openid".to_string(), "profile".to_string()],
            redirect_uri: "http://localhost:8080/callback".to_string(),
            issuer: Some("https://auth.example.com".to_string()),
            userinfo_endpoint: None,
        }
    }

    #[test]
    fn authorization_url_contains_required_params() {
        let result = start_authorization(&test_config(), None, &HashMap::new());

        let url = url::Url::parse(&result.authorization_url).unwrap();
        let params: HashMap<String, String> = url.query_pairs().into_owned().collect();

        assert_eq!(params.get("response_type").unwrap(), "code");
        assert_eq!(params.get("client_id").unwrap(), "my-client-id");
        assert_eq!(
            params.get("redirect_uri").unwrap(),
            "http://localhost:8080/callback"
        );
        assert!(params.contains_key("state"));
        assert!(params.contains_key("code_challenge"));
        assert_eq!(params.get("code_challenge_method").unwrap(), "S256");
        assert!(params.contains_key("nonce")); // OIDC flow
        assert_eq!(params.get("scope").unwrap(), "openid profile");
    }

    #[test]
    fn authorization_url_with_extra_params() {
        let extras = HashMap::from([
            ("prompt".to_string(), "consent".to_string()),
            ("access_type".to_string(), "offline".to_string()),
        ]);
        let result = start_authorization(&test_config(), None, &extras);

        let url = url::Url::parse(&result.authorization_url).unwrap();
        let params: HashMap<String, String> = url.query_pairs().into_owned().collect();

        assert_eq!(params.get("prompt").unwrap(), "consent");
        assert_eq!(params.get("access_type").unwrap(), "offline");
    }

    #[test]
    fn authorization_url_custom_scopes() {
        let custom_scopes = vec!["email".to_string(), "calendar".to_string()];
        let result =
            start_authorization(&test_config(), Some(&custom_scopes), &HashMap::new());

        let url = url::Url::parse(&result.authorization_url).unwrap();
        let params: HashMap<String, String> = url.query_pairs().into_owned().collect();

        assert_eq!(params.get("scope").unwrap(), "email calendar");
    }

    #[test]
    fn state_and_nonce_are_random() {
        let a = start_authorization(&test_config(), None, &HashMap::new());
        let b = start_authorization(&test_config(), None, &HashMap::new());
        assert_ne!(a.state, b.state);
        assert_ne!(a.nonce, b.nonce);
        assert_ne!(a.pkce.code_verifier, b.pkce.code_verifier);
    }
}
