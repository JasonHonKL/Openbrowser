//! OAuth 2.0 / OIDC support for Pardus browser.
//!
//! Implements authorization code flow with PKCE, token management,
//! OIDC discovery, and automatic Authorization header injection.

pub mod flow;
pub mod oidc;
pub mod pkce;
pub mod store;
pub mod token;

pub use flow::{exchange_code, refresh_tokens, start_authorization, StartFlowResult};
pub use oidc::{discover, OpenIdConfiguration};
pub use pkce::PkcePair;
pub use store::{
    OAuthProviderConfig, OAuthSession, OAuthSessionManager, OAuthSessionStatus, SessionSummary,
};
pub use token::{validate_id_token, IdTokenClaims, OAuthTokenSet};
