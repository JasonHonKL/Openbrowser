//! OIDC (OpenID Connect) discovery support.
//!
//! Fetches and parses the OpenID Provider Configuration from
//! `/.well-known/openid-configuration`.

use serde::Deserialize;

/// OpenID Provider Configuration Response (subset of fields).
///
/// See <https://openid.net/specs/openid-connect-discovery-1_0.html#ProviderMetadata>.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenIdConfiguration {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub userinfo_endpoint: Option<String>,
    #[serde(default)]
    pub jwks_uri: Option<String>,
    #[serde(default)]
    pub scopes_supported: Option<Vec<String>>,
    #[serde(default)]
    pub response_types_supported: Vec<String>,
    #[serde(default)]
    pub code_challenge_methods_supported: Option<Vec<String>>,
    #[serde(default)]
    pub token_endpoint_auth_methods_supported: Option<Vec<String>>,
}

/// Fetch the OIDC discovery document for the given issuer URL.
///
/// Appends `/.well-known/openid-configuration` to the issuer URL if not already present.
pub async fn discover(
    http_client: &rquest::Client,
    issuer_url: &str,
) -> anyhow::Result<OpenIdConfiguration> {
    let url = if issuer_url.ends_with("/.well-known/openid-configuration") {
        issuer_url.to_string()
    } else {
        format!(
            "{}/.well-known/openid-configuration",
            issuer_url.trim_end_matches('/')
        )
    };

    let response = http_client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("OIDC discovery request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("OIDC discovery returned status {status}: {body}");
    }

    let config: OpenIdConfiguration = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("failed to parse OIDC discovery document: {e}"))?;

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_discovery_document() {
        let json = serde_json::json!({
            "issuer": "https://accounts.google.com",
            "authorization_endpoint": "https://accounts.google.com/o/oauth2/v2/auth",
            "token_endpoint": "https://oauth2.googleapis.com/token",
            "userinfo_endpoint": "https://openidconnect.googleapis.com/v1/userinfo",
            "jwks_uri": "https://www.googleapis.com/oauth2/v3/certs",
            "scopes_supported": ["openid", "email", "profile"],
            "response_types_supported": ["code", "token", "id_token"],
            "code_challenge_methods_supported": ["S256"]
        });

        let config: OpenIdConfiguration = serde_json::from_value(json).unwrap();
        assert_eq!(config.issuer, "https://accounts.google.com");
        assert_eq!(
            config.authorization_endpoint,
            "https://accounts.google.com/o/oauth2/v2/auth"
        );
        assert_eq!(
            config.code_challenge_methods_supported,
            Some(vec!["S256".to_string()])
        );
    }
}
