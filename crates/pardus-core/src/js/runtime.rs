//! JavaScript execution runtime.
//!
//! Uses deno_core (V8) to execute JavaScript with thread-based timeouts.
//! Provides a minimal `document` and `window` shim via ops that interact with the DOM.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use deno_core::*;
use scraper::{Html, Selector};
use url::Url;

use super::dom::DomDocument;
use super::extension::pardus_dom;

// ==================== Configuration ====================

const SCRIPT_TIMEOUT_MS: u64 = 2000; // 2s per script
const MAX_SCRIPT_SIZE: usize = 100_000; // 100KB
const MAX_SCRIPTS: usize = 50;

/// Analytics/tracking patterns to skip (all lowercase for case-insensitive matching)
const ANALYTICS_PATTERNS: &[&str] = &[
    "google-analytics",
    "gtag(",
    "ga('",
    "gtag('",
    "facebook.com/tr",
    "fbq(",
    "fbq('",
    "hotjar",
    "hj(",
    "hj('",
    "mixpanel",
    "amplitude",
    "segment.com",
    "datalayer", // was dataLayer, but we lowercase the input
    "gtm.js",
    "googletagmanager",
    "adsbygoogle",
    "ads.js",
    "doubleclick",
    "newrelic",
    "nrqueue",
    "fullstory",
    "intercom",
    "zendesk",
    "helpscout",
    "heap.io",
    "logrocket",
];

// ==================== Script Extraction ====================

#[derive(Debug, Clone)]
struct ScriptInfo {
    name: String,
    code: String,
}

/// Extract inline scripts from HTML, filtering out analytics/tracking.
fn extract_scripts(html: &str) -> Vec<ScriptInfo> {
    let doc = Html::parse_document(html);
    let selector = match Selector::parse("script") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    doc.select(&selector)
        .enumerate()
        .filter_map(|(i, el)| {
            // Skip module scripts
            if el.value().attr("type") == Some("module") {
                return None;
            }

            // Skip external scripts (src attribute)
            if el.value().attr("src").is_some() {
                return None;
            }

            let code = el.text().collect::<String>();

            // Skip empty scripts
            if code.trim().is_empty() {
                return None;
            }

            // Skip large scripts
            if code.len() > MAX_SCRIPT_SIZE {
                return None;
            }

            // Skip analytics/tracking scripts
            if is_analytics_script(&code) {
                return None;
            }

            Some(ScriptInfo {
                name: format!("inline_script_{}.js", i),
                code,
            })
        })
        .take(MAX_SCRIPTS)
        .collect()
}

fn is_analytics_script(code: &str) -> bool {
    let lower = code.to_lowercase();
    ANALYTICS_PATTERNS.iter().any(|p| lower.contains(p))
}

// ==================== Runtime Creation ====================

/// Create a deno runtime with our DOM extension.
fn create_runtime(dom: Rc<RefCell<DomDocument>>, base_url: &Url) -> anyhow::Result<JsRuntime> {
    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![pardus_dom::init()],
        ..Default::default()
    });

    // Store DOM in op state
    runtime.op_state().borrow_mut().put(dom);

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
    error: Option<String>,
}

