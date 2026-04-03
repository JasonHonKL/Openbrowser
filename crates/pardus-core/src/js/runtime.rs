//! JavaScript execution runtime.
//!
//! Uses deno_core (V8) to execute JavaScript with thread-based timeouts.
//! Provides a minimal `document` and `window` shim via ops that interact with the DOM.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use deno_core::*;
use scraper::{Html, Selector};
use url::Url;

use super::dom::DomDocument;
use super::extension::pardus_dom;
use crate::sandbox::{JsSandboxMode, SandboxPolicy};

// ==================== Configuration ====================

const SCRIPT_TIMEOUT_MS: u64 = 2000; // 2s per script
const MAX_SCRIPT_SIZE: usize = 100_000; // 100KB
const MAX_SCRIPTS: usize = 20;
const EVENT_LOOP_TIMEOUT_MS: u64 = 500;
const EVENT_LOOP_MAX_POLLS: usize = 3;
const THREAD_JOIN_GRACE_MS: u64 = 2000;

/// Analytics/tracking patterns to skip (all lowercase for case-insensitive matching).
///
/// Only genuine analytics, tracking, and ad-tech patterns are listed here.
/// Framework names (React, Vue, etc.) and web platform APIs are NOT included —
/// those are legitimate code that should execute normally.
const ANALYTICS_PATTERNS: &[&str] = &[
    // Google Analytics
    "google-analytics",
    "gtag(",
    "ga('",
    "gtag('",
    "googletagmanager",
    "gtm.js",
    "datalayer",
    // Facebook Pixel
    "facebook.com/tr",
    "fbq(",
    "fbq('",
    // Hotjar
    "hotjar",
    "hj(",
    "hj('",
    // Other analytics platforms
    "mixpanel",
    "amplitude",
    "segment.com",
    "newrelic",
    "nrqueue",
    "fullstory",
    "heap.io",
    "logrocket",
    // Ad tech
    "adsbygoogle",
    "ads.js",
    "doubleclick",
    // Customer support widgets
    "intercom",
    "zendesk",
    "helpscout",
    // PostHog
    "posthog",
    "posthog.init(",
    "posthog.com",
];

/// Patterns that indicate scripts likely to hang or cause issues.
///
/// These are narrow, targeted patterns — each one must genuinely protect
/// against a hang or destructive operation without causing false positives
/// on normal web scripts.
const PROBLEMATIC_PATTERNS: &[&str] = &[
    // Infinite loop patterns (exact forms only)
    "while(true)",
    "while (true)",
    "for(;;)",
    "for (;;)",
    "while(1)",
    "while (1)",
    // Destructive DOM operations
    "document.write(",
    "document.writeln(",
    // Eval / dynamic code generation (often leads to unbounded execution)
    "eval(",
    "new function(",
];

// ==================== Script Extraction ====================

#[derive(Debug, Clone)]
struct ScriptInfo {
    name: String,
    code: String,
}

