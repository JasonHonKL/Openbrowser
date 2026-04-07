//! OAuth 2.0 token types and basic JWT (ID token) parsing.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};

/// Tokens received from the token endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenSet {
    pub access_token: String,
    pub token_type: String,
    /// Unix epoch seconds when the access token expires.
    pub expires_at: Option<i64>,
    pub refresh_token: Option<String>,
    /// Raw JWT string for the ID token (if OIDC).
    pub id_token: Option<String>,
    pub scope: Option<String>,
}

impl OAuthTokenSet {
    /// Parse a token response from the token endpoint JSON.
    /// Converts `expires_in` (seconds from now) to `expires_at` (absolute timestamp).
    pub fn from_token_response(json: &serde_json::Value) -> anyhow::Result<Self> {
        let access_token = json["access_token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing access_token in token response"))?
            .to_string();

        let token_type = json["token_type"]
            .as_str()
            .unwrap_or("Bearer")
            .to_string();

        let expires_at = json["expires_in"].as_i64().map(|secs| {
            chrono::Utc::now().timestamp() + secs
        });

        let refresh_token = json["refresh_token"].as_str().map(String::from);
        let id_token = json["id_token"].as_str().map(String::from);
        let scope = json["scope"].as_str().map(String::from);

        Ok(Self {
            access_token,
            token_type,
            expires_at,
            refresh_token,
            id_token,
            scope,
        })
    }

    /// Check if the access token is expired or will expire within `buffer_secs`.
    pub fn is_expired(&self, buffer_secs: i64) -> bool {
        match self.expires_at {
            Some(exp) => chrono::Utc::now().timestamp() >= (exp - buffer_secs),
            None => false, // no expiry info, assume valid
        }
    }

    /// Build the Authorization header value (e.g., "Bearer <token>").
    pub fn authorization_header(&self) -> String {
        format!("{} {}", self.token_type, self.access_token)
    }
}

/// Claims extracted from an ID token JWT payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdTokenClaims {
    pub iss: String,
    pub sub: String,
    /// May be a single string or an array.
    #[serde(deserialize_with = "deserialize_aud")]
    pub aud: String,
    pub exp: i64,
    pub iat: i64,
    pub email: Option<String>,
    pub name: Option<String>,
    pub nonce: Option<String>,
}

/// Support both `"aud": "string"` and `"aud": ["string"]` in JWT payloads.
fn deserialize_aud<'de, D: serde::Deserializer<'de>>(de: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Aud {
        Single(String),
        Multiple(Vec<String>),
    }
    match Aud::deserialize(de)? {
        Aud::Single(s) => Ok(s),
        Aud::Multiple(v) => Ok(v.first().cloned().unwrap_or_default()),
    }
}

/// Basic ID token validation: decode the JWT payload and check claims.
/// Does NOT verify the JWT signature — only suitable for trusted environments.
pub fn validate_id_token(
    id_token: &str,
    expected_issuer: Option<&str>,
    expected_audience: &str,
    expected_nonce: Option<&str>,
) -> anyhow::Result<IdTokenClaims> {
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() != 3 {
        anyhow::bail!("invalid JWT: expected 3 parts, got {}", parts.len());
    }

    let payload_bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|e| anyhow::anyhow!("failed to decode JWT payload: {e}"))?;
    let claims: IdTokenClaims = serde_json::from_slice(&payload_bytes)
        .map_err(|e| anyhow::anyhow!("failed to parse JWT claims: {e}"))?;

    // Check expiry
    let now = chrono::Utc::now().timestamp();
    if claims.exp < now {
        anyhow::bail!(
            "ID token expired: exp={} now={}",
            claims.exp,
            now
        );
    }

    // Check issuer
    if let Some(iss) = expected_issuer {
        if claims.iss != iss {
            anyhow::bail!(
                "ID token issuer mismatch: expected={} got={}",
                iss,
                claims.iss
            );
        }
    }

    // Check audience
    if claims.aud != expected_audience {
        anyhow::bail!(
            "ID token audience mismatch: expected={} got={}",
            expected_audience,
            claims.aud
        );
    }

    // Check nonce
    if let Some(nonce) = expected_nonce {
        match &claims.nonce {
            Some(n) if n == nonce => {}
            Some(n) => anyhow::bail!("ID token nonce mismatch: expected={} got={}", nonce, n),
            None => anyhow::bail!("ID token missing nonce, expected={}", nonce),
        }
    }

    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_token(expires_in: i64) -> OAuthTokenSet {
        OAuthTokenSet {
            access_token: "test-token".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: Some(chrono::Utc::now().timestamp() + expires_in),
            refresh_token: Some("refresh-123".to_string()),
            id_token: None,
            scope: Some("openid profile".to_string()),
        }
    }

    #[test]
    fn token_not_expired() {
        let token = make_token(300);
        assert!(!token.is_expired(60));
    }

    #[test]
    fn token_expired() {
        let token = make_token(-10);
        assert!(token.is_expired(0));
    }

    #[test]
    fn token_near_expiry_within_buffer() {
        let token = make_token(30);
        assert!(token.is_expired(60));
    }

    #[test]
    fn authorization_header() {
        let token = make_token(300);
        assert_eq!(token.authorization_header(), "Bearer test-token");
    }

    #[test]
    fn from_token_response() {
        let json = serde_json::json!({
            "access_token": "abc123",
            "token_type": "Bearer",
            "expires_in": 3600,
            "refresh_token": "refresh-abc",
            "scope": "openid profile email"
        });
        let token = OAuthTokenSet::from_token_response(&json).unwrap();
        assert_eq!(token.access_token, "abc123");
        assert_eq!(token.token_type, "Bearer");
        assert!(token.expires_at.is_some());
        assert_eq!(token.refresh_token.as_deref(), Some("refresh-abc"));
    }

    #[test]
    fn validate_id_token_basic() {
        // Create a minimal JWT: header.payload.signature
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(
            serde_json::json!({
                "iss": "https://accounts.google.com",
                "sub": "12345",
                "aud": "my-client-id",
                "exp": chrono::Utc::now().timestamp() + 3600,
                "iat": chrono::Utc::now().timestamp(),
                "email": "test@example.com",
                "nonce": "abc"
            }).to_string(),
        );
        let jwt = format!("{header}.{payload}.signature");

        let claims = validate_id_token(
            &jwt,
            Some("https://accounts.google.com"),
            "my-client-id",
            Some("abc"),
        )
        .unwrap();

        assert_eq!(claims.iss, "https://accounts.google.com");
        assert_eq!(claims.sub, "12345");
        assert_eq!(claims.email.as_deref(), Some("test@example.com"));
    }
}
