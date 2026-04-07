pub mod app;
pub mod browser;
pub mod frame;
pub mod cache;
pub mod config;
pub mod csp;
pub mod dedup;
pub mod http;
pub mod interact;
pub mod intercept;
#[cfg(feature = "js")]
pub mod js;
pub mod navigation;
pub mod oauth;
pub mod output;
pub mod page;
pub mod page_analysis;
pub mod parser;
#[cfg(feature = "screenshot")]
pub mod screenshot;
pub mod feed;
pub mod pdf;
pub mod prefetch;
pub mod push;
pub mod resource;
pub mod sandbox;
pub mod semantic;
pub mod session;
pub mod sse;
pub mod tab;
#[cfg(feature = "tls-pinning")]
pub mod tls;
pub mod url_policy;
#[cfg(feature = "js")]
pub mod websocket;

pub use app::App;
pub use browser::Browser;
pub use config::{BrowserConfig, ProxyConfig, CspConfig, RetryConfig};
pub use page::Page;
pub use page::{RedirectHop, RedirectChain, OAuthNavigateResult};
pub use sandbox::{JsSandboxMode, SandboxPolicy};
pub use page::PageSnapshot;
pub use url_policy::UrlPolicy;
pub use frame::{FrameId, FrameData, FrameTree};
#[cfg(feature = "tls-pinning")]
pub use tls::{CertificatePinningConfig, CertPin, PinAlgorithm, PinMatchPolicy};
pub use csp::{CspPolicy, CspPolicySet, CspDirectiveKind, CspCheckResult};
#[cfg(feature = "js")]
pub use js::runtime::execute_js;
#[cfg(feature = "js")]
pub use js::runtime::{evaluate_js_expression, EvaluateResult};
pub use semantic::tree::{SelectOption, SemanticNode, SemanticRole, SemanticTree, TreeStats};
pub use navigation::graph::NavigationGraph;
pub use output::tree_formatter::format_tree;
pub use output::json_formatter::format_json;
pub use output::llm_formatter::format_llm;
pub use interact::{ElementHandle, FormState, InteractionResult, ScrollDirection};
pub use interact::upload::{FileEntry, UploadError};
#[cfg(feature = "js")]
pub use interact::action_plan::{ActionPlan, ActionType, PageType, SuggestedAction};
#[cfg(feature = "js")]
pub use interact::auto_fill::{AutoFillValues, AutoFillResult, ValidationStatus};
#[cfg(feature = "js")]
pub use interact::recording::{SessionRecording, SessionRecorder, RecordedAction, RecordedActionType, ReplayStepResult, replay};
pub use oauth::{
    exchange_code, refresh_tokens, start_authorization, StartFlowResult,
    discover as oidc_discover, OpenIdConfiguration, PkcePair,
    OAuthProviderConfig, OAuthSession, OAuthSessionManager, OAuthSessionStatus, SessionSummary,
    validate_id_token, IdTokenClaims, OAuthTokenSet,
};
pub use tab::tab::TabConfig;
pub use tab::{Tab, TabId, TabManager};
pub use intercept::InterceptorManager;
pub use intercept::{InterceptAction, ModifiedRequest, MockResponse, PauseHandle, InterceptorPhase, RequestContext, ResponseContext, Interceptor};
pub use dedup::{RequestDedup, DedupEntry, DedupResult, dedup_key};
pub use session::{CookieEntry, SessionStore};
pub use sse::{SseEvent, SseManager, SseParser};
#[cfg(feature = "js")]
pub use websocket::{WebSocketConfig, WebSocketConnection, WebSocketManager};
