//! JavaScript execution module.
//!
//! Provides V8-based JavaScript execution via deno_core with:
//! - DOM operations (ops.rs)
//! - Fetch API (fetch.rs)
//! - Extension registration (extension.rs)
//! - Runtime with thread-based timeouts (runtime.rs)

pub mod dom;
pub mod extension;
pub mod fetch;
pub mod ops;
pub mod runtime;

pub use runtime::execute_js;