/// Extract inline and external scripts from HTML, filtering out analytics/tracking.
fn extract_scripts(html: &str, base_url: &Url) -> Vec<ScriptInfo> {
    let doc = Html::parse_document(html);
    let selector = match Selector::parse("script") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    const MAX_EXTERNAL_SCRIPTS: usize = 5;
    const MAX_EXTERNAL_SCRIPT_SIZE: usize = 200_000; // 200 KB
    const EXTERNAL_FETCH_TIMEOUT_MS: u64 = 5_000;

    let mut inline_scripts: Vec<ScriptInfo> = Vec::new();
    let mut external_urls: Vec<String> = Vec::new();

    for el in doc.select(&selector) {
        // Collect external script URLs
        if let Some(src) = el.value().attr("src") {
            if external_urls.len() < MAX_EXTERNAL_SCRIPTS {
                if let Ok(resolved) = base_url.join(src) {
                    let url_str = resolved.to_string();
                    if url_str.starts_with("http://") || url_str.starts_with("https://") {
                        external_urls.push(url_str);
                    }
                }
            }
            continue;
        }

        let is_module = el.value().attr("type") == Some("module");
        let mut code = el.text().collect::<String>();

        if is_module {
            code = transform_module_syntax(&code);
        }

        if code.trim().is_empty() || code.len() > MAX_SCRIPT_SIZE {
            continue;
        }
        if is_analytics_script(&code) || is_problematic_script(&code) {
            continue;
        }

        inline_scripts.push(ScriptInfo {
            name: format!("inline_script_{}.js", inline_scripts.len()),
            code,
        });
        if inline_scripts.len() >= MAX_SCRIPTS {
            break;
        }
    }

    // Fetch external scripts synchronously (we're already inside a thread)
    let mut all_scripts = inline_scripts;
    for (i, url) in external_urls.into_iter().enumerate() {
        if all_scripts.len() >= MAX_SCRIPTS {
            break;
        }
        match fetch_external_script(&url, MAX_EXTERNAL_SCRIPT_SIZE, EXTERNAL_FETCH_TIMEOUT_MS) {
            Ok(code) => {
                if !code.trim().is_empty()
                    && code.len() <= MAX_SCRIPT_SIZE
                    && !is_analytics_script(&code)
                    && !is_problematic_script(&code)
                {
                    eprintln!("[JS] Fetched external script {}: {} ({} bytes)", i, url, code.len());
                    all_scripts.push(ScriptInfo {
                        name: format!("external_script_{}.js", i),
                        code,
                    });
                }
            }
            Err(e) => {
                eprintln!("[JS] Failed to fetch external script {}: {}", url, e);
            }
        }
    }

    all_scripts
}

/// Fetch an external JavaScript file via HTTP.
fn fetch_external_script(url: &str, max_size: usize, timeout_ms: u64) -> anyhow::Result<String> {
    let parsed = Url::parse(url)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        anyhow::bail!("Non-HTTP scheme: {}", parsed.scheme());
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .user_agent("PardusBrowser/0.1.0")
        .build()?;

    let response = client.get(url).send()?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        anyhow::bail!("HTTP {}", status);
    }

    if let Some(len) = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok())
    {
        if len > max_size {
            anyhow::bail!("Script too large: {} bytes", len);
        }
    }

    let body = response.text()?;
    Ok(body)
}

fn is_analytics_script(code: &str) -> bool {
    let lower = code.to_lowercase();
    ANALYTICS_PATTERNS.iter().any(|p| lower.contains(p))
}

fn is_problematic_script(code: &str) -> bool {
    let lower = code.to_lowercase();
    PROBLEMATIC_PATTERNS.iter().any(|p| lower.contains(p))
}

fn transform_module_syntax(code: &str) -> String {
    let mut result = String::new();

    for line in code.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("import ") || trimmed.starts_with("import{") || trimmed.starts_with("import(") {
            continue;
        }

        if trimmed.starts_with("export default ") {
            result.push_str(&trimmed[15..]);
            result.push('\n');
            continue;
        }

        if trimmed.starts_with("export const ") {
            result.push_str(&trimmed[7..]);
            result.push('\n');
            continue;
        }
        if trimmed.starts_with("export let ") {
            result.push_str(&trimmed[7..]);
            result.push('\n');
            continue;
        }
        if trimmed.starts_with("export var ") {
            result.push_str(&trimmed[7..]);
            result.push('\n');
            continue;
        }
        if trimmed.starts_with("export function ") {
            result.push_str(&trimmed[7..]);
            result.push('\n');
            continue;
        }
        if trimmed.starts_with("export class ") {
            result.push_str(&trimmed[7..]);
            result.push('\n');
            continue;
        }
        if trimmed.starts_with("export {") || trimmed.starts_with("export{") {
            let inner = trimmed.trim_start_matches("export ");
            result.push_str(inner);
            result.push('\n');
            continue;
        }
        if trimmed.starts_with("export = ") {
            continue;
        }

        result.push_str(line);
        result.push('\n');
    }

    result
}

// ==================== Runtime Creation ====================