/// Execute scripts in a separate thread with timeout.
fn execute_scripts_with_timeout(
    html: String,
    base_url: String,
    scripts: Vec<ScriptInfo>,
    timeout_ms: u64,
) -> Option<String> {
    let result = Arc::new(Mutex::new(ThreadResult {
        dom_html: None,
        error: None,
    }));
    let result_clone = result.clone();

    let handle = thread::spawn(move || {
        // Parse base URL
        let base = match Url::parse(&base_url) {
            Ok(u) => u,
            Err(e) => {
                *result_clone.lock().unwrap() = ThreadResult {
                    dom_html: None,
                    error: Some(format!("Invalid base URL: {}", e)),
                };
                return;
            }
        };

        // Create DOM from HTML
        let dom = Rc::new(RefCell::new(DomDocument::from_html(&html)));

        // Create runtime
        let mut runtime = match create_runtime(dom.clone(), &base) {
            Ok(r) => r,
            Err(e) => {
                *result_clone.lock().unwrap() = ThreadResult {
                    dom_html: None,
                    error: Some(format!("Failed to create runtime: {}", e)),
                };
                return;
            }
        };

        // Execute bootstrap.js
        let bootstrap = include_str!("bootstrap.js");
        if let Err(e) = runtime.execute_script("bootstrap.js", bootstrap) {
            *result_clone.lock().unwrap() = ThreadResult {
                dom_html: None,
                error: Some(format!("Bootstrap error: {}", e)),
            };
            return;
        }

        // Execute each script
        for script in scripts {
            if let Err(e) = runtime.execute_script(script.name.clone(), script.code) {
                // Log error but continue with next script
                eprintln!("[JS] Script {} error: {}", script.name, e);
            }
        }

        // Run event loop briefly for any pending async ops
        let _ = runtime.run_event_loop(PollEventLoopOptions::default());

        // Serialize DOM back to HTML
        let output = dom.borrow().to_html();
        *result_clone.lock().unwrap() = ThreadResult {
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
            // Timeout - return original HTML
            eprintln!("[JS] Execution timed out after {}ms", timeout_ms);
            return None;
        }
        thread::sleep(Duration::from_millis(10));
    }

    let guard = result.lock().unwrap();
    guard.dom_html.clone()
}

// ==================== Main Entry Point ====================

/// Execute all scripts in the given HTML and return the modified HTML.
///
/// This uses deno_core (V8) to execute JavaScript. We provide a minimal
/// `document` and `window` shim via ops that interact with the DOM.
///
/// Thread-based timeout ensures we don't hang on complex scripts.
pub async fn execute_js(html: &str, base_url: &str, wait_ms: u32) -> anyhow::Result<String> {
    // Parse base URL
    let base = match Url::parse(base_url) {
        Ok(u) => u,
        Err(_) => return Ok(html.to_string()),
    };

    // Extract scripts from HTML
    let scripts = extract_scripts(html);

    // If no scripts, return original HTML
    if scripts.is_empty() {
        return Ok(html.to_string());
    }

    eprintln!(
        "[JS] Found {} inline script(s) to execute for {}",
        scripts.len(),
        base.as_str()
    );

    // Calculate total timeout: per-script timeout * number of scripts, max 30s
    let total_timeout = ((scripts.len() as u64) * SCRIPT_TIMEOUT_MS).min(30_000);
    let timeout = total_timeout.max(wait_ms as u64);

    // Execute in a separate thread with timeout
    let result = execute_scripts_with_timeout(html.to_string(), base_url.to_string(), scripts, timeout);

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
        let scripts = extract_scripts("<html></html>");
        assert!(scripts.is_empty());
    }

    #[test]
    fn test_extract_scripts_no_scripts() {
        let html = r#"<html><body><p>Hello</p></body></html>"#;
        let scripts = extract_scripts(html);
        assert!(scripts.is_empty());
    }

    #[test]
    fn test_extract_scripts_simple_inline() {
        let html = r#"
            <html><body>
                <script>document.body.innerHTML = 'Hello';</script>
            </body></html>
        "#;
        let scripts = extract_scripts(html);
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
        let scripts = extract_scripts(html);
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
        let scripts = extract_scripts(html);
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].code.contains("inline code"));
    }

    #[test]
    fn test_extract_scripts_skips_module() {
        let html = r#"
            <html><body>
                <script type="module">import { foo } from './bar.js';</script>
                <script>regular script</script>
            </body></html>
        "#;
        let scripts = extract_scripts(html);
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].code.contains("regular script"));
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
        let scripts = extract_scripts(html);
        assert_eq!(scripts.len(), 1);
    }

    #[test]
    fn test_extract_scripts_skips_large() {
        let large_code: String = "x".repeat(MAX_SCRIPT_SIZE + 1);
        let html = format!(
            r#"<html><body><script>{}</script></body></html>"#,
            large_code
        );
        let scripts = extract_scripts(&html);
        assert!(scripts.is_empty());
    }

    #[test]
    fn test_extract_scripts_limits_count() {
        let mut scripts_html = String::from("<html><body>");
        for i in 0..60 {
            scripts_html.push_str(&format!("<script>var a{} = {};</script>", i, i));
        }
        scripts_html.push_str("</body></html>");

        let scripts = extract_scripts(&scripts_html);
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
        let result = execute_js(html, "https://example.com", 100).await.unwrap();
        assert_eq!(result, html);
    }

    #[tokio::test]
    async fn test_execute_js_invalid_url() {
        let html = "<html><body><p>Hello</p></body></html>";
        let result = execute_js(html, "not-a-url", 100).await.unwrap();
        assert_eq!(result, html);
    }

    #[tokio::test]
    async fn test_execute_js_with_analytics_skipped() {
        let html = r#"
            <html><body>
                <script>gtag('event', 'click');</script>
            </body></html>
        "#;
        let result = execute_js(html, "https://example.com", 100).await.unwrap();
        assert!(result.contains("<html>"));
    }
}
