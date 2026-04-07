//! V8 snapshot management for fast bootstrap startup.
//!
//! Creates a V8 startup snapshot from bootstrap.js so subsequent JS executions
//! skip parsing and compiling the ~1049-line DOM shim on every invocation.
//! The snapshot is created lazily on first use and shared for the process lifetime.

use std::sync::OnceLock;

use deno_core::RuntimeOptions;

use super::extension::pardus_dom;
use crate::sandbox::JsSandboxMode;

fn create_bootstrap_snapshot(bootstrap_code: &'static str) -> &'static [u8] {
    let mut runtime = deno_core::JsRuntimeForSnapshot::new(RuntimeOptions {
        extensions: vec![pardus_dom::init()],
        ..Default::default()
    });
    if let Err(e) = runtime.execute_script("bootstrap.js", bootstrap_code) {
        tracing::warn!("[JS] Bootstrap snapshot creation failed: {e}");
        // Fall back: return an empty slice — runtime will bootstrap normally
        return &[];
    }
    let snapshot = runtime.snapshot();
    tracing::debug!("[JS] Bootstrap snapshot created ({} bytes)", snapshot.len());
    Box::leak(snapshot)
}

static FULL_SNAPSHOT: OnceLock<&'static [u8]> = OnceLock::new();
static READONLY_SNAPSHOT: OnceLock<&'static [u8]> = OnceLock::new();

/// Get (or lazily create) a V8 startup snapshot for the given sandbox mode.
/// Returns `Some(snapshot)` if available, `None` if snapshot creation failed.
pub fn get_bootstrap_snapshot(mode: &JsSandboxMode) -> Option<&'static [u8]> {
    let bytes = match mode {
        JsSandboxMode::ReadOnly => READONLY_SNAPSHOT
            .get_or_init(|| create_bootstrap_snapshot(include_str!("bootstrap_readonly.js"))),
        _ => FULL_SNAPSHOT.get_or_init(|| create_bootstrap_snapshot(include_str!("bootstrap.js"))),
    };
    // Empty slice means snapshot creation failed — signal caller to bootstrap normally
    if bytes.is_empty() { None } else { Some(bytes) }
}