/// Create a deno runtime with our DOM extension.
fn create_runtime(
    dom: Rc<RefCell<DomDocument>>,
    base_url: &Url,
    sandbox: &SandboxPolicy,
) -> anyhow::Result<JsRuntime> {
    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![pardus_dom::init()],
        ..Default::default()
    });

    // Store DOM in op state
    runtime.op_state().borrow_mut().put(dom);

    // Store timer queue in op state
    runtime.op_state().borrow_mut().put(super::timer::TimerQueue::new());

    // Store sandbox policy in op state so ops can check restrictions
    runtime.op_state().borrow_mut().put(sandbox.clone());

    // Set up window.location from base_url
    let location_js = format!(
        r#"
        window.location = {{
            href: "{}",
            origin: "{}",
            protocol: "{}",
            host: "{}",
            hostname: "{}",
            pathname: "{}",
            search: "{}",
            hash: "{}"
        }};
    "#,
        base_url.as_str(),
        base_url.origin().ascii_serialization(),
        base_url.scheme(),
        base_url.host_str().unwrap_or(""),
        base_url.host_str().unwrap_or(""),
        base_url.path(),
        base_url.query().unwrap_or(""),
        base_url.fragment().unwrap_or("")
    );

    runtime.execute_script("location.js", location_js)?;

    Ok(runtime)
}

// ==================== Thread-Based Execution ====================

