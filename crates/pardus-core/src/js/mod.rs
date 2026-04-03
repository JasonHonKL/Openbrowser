//! JavaScript execution module.
//!
//! Provides V8-based JavaScript execution via deno_core with:
//! - DOM operations (ops.rs)
//! - Fetch API (fetch.rs)
//! - SSE / EventSource (sse.rs)
//! - Extension registration (extension.rs)
//! - Runtime with thread-based timeouts (runtime.rs)

pub mod dom;
pub mod extension;
pub mod fetch;
pub mod ops;
pub mod runtime;
pub mod snapshot;
pub mod sse;
pub mod timer;

pub use runtime::execute_js;

// Re-export types that RuntimeDomain needs
// These are available regardless of whether the "js" feature is enabled
pub use runtime::{EvaluateResult, evaluate_js_expression};
