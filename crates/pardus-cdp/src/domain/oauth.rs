//! CDP domain handler for OAuth 2.0 / OIDC operations.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

use crate::domain::{method_not_found, CdpDomainHandler, DomainContext, HandleResult};
use crate::error::SERVER_ERROR;
use crate::protocol::message::CdpErrorResponse;
use crate::protocol::target::CdpSession;

pub struct OAuthDomain;

fn resolve_target_id(session: &CdpSession) -> &str {
    session.target_id.as_deref().unwrap_or("default")
}

#[async_trait(?Send)]
impl CdpDomainHandler for OAuthDomain {
    fn domain_name(&self) -> &'static str {
        "OAuth"
    }

    async fn handle(
        &self,
        method: &str,
        params: Value,
        session: &mut CdpSession,
        ctx: &DomainContext,
    ) -> HandleResult {
        match method {
            "enable" => {
                session.enable_domain("OAuth");
                HandleResult::Ack
            }
            "disable" => {
                session.disable_domain("OAuth");
                HandleResult::Ack
            }
            "setProvider" => handle_set_provider(params, ctx).await,
            "startFlow" => handle_start_flow(params, ctx).await,
            "navigateForAuth" => {
                handle_navigate_for_auth(params, session, ctx).await
            }
            "completeFlow" => handle_complete_flow(params, ctx).await,
            "getTokens" => handle_get_tokens(params, ctx).await,
            "refreshTokens" => handle_refresh_tokens(params, ctx).await,
            "listSessions" => handle_list_sessions(ctx).await,
            "removeSession" => handle_remove_session(params, ctx).await,
            _ => method_not_found("OAuth", method),
        }
    }
}

// ---------------------------------------------------------------------------
// setProvider — register an OAuth provider configuration
// ---------------------------------------------------------------------------