/// Result of script execution in a thread.
struct ThreadResult {
    dom_html: Option<String>,
    #[allow(dead_code)]
    error: Option<String>,
}/// Execute scripts in a separate thread with timeout, graceful termination, and no leaks.
fn execute_scripts_with_timeout(
    html: String,
    base_url: String,
    scripts: Vec<ScriptInfo>,
    timeout_ms: u64,
    sandbox: SandboxPolicy,
) -> Option<String> {
    let result = Arc::new(Mutex::new(ThreadResult {
        dom_html: None,
        error: None,
    }));
    let result_clone = result.clone();
    let terminated = Arc::new(AtomicBool::new(false));
    let terminated_clone = terminated.clone();

    let handle = thread::spawn(move || {
        // Parse base URL
        let base = match Url::parse(&base_url) {
            Ok(u) => u,
            Err(e) => {
                *result_clone.lock().unwrap_or_else(|e| e.into_inner()) = ThreadResult {
                    dom_html: None,
                    error: Some(format!("Invalid base URL: {}", e)),
                };
                return;
            }
        };

        // Create DOM from HTML
        let mut doc = DomDocument::from_html(&html);

        // Sandbox: set max DOM nodes limit
        if sandbox.js_max_dom_nodes > 0 {
            doc.set_max_nodes(sandbox.js_max_dom_nodes);
        }

        // Sandbox: propagate fetch block flag for async ops (SSE uses OpState directly)
        super::fetch::set_sandbox_fetch_blocked(sandbox.block_js_fetch);

        let dom = Rc::new(RefCell::new(doc));

        // Create runtime (pass sandbox policy)
        let mut runtime = match create_runtime(dom.clone(), &base, &sandbox) {
            Ok(r) => r,
            Err(e) => {
                *result_clone.lock().unwrap_or_else(|e| e.into_inner()) = ThreadResult {
                    dom_html: None,
                    error: Some(format!("Failed to create runtime: {}", e)),
                };
                return;
            }
        };

        // Execute bootstrap.js — select variant based on sandbox mode
        let bootstrap = match sandbox.js_mode {
            JsSandboxMode::ReadOnly => include_str!("bootstrap_readonly.js"),
            _ => include_str!("bootstrap.js"),
        };
        if let Err(e) = runtime.execute_script("bootstrap.js", bootstrap) {
            *result_clone.lock().unwrap_or_else(|e| e.into_inner()) = ThreadResult {
                dom_html: None,
                error: Some(format!("Bootstrap error: {}", e)),
            };
            return;
        }

        if terminated_clone.load(Ordering::Relaxed) {
            return;
        }

        // Execute each script with termination checks between them
        for script in scripts {
            if terminated_clone.load(Ordering::Relaxed) {
                return;
            }
            if let Err(e) = runtime.execute_script(script.name.clone(), script.code) {
                // Log error but continue with next script
                eprintln!("[JS] Script {} error: {}", script.name, e);
            }
        }

        if terminated_clone.load(Ordering::Relaxed) {
            return;
        }

        // Fire DOMContentLoaded event after all scripts
        let _ = runtime.execute_script("dom_content_loaded.js", r#"
    (function() {
        if (typeof _fireDOMContentLoaded === 'function') _fireDOMContentLoaded();
        var event = new Event('DOMContentLoaded', { bubbles: true, cancelable: false });
        document.dispatchEvent(event);
    })();
"#);

        // Run event loop with bounded timeout (not infinite)
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(_) => return,
        };
        for _ in 0..EVENT_LOOP_MAX_POLLS {
            if terminated_clone.load(Ordering::Relaxed) {
                return;
            }
            let _ = rt.block_on(async {
                let _ = tokio::time::timeout(
                    Duration::from_millis(EVENT_LOOP_TIMEOUT_MS),
                    runtime.run_event_loop(PollEventLoopOptions::default()),
                )
                .await;
            });

            // Drain SSE events after each event loop poll
            {
                let op_state_rc = runtime.op_state();
                let state_rc = op_state_rc.borrow();
                if let Some(manager) = state_rc.try_borrow::<crate::sse::SseManager>() {
                    let sse_js = manager.drain_events_js();
                    if !sse_js.is_empty() {
                        drop(state_rc);
                        let _ = runtime.execute_script("sse_events.js", sse_js);
                    }
                }
            }
        }

        // Drain expired timers (delay=0 callbacks)
        {
            let op_state_rc = runtime.op_state();
            let state_rc = op_state_rc.borrow();
            if let Some(queue) = state_rc.try_borrow::<super::timer::TimerQueue>() {
                if !queue.is_at_limit() {
                    let timer_js = queue.get_expired_timer_callbacks_js();
                    if !timer_js.is_empty() {
                        drop(state_rc);
                        let _ = runtime.execute_script("timers.js", timer_js);
                        let op_state_mut = runtime.op_state();
                        let mut state_mut = op_state_mut.borrow_mut();
                        if let Some(queue_mut) = state_mut.try_borrow_mut::<super::timer::TimerQueue>() {
                            queue_mut.mark_delay_zero_fired();
                        }
                    }
                }
            }
        }

        // Serialize DOM back to HTML
        let output = dom.borrow().to_html();
        *result_clone.lock().unwrap_or_else(|e| e.into_inner()) = ThreadResult {
            dom_html: Some(output),
            error: None,
        };
    });

    // Wait for thread with timeout
    let start = Instant::now();
    loop {
        if handle.is_finished() {
            break;
        }
        if start.elapsed() >= Duration::from_millis(timeout_ms) {
            // Signal termination
            terminated.store(true, Ordering::SeqCst);
            eprintln!("[JS] Execution timed out after {}ms, waiting for thread to finish...", timeout_ms);

            // Give the thread a grace period to finish after termination signal
            let grace_start = Instant::now();
            loop {
                if handle.is_finished() {
                    break;
                }
                if grace_start.elapsed() >= Duration::from_millis(THREAD_JOIN_GRACE_MS) {
                    eprintln!("[JS] Thread did not finish within grace period, returning original HTML");
                    return None;
                }
                thread::sleep(Duration::from_millis(10));
            }
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    // One final check after the loop (fixes race condition where thread finishes between
    // is_finished() check and elapsed() check)
    let guard = result.lock().unwrap_or_else(|e| e.into_inner());
    guard.dom_html.clone()
}

// ==================== CDP Evaluate Result Types ====================

/// Result of evaluating a JavaScript expression.
/// Used by CDP Runtime domain for script evaluation.
#[derive(Debug)]
pub struct EvaluateResult {
    /// The type of the result (e.g., "string", "number", "boolean", "undefined", "object")
    pub r#type: String,
    /// The value as a JSON-serializable string
    pub value: String,
    /// Optional description
    pub description: Option<String>,
    /// Optional subtype (e.g., "null", "regexp", "promise")
    pub subtype: Option<String>,
    /// Exception details if an error occurred
    pub exception_details: Option<serde_json::Value>,
}

/// Evaluate a JavaScript expression in the context of an HTML page.
///
/// This is used by CDP Runtime domain to support Runtime.evaluate and Runtime.callFunctionOn.
pub fn evaluate_js_expression(
    _html: &str,
    _url: &str,
    _expression: &str,
    _await_promise: bool,
    _timeout_ms: u64,
) -> EvaluateResult {
    // Stub implementation - full JS evaluation requires proper V8 integration
    // which is done via execute_js() above for page-level script execution.
    // For expression evaluation (e.g., document.title), we would need:
    // 1. Parse the HTML
    // 2. Create a V8 context with DOM shims
    // 3. Execute the expression
    // 4. Serialize the result
    
    // For now, return a stub that indicates the expression was received
    // This allows CDP clients to connect without crashing
    EvaluateResult {
        r#type: "undefined".to_string(),
        value: "null".to_string(),
        description: Some("JS evaluation stub - full implementation requires V8 context".to_string()),
        subtype: None,
        exception_details: None,
    }
}

// ==================== Main Entry Point ====================

/// Execute all scripts in the given HTML and return the modified HTML.
///
/// This uses deno_core (V8) to execute JavaScript. We provide a minimal
/// `document` and `window` shim via ops that interact with the DOM.
///
/// Thread-based timeout ensures we don't hang on complex scripts.
#[must_use = "ignoring Result will silently discard JS execution errors"]
pub async fn execute_js(
    html: &str,
    base_url: &str,
    wait_ms: u32,
    sandbox: Option<&SandboxPolicy>,
) -> anyhow::Result<String> {
    let sandbox = sandbox.cloned().unwrap_or_default();

    // If JS is disabled by sandbox, return original HTML immediately
    if sandbox.js_mode == JsSandboxMode::Disabled {
        return Ok(html.to_string());
    }

    // Parse base URL
    let base = match Url::parse(base_url) {
        Ok(u) => u,
        Err(_) => return Ok(html.to_string()),
    };

    // Extract scripts from HTML (inline + external)
    let mut scripts = extract_scripts(html, &base);

    // Apply sandbox-configurable script limits
    if sandbox.js_max_scripts > 0 {
        scripts.truncate(sandbox.js_max_scripts);
    }
    if sandbox.js_max_script_size > 0 {
        scripts.retain(|s| s.code.len() <= sandbox.js_max_script_size);
    }

    // If no scripts, return original HTML
    if scripts.is_empty() {
        return Ok(html.to_string());
    }

    eprintln!(
        "[JS] Found {} inline script(s) to execute for {}",
        scripts.len(),
        base.as_str()
    );

    // Use sandbox-configurable timeout if specified
    let per_script_timeout = if sandbox.js_timeout_ms > 0 {
        sandbox.js_timeout_ms
    } else {
        SCRIPT_TIMEOUT_MS
    };

    // Calculate total timeout: per-script timeout * number of scripts, max 30s
    let total_timeout = ((scripts.len() as u64) * per_script_timeout).min(30_000);
    let timeout = total_timeout.max(wait_ms as u64);

    // Execute in a separate thread with timeout
    let result = execute_scripts_with_timeout(
        html.to_string(),
        base_url.to_string(),
        scripts,
        timeout,
        sandbox,
    );

    match result {
        Some(modified_html) => Ok(modified_html),
        None => {
            // Timeout or error - return original HTML
            Ok(html.to_string())
        }
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== extract_scripts Tests ====================

    #[test]
    fn test_extract_scripts_empty_html() {
        let scripts = extract_scripts("<html></html>", &Url::parse("https://example.com").unwrap());
        assert!(scripts.is_empty());
    }

    #[test]
    fn test_extract_scripts_no_scripts() {
        let html = r#"<html><body><p>Hello</p></body></html>"#;
        let scripts = extract_scripts(html, &Url::parse("https://example.com").unwrap());
        assert!(scripts.is_empty());
    }

    #[test]
    fn test_extract_scripts_simple_inline() {
        let html = r#"
            <html><body>
                <script>document.body.innerHTML = 'Hello';</script>
            </body></html>
        "#;
        let scripts = extract_scripts(html, &Url::parse("https://example.com").unwrap());
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0].name, "inline_script_0.js");
        assert!(scripts[0].code.contains("document.body.innerHTML"));
    }

    #[test]
    fn test_extract_scripts_multiple_scripts() {
        let html = r#"
            <html><body>
                <script>var a = 1;</script>
                <script>var b = 2;</script>
                <script>var c = 3;</script>
            </body></html>
        "#;
        let scripts = extract_scripts(html, &Url::parse("https://example.com").unwrap());
        assert_eq!(scripts.len(), 3);
    }

    #[test]
    fn test_extract_scripts_skips_external() {
        let html = r#"
            <html><body>
                <script src="external.js"></script>
                <script>inline code</script>
            </body></html>
        "#;
        let scripts = extract_scripts(html, &Url::parse("https://example.com").unwrap());
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].code.contains("inline code"));
    }

    #[test]
    fn test_extract_scripts_transforms_module() {
        let html = r#"
            <html><body>
                <script type="module">import { foo } from './bar.js';
export const x = 1;
export function hello() {}</script>
                <script>regular script</script>
            </body></html>
        "#;
        let scripts = extract_scripts(html, &Url::parse("https://example.com").unwrap());
        assert_eq!(scripts.len(), 2);
        assert!(scripts[0].code.contains("const x = 1;"));
        assert!(scripts[0].code.contains("function hello() {}"));
        assert!(!scripts[0].code.contains("import "));
        assert!(!scripts[0].code.contains("export "));
        assert!(scripts[1].code.contains("regular script"));
    }

    #[test]
    fn test_extract_scripts_skips_empty() {
        let html = r#"
            <html><body>
                <script></script>
                <script>   </script>
                <script>real code</script>
            </body></html>
        "#;
        let scripts = extract_scripts(html, &Url::parse("https://example.com").unwrap());
        assert_eq!(scripts.len(), 1);
    }

    #[test]
    fn test_extract_scripts_skips_large() {
        let large_code: String = "x".repeat(MAX_SCRIPT_SIZE + 1);
        let html = format!(
            r#"<html><body><script>{}</script></body></html>"#,
            large_code
        );
        let scripts = extract_scripts(&html, &Url::parse("https://example.com").unwrap());
        assert!(scripts.is_empty());
    }

    #[test]
    fn test_extract_scripts_limits_count() {
        let mut scripts_html = String::from("<html><body>");
        for i in 0..60 {
            scripts_html.push_str(&format!("<script>var a{} = {};</script>", i, i));
        }
        scripts_html.push_str("</body></html>");

        let scripts = extract_scripts(&scripts_html, &Url::parse("https://example.com").unwrap());
        assert_eq!(scripts.len(), MAX_SCRIPTS);
    }

    // ==================== is_analytics_script Tests ====================

    #[test]
    fn test_is_analytics_script_google() {
        assert!(is_analytics_script("gtag('event', 'click');"));
        assert!(is_analytics_script("ga('send', 'pageview');"));
        assert!(is_analytics_script("google-analytics.com/analytics.js"));
    }

    #[test]
    fn test_is_analytics_script_facebook_pixel() {
        assert!(is_analytics_script("fbq('track', 'PageView');"));
        assert!(is_analytics_script("facebook.com/tr?id=123"));
    }

    #[test]
    fn test_is_analytics_script_hotjar() {
        assert!(is_analytics_script("hj('trigger', 'button');"));
        assert!(is_analytics_script("hotjar.identify({userId: 123});"));
    }

    #[test]
    fn test_is_analytics_script_segment() {
        assert!(is_analytics_script("segment.com/analytics.js"));
        assert!(is_analytics_script("mixpanel.track('Event');"));
    }

    #[test]
    fn test_is_analytics_script_not_analytics() {
        assert!(!is_analytics_script("function doSomething() { return 1; }"));
        assert!(!is_analytics_script("const app = { name: 'MyApp' };"));
        assert!(!is_analytics_script("document.querySelector('.btn').click();"));
    }

    #[test]
    fn test_is_analytics_script_case_insensitive() {
        assert!(is_analytics_script("GOOGLE-ANALYTICS.com/script.js"));
        assert!(is_analytics_script("GTag('event');"));
        // Note: dataLayer becomes datalayer when lowercased, so test with lowercase
        assert!(is_analytics_script("dataLayer.push({});"));
    }

    #[test]
    fn test_is_analytics_script_googletagmanager() {
        assert!(is_analytics_script("googletagmanager.com/gtm.js"));
        assert!(is_analytics_script("gtm.js"));
        assert!(is_analytics_script("dataLayer.push({event: 'click'});"));
    }

    #[test]
    fn test_is_analytics_script_ads() {
        assert!(is_analytics_script("adsbygoogle.push({});"));
        assert!(is_analytics_script("doubleclick.net/ad.js"));
    }

    // ==================== execute_js Tests ====================

    #[tokio::test]
    async fn test_execute_js_no_scripts() {
        let html = "<html><body><p>Hello</p></body></html>";
        let result = execute_js(html, "https://example.com", 100, None).await.unwrap();
        assert_eq!(result, html);
    }

    #[tokio::test]
    async fn test_execute_js_invalid_url() {
        let html = "<html><body><p>Hello</p></body></html>";
        let result = execute_js(html, "not-a-url", 100, None).await.unwrap();
        assert_eq!(result, html);
    }

    #[tokio::test]
    async fn test_execute_js_with_analytics_skipped() {
        let html = r#"
            <html><body>
                <script>gtag('event', 'click');</script>
            </body></html>
        "#;
        let result = execute_js(html, "https://example.com", 100, None).await.unwrap();
        assert!(result.contains("<html>"));
    }

    // ==================== is_problematic_script Tests ====================

    #[test]
    fn test_is_problematic_script_infinite_loops() {
        assert!(is_problematic_script("while(true) { }"));
        assert!(is_problematic_script("while (true) { }"));
        assert!(is_problematic_script("for(;;) { }"));
        assert!(is_problematic_script("for (;;) { }"));
        assert!(is_problematic_script("while(1) { }"));
        assert!(is_problematic_script("while (1) { }"));
    }

    #[test]
    fn test_is_problematic_script_not_flagged() {
        // These are standard web APIs — they should NOT be flagged
        assert!(!is_problematic_script("element.addEventListener('click', handler)"));
        assert!(!is_problematic_script("setInterval(function() {}, 100)"));
        assert!(!is_problematic_script("requestAnimationFrame(render)"));
        assert!(!is_problematic_script("new MutationObserver(function() {})"));
    }

    #[test]
    fn test_is_problematic_script_destructive() {
        // These ARE destructive and should be flagged
        assert!(is_problematic_script("document.write('overwrites everything')"));
        assert!(is_problematic_script("document.writeln('content')"));
        assert!(is_problematic_script("eval('dynamic code')"));
    }

    #[test]
    fn test_frameworks_not_analytics() {
        // Framework names should NOT be treated as analytics
        assert!(!is_analytics_script("const React = require('react')"));
        assert!(!is_analytics_script("import Vue from 'vue'"));
        assert!(!is_analytics_script("angular.module('app')"));
        assert!(!is_analytics_script("window.__NEXT_DATA__ = {}"));
        assert!(!is_analytics_script("import('module')"));
        assert!(!is_analytics_script("__webpack_require__('main')"));
    }

    #[test]
    fn test_is_problematic_script_safe_code() {
        // These should NOT be flagged as problematic
        assert!(!is_problematic_script("function add(a, b) { return a + b; }"));
        assert!(!is_problematic_script("const x = 1;"));
        assert!(!is_problematic_script("document.body.innerHTML = 'Hello';"));
    }

    #[tokio::test]
    async fn test_execute_js_skips_problematic_scripts() {
        let html = r#"
            <html><body>
                <script>while(true) { }</script>
                <script>document.body.innerHTML = 'Safe';</script>
            </body></html>
        "#;
        let result = execute_js(html, "https://example.com", 100, None).await.unwrap();
        assert!(result.contains("Safe"));
    }
}