async fn handle_set_provider(params: Value, ctx: &DomainContext) -> HandleResult {
    let name = match params["name"].as_str() {
        Some(n) => n.to_string(),
        None => {
            return err_response(
                crate::error::INVALID_PARAMS,
                "missing 'name' parameter",
            );
        }
    };

    let client_id = params["client_id"].as_str().unwrap_or("").to_string();
    let client_secret = params["client_secret"].as_str().map(String::from);
    let scopes = params["scopes"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let redirect_uri = params["redirect_uri"]
        .as_str()
        .unwrap_or("http://localhost:8080/callback")
        .to_string();

    let mut auth_endpoint = params["authorization_endpoint"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let mut token_endpoint = params["token_endpoint"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let issuer = params["issuer"].as_str().map(String::from);
    let mut userinfo_endpoint = params["userinfo_endpoint"].as_str().map(String::from);

    // If issuer is provided and endpoints are missing, try OIDC discovery
    let discovered = if let Some(issuer_url) = &issuer {
        if auth_endpoint.is_empty() || token_endpoint.is_empty() {
            match pardus_core::oauth::oidc::discover(&ctx.app.http_client, issuer_url).await {
                Ok(config) => {
                    if auth_endpoint.is_empty() {
                        auth_endpoint = config.authorization_endpoint;
                    }
                    if token_endpoint.is_empty() {
                        token_endpoint = config.token_endpoint;
                    }
                    if userinfo_endpoint.is_none() {
                        userinfo_endpoint = config.userinfo_endpoint;
                    }
                    true
                }
                Err(e) => {
                    return err_response(SERVER_ERROR, &format!("OIDC discovery failed: {e}"));
                }
            }
        } else {
            false
        }
    } else {
        false
    };

    if auth_endpoint.is_empty() || token_endpoint.is_empty() {
        return err_response(
            crate::error::INVALID_PARAMS,
            "authorization_endpoint and token_endpoint are required (or provide issuer for OIDC discovery)",
        );
    }

    let provider_config = pardus_core::oauth::OAuthProviderConfig {
        name: name.clone(),
        authorization_endpoint: auth_endpoint,
        token_endpoint,
        client_id,
        client_secret,
        scopes,
        redirect_uri,
        issuer,
        userinfo_endpoint,
    };

    let mut sessions = ctx.oauth_sessions.lock().await;
    sessions.register_provider(provider_config);

    HandleResult::Success(serde_json::json!({
        "success": true,
        "provider": name,
        "discovered": discovered,
    }))
}

// ---------------------------------------------------------------------------
// startFlow — generate authorization URL with PKCE
// ---------------------------------------------------------------------------

async fn handle_start_flow(params: Value, ctx: &DomainContext) -> HandleResult {
    let provider_name = match params["provider"].as_str() {
        Some(n) => n.to_string(),
        None => return err_response(crate::error::INVALID_PARAMS, "missing 'provider' parameter"),
    };

    let scopes = params["scopes"].as_array().map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect::<Vec<_>>()
    });

    let extra_params: HashMap<String, String> = params["extra_params"]
        .as_object()
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    // Get provider config
    let config = {
        let sessions = ctx.oauth_sessions.lock().await;
        match sessions.get_provider(&provider_name) {
            Some(c) => c.clone(),
            None => {
                return err_response(
                    SERVER_ERROR,
                    &format!("provider '{}' not registered", provider_name),
                );
            }
        }
    };

    let result = pardus_core::oauth::start_authorization(
        &config,
        scopes.as_deref(),
        &extra_params,
    );

    let auth_url = result.authorization_url.clone();
    let state = result.state.clone();
    let nonce = result.nonce.clone();

    let mut sessions = ctx.oauth_sessions.lock().await;
    match sessions.start_flow(
        &provider_name,
        result.state,
        result.nonce,
        result.pkce,
        result.authorization_url,
    ) {
        Ok(()) => HandleResult::Success(serde_json::json!({
            "authorizationUrl": auth_url,
            "state": state,
            "nonce": nonce,
        })),
        Err(e) => err_response(SERVER_ERROR, &e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// navigateForAuth — navigate to auth URL and capture redirect
// ---------------------------------------------------------------------------

async fn handle_navigate_for_auth(
    params: Value,
    session: &mut CdpSession,
    ctx: &DomainContext,
) -> HandleResult {
    let target_id = resolve_target_id(session);
    let provider_name = match params["provider"].as_str() {
        Some(n) => n.to_string(),
        None => {
            return err_response(crate::error::INVALID_PARAMS, "missing 'provider' parameter");
        }
    };

    // Get provider config for callback URL
    let callback_url = {
        let sessions = ctx.oauth_sessions.lock().await;
        match sessions.get_provider(&provider_name) {
            Some(c) => c.redirect_uri.clone(),
            None => {
                return err_response(
                    SERVER_ERROR,
                    &format!("provider '{}' not registered", provider_name),
                );
            }
        }
    };

    // Re-build the auth URL from stored session PKCE
    let auth_url = {
        let sessions = ctx.oauth_sessions.lock().await;
        let config = sessions.get_provider(&provider_name).unwrap().clone();
        // Note: PKCE is already stored from startFlow; we reconstruct the URL
        let result = pardus_core::oauth::start_authorization(&config, None, &HashMap::new());
        result.authorization_url
    };

    // Navigate with redirect capture
    match pardus_core::Page::navigate_with_redirect_capture(
        &ctx.app,
        &auth_url,
        &callback_url,
    )
    .await
    {
        Ok(pardus_core::OAuthNavigateResult::Callback { url, code, state }) => {
            // Validate state parameter
            let expected_state = {
                let sessions = ctx.oauth_sessions.lock().await;
                sessions.get_state(&provider_name)
            };

            if let Some(expected) = expected_state {
                if state != expected {
                    return err_response(
                        SERVER_ERROR,
                        &format!("state mismatch: expected '{}' got '{}'", expected, state),
                    );
                }
            }

            // Store the captured code
            let mut sessions = ctx.oauth_sessions.lock().await;
            let _ = sessions.set_pending_code(&provider_name, code.clone());

            HandleResult::Success(serde_json::json!({
                "status": "callback_captured",
                "code": code,
                "state": state,
                "url": url,
            }))
        }
        Ok(pardus_core::OAuthNavigateResult::Page(page)) => {
            // Landed on a login/consent page — update the target
            let html_str = page.html.html().to_string();
            let final_url = page.url.clone();
            let title = page.title();

            let mut targets = ctx.targets.lock().await;
            targets.insert(
                target_id.to_string(),
                crate::domain::TargetEntry {
                    url: final_url.clone(),
                    html: Some(html_str),
                    title,
                    js_enabled: false,
                    frame_tree_json: None,
                    form_state: HashMap::new(),
                },
            );

            HandleResult::Success(serde_json::json!({
                "status": "login_required",
                "url": final_url,
            }))
        }
        Err(e) => err_response(SERVER_ERROR, &format!("OAuth navigation failed: {e}")),
    }
}

// ---------------------------------------------------------------------------
// completeFlow — exchange authorization code for tokens
// ---------------------------------------------------------------------------

async fn handle_complete_flow(params: Value, ctx: &DomainContext) -> HandleResult {
    let provider_name = match params["provider"].as_str() {
        Some(n) => n.to_string(),
        None => {
            return err_response(crate::error::INVALID_PARAMS, "missing 'provider' parameter");
        }
    };

    let explicit_code = params["code"].as_str().map(String::from);

    // Get code, verifier, and provider config
    let (code, code_verifier, config) = {
        let sessions = ctx.oauth_sessions.lock().await;
        let config = match sessions.get_provider(&provider_name) {
            Some(c) => c.clone(),
            None => {
                return err_response(
                    SERVER_ERROR,
                    &format!("provider '{}' not registered", provider_name),
                );
            }
        };

        let code = explicit_code.unwrap_or_else(|| {
            sessions
                .get_pending_code(&provider_name)
                .unwrap_or_default()
        });

        let code_verifier = match sessions.get_code_verifier(&provider_name) {
            Ok(v) => v,
            Err(e) => return err_response(SERVER_ERROR, &e.to_string()),
        };

        (code, code_verifier, config)
    };

    if code.is_empty() {
        return err_response(
            SERVER_ERROR,
            "no authorization code available — call navigateForAuth first or provide 'code' parameter",
        );
    }

    // Exchange code for tokens
    match pardus_core::oauth::exchange_code(
        &ctx.app.http_client,
        &config,
        &code,
        &code_verifier,
    )
    .await
    {
        Ok(tokens) => {
            let has_refresh = tokens.refresh_token.is_some();
            let expires_at = tokens.expires_at;
            let scopes = tokens.scope.clone();

            // Validate ID token if present
            let id_token_claims = if let Some(id_token) = &tokens.id_token {
                pardus_core::oauth::validate_id_token(
                    id_token,
                    config.issuer.as_deref(),
                    &config.client_id,
                    None,
                )
                .ok()
            } else {
                None
            };

            // Store tokens
            let mut sessions = ctx.oauth_sessions.lock().await;
            sessions.complete_flow(&provider_name, tokens);

            HandleResult::Success(serde_json::json!({
                "success": true,
                "hasRefreshToken": has_refresh,
                "expiresAt": expires_at,
                "scopes": scopes,
                "idTokenClaims": id_token_claims,
            }))
        }
        Err(e) => {
            let mut sessions = ctx.oauth_sessions.lock().await;
            sessions.fail_flow(&provider_name, e.to_string());

            err_response(SERVER_ERROR, &format!("token exchange failed: {e}"))
        }
    }
}

// ---------------------------------------------------------------------------
// getTokens — retrieve stored tokens
// ---------------------------------------------------------------------------

async fn handle_get_tokens(params: Value, ctx: &DomainContext) -> HandleResult {
    let provider_name = match params["provider"].as_str() {
        Some(n) => n.to_string(),
        None => {
            return err_response(crate::error::INVALID_PARAMS, "missing 'provider' parameter");
        }
    };

    let sessions = ctx.oauth_sessions.lock().await;
    match sessions.get_tokens(&provider_name) {
        Some(tokens) => HandleResult::Success(serde_json::json!({
            "accessToken": tokens.access_token,
            "tokenType": tokens.token_type,
            "expiresAt": tokens.expires_at,
            "scope": tokens.scope,
            "hasRefreshToken": tokens.refresh_token.is_some(),
        })),
        None => err_response(
            SERVER_ERROR,
            &format!("no tokens for provider '{}'", provider_name),
        ),
    }
}

// ---------------------------------------------------------------------------
// refreshTokens — refresh access token
// ---------------------------------------------------------------------------

async fn handle_refresh_tokens(params: Value, ctx: &DomainContext) -> HandleResult {
    let provider_name = match params["provider"].as_str() {
        Some(n) => n.to_string(),
        None => {
            return err_response(crate::error::INVALID_PARAMS, "missing 'provider' parameter");
        }
    };

    let (refresh_token, config) = {
        let sessions = ctx.oauth_sessions.lock().await;
        let config = match sessions.get_provider(&provider_name) {
            Some(c) => c.clone(),
            None => {
                return err_response(
                    SERVER_ERROR,
                    &format!("provider '{}' not registered", provider_name),
                );
            }
        };
        let refresh_token = sessions
            .get_tokens(&provider_name)
            .and_then(|t| t.refresh_token.clone());

        (refresh_token, config)
    };

    let refresh_token = match refresh_token {
        Some(rt) => rt,
        None => return err_response(SERVER_ERROR, "no refresh token available"),
    };

    match pardus_core::oauth::refresh_tokens(
        &ctx.app.http_client,
        &config,
        &refresh_token,
    )
    .await
    {
        Ok(tokens) => {
            let expires_at = tokens.expires_at;
            let scopes = tokens.scope.clone();
            let has_refresh = tokens.refresh_token.is_some();

            let mut sessions = ctx.oauth_sessions.lock().await;
            sessions.complete_flow(&provider_name, tokens);

            HandleResult::Success(serde_json::json!({
                "success": true,
                "hasRefreshToken": has_refresh,
                "expiresAt": expires_at,
                "scopes": scopes,
            }))
        }
        Err(e) => err_response(SERVER_ERROR, &format!("token refresh failed: {e}")),
    }
}

// ---------------------------------------------------------------------------
// listSessions — list all OAuth sessions
// ---------------------------------------------------------------------------

async fn handle_list_sessions(ctx: &DomainContext) -> HandleResult {
    let sessions = ctx.oauth_sessions.lock().await;
    let list = sessions.get_all_sessions();
    HandleResult::Success(serde_json::json!({
        "sessions": list,
    }))
}

// ---------------------------------------------------------------------------
// removeSession — remove a session
// ---------------------------------------------------------------------------

async fn handle_remove_session(params: Value, ctx: &DomainContext) -> HandleResult {
    let provider_name = match params["provider"].as_str() {
        Some(n) => n.to_string(),
        None => {
            return err_response(crate::error::INVALID_PARAMS, "missing 'provider' parameter");
        }
    };

    let mut sessions = ctx.oauth_sessions.lock().await;
    let removed = sessions.remove_session(&provider_name);

    HandleResult::Success(serde_json::json!({
        "success": removed,
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn err_response(code: i64, message: &str) -> HandleResult {
    HandleResult::Error(CdpErrorResponse {
        id: 0,
        error: crate::error::CdpErrorBody {
            code,
            message: message.to_string(),
        },
        session_id: None,
    })
}
