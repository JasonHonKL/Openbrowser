//! JS-level interaction via deno_core DOM.
//!
//! When JS is enabled, interactions (click/type/scroll/submit) dispatch events
//! on the in-memory DOM, execute inline event handlers (onclick, onchange, etc.),
//! and return the modified HTML as a new page state.

use std::{cell::RefCell, rc::Rc, sync::Arc, thread, time::Duration};

use deno_core::*;
use parking_lot::{Condvar, Mutex};
use scraper::{Html, Selector};
use url::Url;

use crate::{
    interact::{actions::InteractionResult, form::FormState, scroll::ScrollDirection},
    js::{dom::DomDocument, extension::pardus_dom},
    session::SessionStore,
};

// ==================== Configuration ====================

const INTERACTION_TIMEOUT_MS: u64 = 5000;

// ==================== Inline Handler Registration ====================

/// JS script that extracts all inline on* attributes and registers them
/// as actual event listeners so they fire during dispatchEvent.
const INLINE_HANDLER_SCRIPT: &str = r#"
(function registerInlineHandlers() {
  var inlineAttrs = [
    'onclick','onchange','oninput','onsubmit','onfocus','onblur',
    'onkeydown','onkeyup','onkeypress','onmouseover','onmouseout','onscroll',
    'ondblclick','onmousedown','onmouseup','onresize','onload'
  ];
  var all = document.querySelectorAll('*');
  for (var i = 0; i < all.length; i++) {
    var el = all[i];
    for (var j = 0; j < inlineAttrs.length; j++) {
      var attr = inlineAttrs[j];
      var handler = el.getAttribute(attr);
      if (handler) {
        var eventType = attr.slice(2);
        (function(el, handler, eventType) {
          el.addEventListener(eventType, function(event) {
            try {
              (new Function('event', handler)).call(el, event);
            } catch(e) {}
          });
        })(el, handler, eventType);
      }
    }
  }
})();
"#;

// ==================== Thread-Based Execution ====================

/// Results from a JS interaction execution.
struct InteractionThreadResult {
    html: Option<String>,
    click_prevented: bool,
    href: Option<String>,
    /// Detected via Proxy setter on window.location in bootstrap.js
    navigation_href: Option<String>,
    /// Whether a submit handler called preventDefault
    submit_prevented: Option<bool>,
}

fn execute_interaction_inner(
    html: String,
    base_url: String,
    interaction_js: String,
    user_agent: String,
    session: Option<Arc<SessionStore>>,
) -> anyhow::Result<InteractionThreadResult> {
    let base = Url::parse(&base_url)?;

    let dom = Rc::new(RefCell::new(DomDocument::from_html(&html)));
    let mut runtime = create_interaction_runtime(dom.clone(), &base, &user_agent, session)?;

    // Bootstrap first — sets up window, document, etc.
    let bootstrap = include_str!("../js/bootstrap.js");
    runtime.execute_script("bootstrap.js", bootstrap)?;

    // Set up location and user agent after bootstrap.
    // We set individual properties on the existing Proxy to preserve the
    // navigation-detection setter. After populating the values, we clear the
    // navigation-href attribute so that only the *actual* interaction sets it.
    let location_js = format!(
        r#"
        window.location.href = "{}";
        window.location.origin = "{}";
        window.location.protocol = "{}";
        window.location.host = "{}";
        window.location.hostname = "{}";
        window.location.pathname = "{}";
        window.location.search = "{}";
        window.location.hash = "{}";
        globalThis.__pardusUserAgent = "{}";
        // Clear navigation marker set during location initialization
        var _docEl = document.documentElement;
        if (_docEl) _docEl.removeAttribute("data-pardus-navigation-href");
    "#,
        base.as_str(),
        base.origin().ascii_serialization(),
        base.scheme(),
        base.host_str().unwrap_or(""),
        base.host_str().unwrap_or(""),
        base.path(),
        base.query().unwrap_or(""),
        base.fragment().unwrap_or(""),
        user_agent
    );
    runtime.execute_script("location.js", location_js)?;

    // Register inline handlers
    let _ = runtime.execute_script("inline_handlers.js", INLINE_HANDLER_SCRIPT.to_string());

    // Run interaction
    let _ = runtime.execute_script("interaction.js", interaction_js);

    // Run event loop briefly
    let _ = runtime.run_event_loop(PollEventLoopOptions::default());

    // Read results from DomDocument data attributes (set by interaction JS and bootstrap Proxy)
    let dom_ref = dom.borrow();
    let doc_el = dom_ref.document_element();
    let click_prevented = dom_ref
        .get_attribute(doc_el, "data-pardus-click-prevented")
        .map(|v| v == "true")
        .unwrap_or(false);
    let href = dom_ref
        .get_attribute(doc_el, "data-pardus-clicked-href")
        .filter(|s| !s.is_empty());
    let navigation_href = dom_ref
        .get_attribute(doc_el, "data-pardus-navigation-href")
        .filter(|s| !s.is_empty());
    let submit_prevented = dom_ref
        .get_attribute(doc_el, "data-pardus-submit-prevented")
        .map(|v| v == "true");

    let output = dom_ref.to_html();
    Ok(InteractionThreadResult {
        html: Some(output),
        click_prevented,
        href,
        navigation_href,
        submit_prevented,
    })
}

/// Shared helper: create DomDocument from HTML, run bootstrap + inline handlers + interaction JS,
/// serialize DOM back to HTML.
///
/// Communication of results (href, click_prevented, navigation_href, submit_prevented) happens
/// via data attributes on the <html> element, which we read from the DomDocument after execution.
fn execute_interaction_thread(
    html: String,
    base_url: String,
    interaction_js: String,
    timeout_ms: u64,
    user_agent: String,
    session: Option<Arc<SessionStore>>,
) -> Option<InteractionThreadResult> {
    let lock = Arc::new(Mutex::new(InteractionThreadResult {
        html: None,
        click_prevented: false,
        href: None,
        navigation_href: None,
        submit_prevented: None,
    }));
    let cvar = Arc::new(Condvar::new());

    let lock_clone = lock.clone();
    let cvar_clone = cvar.clone();

    let _handle = thread::spawn(move || {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            execute_interaction_inner(html, base_url, interaction_js, user_agent, session)
        }));

        let output = match res {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                tracing::warn!("[js_interact] Error: {:#}", e);
                InteractionThreadResult {
                    html: None,
                    click_prevented: false,
                    href: None,
                    navigation_href: None,
                    submit_prevented: None,
                }
            }
            Err(panic_val) => {
                tracing::error!("[js_interact] Thread panicked: {:?}", panic_val);
                InteractionThreadResult {
                    html: None,
                    click_prevented: false,
                    href: None,
                    navigation_href: None,
                    submit_prevented: None,
                }
            }
        };
        *lock_clone.lock() = output;
        cvar_clone.notify_one();
    });

    let mut guard = lock.lock();
    let wait_result = cvar.wait_for(&mut guard, Duration::from_millis(timeout_ms));

    if wait_result.timed_out() {
        return None;
    }

    let ret = InteractionThreadResult {
        html: guard.html.clone(),
        click_prevented: guard.click_prevented,
        href: guard.href.clone(),
        navigation_href: guard.navigation_href.clone(),
        submit_prevented: guard.submit_prevented,
    };
    Some(ret)
}

fn create_interaction_runtime(
    dom: Rc<RefCell<DomDocument>>,
    base_url: &Url,
    _user_agent: &str,
    session: Option<Arc<SessionStore>>,
) -> anyhow::Result<JsRuntime> {
    let runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![pardus_dom::init()],
        ..Default::default()
    });

    runtime.op_state().borrow_mut().put(dom);

    if let Some(s) = session {
        runtime.op_state().borrow_mut().put(s);
    }
    runtime
        .op_state()
        .borrow_mut()
        .put(base_url.origin().ascii_serialization());

    Ok(runtime)
}

// ==================== Public API ====================

/// Perform a JS-level click on an element.
///
/// Dispatches a click event on the element found by `selector`.
/// If the element is a link (`<a href>`), resolves the href and performs
/// an HTTP GET unless the click handler prevented default.
/// If a handler sets `window.location.href`, detects and follows that.
/// Otherwise returns the modified DOM as a new Page.
pub async fn js_click(
    app: &Arc<crate::App>,
    page: &crate::Page,
    selector: &str,
) -> anyhow::Result<InteractionResult> {
    let html = page.html.html();
    let base_url = &page.base_url;

    // Verify element exists in the HTML first
    if let Ok(sel) = Selector::parse(selector) {
        let doc = Html::parse_document(&html);
        if doc.select(&sel).next().is_none() {
            return Ok(InteractionResult::ElementNotFound {
                selector: selector.to_string(),
                reason: "no element matches selector".to_string(),
            });
        }
    }

    let selector_json =
        serde_json::to_string(selector).unwrap_or_else(|_| format!("'{}'", selector));

    // Build interaction JS — writes results to data attributes on <html>
    let interaction_js = format!(
        r#"
        (function() {{
            var target = document.querySelector({selector_json});
            if (!target) return;

            // Store href for link handling via data attribute on documentElement
            var href = target.getAttribute('href') || '';
            var docEl = document.documentElement;
            if (docEl) {{
                docEl.setAttribute('data-pardus-clicked-href', href);
            }}

            // Dispatch click event
            var event = new Event('click', {{ bubbles: true, cancelable: true }});
            var notPrevented = target.dispatchEvent(event);

            if (docEl) {{
                docEl.setAttribute('data-pardus-click-prevented', String(!notPrevented));
            }}
        }})();
    "#,
        selector_json = selector_json,
    );

    let timeout = INTERACTION_TIMEOUT_MS;
    let user_agent = app.config.read().effective_user_agent().to_string();

    let thread_result = execute_interaction_thread(
        html,
        base_url.clone(),
        interaction_js,
        timeout,
        user_agent,
        None,
    );

    match thread_result {
        Some(result) => {
            let modified_html = match result.html {
                Some(h) => h,
                None => {
                    return Ok(InteractionResult::ElementNotFound {
                        selector: selector.to_string(),
                        reason: "JS execution failed".to_string(),
                    });
                }
            };

            // Check if a JS handler set window.location.href (via Proxy in bootstrap.js)
            if let Some(nav_href) = &result.navigation_href {
                if !result.click_prevented && !nav_href.starts_with('#') {
                    let resolved = Url::parse(base_url)
                        .and_then(|base| base.join(nav_href))
                        .map(|u| u.to_string())
                        .unwrap_or_else(|_| nav_href.clone());

                    let new_page = crate::Page::from_url(app, &resolved).await?;
                    return Ok(InteractionResult::Navigated(new_page));
                }
            }

            // If a link was clicked and default wasn't prevented, navigate via HTTP
            if let Some(href) = &result.href {
                if !result.click_prevented && !href.starts_with('#') {
                    let resolved = Url::parse(base_url)
                        .and_then(|base| base.join(href))
                        .map(|u| u.to_string())
                        .unwrap_or_else(|_| href.clone());

                    let new_page = crate::Page::from_url(app, &resolved).await?;
                    return Ok(InteractionResult::Navigated(new_page));
                }
            }

            // Otherwise return modified DOM as a new page
            let new_page = crate::Page::from_html(&modified_html, &page.url);
            Ok(InteractionResult::Navigated(new_page))
        }
        None => Ok(InteractionResult::ElementNotFound {
            selector: selector.to_string(),
            reason: "JS interaction timed out".to_string(),
        }),
    }
}

/// Perform a JS-level type into a form field.
///
/// Sets the value attribute on the element, dispatches input and change events,
/// and returns the modified DOM as a new Page so the caller can track DOM changes.
pub async fn js_type(
    page: &crate::Page,
    selector: &str,
    value: &str,
) -> anyhow::Result<InteractionResult> {
    let html = page.html.html();
    let base_url = &page.base_url;

    // Verify element exists
    if let Ok(sel) = Selector::parse(selector) {
        let doc = Html::parse_document(&html);
        if doc.select(&sel).next().is_none() {
            return Ok(InteractionResult::ElementNotFound {
                selector: selector.to_string(),
                reason: "no element matches selector".to_string(),
            });
        }
    }

    let selector_json =
        serde_json::to_string(selector).unwrap_or_else(|_| format!("'{}'", selector));
    let value_json = serde_json::to_string(value).unwrap_or_else(|_| format!("'{}'", value));

    let interaction_js = format!(
        r#"
        (function() {{
            var target = document.querySelector({selector_json});
            if (!target) return;

            // Set value attribute
            target.setAttribute('value', {value_json});

            // Dispatch input event
            var inputEvent = new Event('input', {{ bubbles: true }});
            target.dispatchEvent(inputEvent);

            // Dispatch change event
            var changeEvent = new Event('change', {{ bubbles: true }});
            target.dispatchEvent(changeEvent);
        }})();
    "#,
        selector_json = selector_json,
        value_json = value_json,
    );

    let thread_result = execute_interaction_thread(
        html,
        base_url.clone(),
        interaction_js,
        INTERACTION_TIMEOUT_MS,
        "PardusBrowser".to_string(),
        None,
    );

    match thread_result {
        Some(result) => {
            match result.html {
                Some(modified_html) => {
                    // Return the modified DOM so the caller can update page state
                    let new_page = crate::Page::from_html(&modified_html, &page.url);
                    Ok(InteractionResult::Navigated(new_page))
                }
                None => Ok(InteractionResult::ElementNotFound {
                    selector: selector.to_string(),
                    reason: "JS execution failed".to_string(),
                }),
            }
        }
        None => Ok(InteractionResult::ElementNotFound {
            selector: selector.to_string(),
            reason: "JS interaction timed out".to_string(),
        }),
    }
}

/// Perform a JS-level form submission.
///
/// Sets form field values from FormState, dispatches a submit event on the form.
/// If the handler calls `preventDefault`, returns the modified DOM without HTTP submission.
/// Otherwise falls through to HTTP-level form submission.
pub async fn js_submit(
    app: &Arc<crate::App>,
    page: &crate::Page,
    form_selector: &str,
    state: &FormState,
) -> anyhow::Result<InteractionResult> {
    let html = page.html.html();
    let base_url = &page.base_url;

    // Verify form exists
    if let Ok(sel) = Selector::parse(form_selector) {
        let doc = Html::parse_document(&html);
        if doc.select(&sel).next().is_none() {
            return Ok(InteractionResult::ElementNotFound {
                selector: form_selector.to_string(),
                reason: "no form matches selector".to_string(),
            });
        }
    }

    let selector_json =
        serde_json::to_string(form_selector).unwrap_or_else(|_| format!("'{}'", form_selector));

    // Build JS that sets form field values and dispatches submit event
    let mut field_setters = String::new();
    for (name, value) in state.entries() {
        let name_json = serde_json::to_string(name).unwrap_or_else(|_| format!("'{}'", name));
        let value_json = serde_json::to_string(value).unwrap_or_else(|_| format!("'{}'", value));
        field_setters.push_str(&format!(
            r#"
            var input = form.querySelector('[name=' + {name_json} + ']');
            if (input) input.setAttribute('value', {value_json});
"#,
            name_json = name_json,
            value_json = value_json,
        ));
    }

    let interaction_js = format!(
        r#"
        (function() {{
            var form = document.querySelector({selector_json});
            if (!form) return;

            // Set form field values from state
            {field_setters}

            // Dispatch submit event
            var event = new Event('submit', {{ bubbles: true, cancelable: true }});
            var notPrevented = form.dispatchEvent(event);

            var docEl = document.documentElement;
            if (docEl) {{
                docEl.setAttribute('data-pardus-submit-prevented', String(!notPrevented));
            }}
        }})();
    "#,
        selector_json = selector_json,
        field_setters = field_setters,
    );

    let thread_result = execute_interaction_thread(
        html,
        base_url.clone(),
        interaction_js,
        INTERACTION_TIMEOUT_MS,
        app.config.read().effective_user_agent().to_string(),
        None,
    );

    match thread_result {
        Some(result) => {
            let modified_html = match result.html {
                Some(h) => h,
                None => {
                    return Ok(InteractionResult::ElementNotFound {
                        selector: form_selector.to_string(),
                        reason: "JS execution failed".to_string(),
                    });
                }
            };

            // If handler prevented default, return modified DOM without HTTP submit
            if result.submit_prevented == Some(true) {
                let new_page = crate::Page::from_html(&modified_html, &page.url);
                return Ok(InteractionResult::Navigated(new_page));
            }

            // Check if handler navigated via window.location
            if let Some(nav_href) = &result.navigation_href {
                if !nav_href.starts_with('#') {
                    let resolved = Url::parse(base_url)
                        .and_then(|base| base.join(nav_href))
                        .map(|u| u.to_string())
                        .unwrap_or_else(|_| nav_href.clone());

                    let new_page = crate::Page::from_url(app, &resolved).await?;
                    return Ok(InteractionResult::Navigated(new_page));
                }
            }

            // Not prevented — fall through to HTTP-level form submission
            crate::interact::form::submit_form(app, page, form_selector, state).await
        }
        None => Ok(InteractionResult::ElementNotFound {
            selector: form_selector.to_string(),
            reason: "JS submit interaction timed out".to_string(),
        }),
    }
}

/// Perform a JS-level scroll.
///
/// Dispatches scroll events on the document and window.
/// Returns the modified DOM as a new page.
pub async fn js_scroll(
    page: &crate::Page,
    direction: ScrollDirection,
) -> anyhow::Result<InteractionResult> {
    let html = page.html.html();
    let base_url = &page.base_url;

    let delta_y = match direction {
        ScrollDirection::Down | ScrollDirection::ToBottom => 120,
        ScrollDirection::Up | ScrollDirection::ToTop => -120,
    };

    let scroll_js = format!(
        r#"
        (function() {{
            var scrollEvent = new Event('scroll', {{ bubbles: true }});
            document.dispatchEvent(scrollEvent);

            if (window.dispatchEvent) {{
                window.dispatchEvent(scrollEvent);
            }}

            // Dispatch a wheel event for direction-aware handlers
            var wheelEvent = new Event('wheel', {{ bubbles: true }});
            wheelEvent.deltaY = {delta_y};
            document.dispatchEvent(wheelEvent);
        }})();
    "#,
        delta_y = delta_y,
    );

    let thread_result = execute_interaction_thread(
        html,
        base_url.clone(),
        scroll_js,
        INTERACTION_TIMEOUT_MS,
        "PardusBrowser".to_string(),
        None,
    );

    match thread_result {
        Some(result) => {
            let modified_html = match result.html {
                Some(h) => h,
                None => {
                    return Ok(InteractionResult::ElementNotFound {
                        selector: String::new(),
                        reason: "JS execution failed".to_string(),
                    });
                }
            };

            let new_page = crate::Page::from_html(&modified_html, &page.url);
            Ok(InteractionResult::Navigated(new_page))
        }
        None => Ok(InteractionResult::ElementNotFound {
            selector: String::new(),
            reason: "JS scroll interaction timed out".to_string(),
        }),
    }
}

/// Dispatch an arbitrary DOM event on an element via the JS runtime.
///
/// Creates an `Event` (or `CustomEvent` if `detail` is provided in `event_init`)
/// and calls `dispatchEvent` on the the element found by `selector`.
/// Returns the modified DOM as a new page so the caller can inspect DOM changes.
pub async fn js_dispatch_event(
    page: &crate::Page,
    selector: &str,
    event_type: &str,
    event_init: Option<&str>,
) -> anyhow::Result<InteractionResult> {
    let html = page.html.html();
    let base_url = &page.base_url;

    // Verify element exists
    if let Ok(sel) = Selector::parse(selector) {
        let doc = Html::parse_document(&html);
        if doc.select(&sel).next().is_none() {
            return Ok(InteractionResult::ElementNotFound {
                selector: selector.to_string(),
                reason: "no element matches selector".to_string(),
            });
        }
    }

    let selector_json =
        serde_json::to_string(selector).unwrap_or_else(|_| format!("'{}'", selector));
    let event_type_json =
        serde_json::to_string(event_type).unwrap_or_else(|_| format!("'{}'", event_type));

    // Build the EventInit dictionary from optional JSON string
    let init_js = match event_init {
        Some(json) => {
            // If the init JSON contains a "detail" key, use CustomEvent
            let uses_custom = json.contains("\"detail\"");
            let constructor = if uses_custom { "CustomEvent" } else { "Event" };
            format!(
                "var initOpts = {}; try {{ initOpts = JSON.parse({}); }} catch(e) {{}} new {}({}, \
                 initOpts)",
                "{}",
                serde_json::to_string(json).unwrap_or_else(|_| "null".to_string()),
                constructor,
                event_type_json,
            )
        }
        None => format!("new Event({}, {{ bubbles: true }})", event_type_json),
    };

    let interaction_js = format!(
        r#"
        (function() {{
            var target = document.querySelector({selector_json});
            if (!target) return;
            var event = {init_js};
            target.dispatchEvent(event);
        }})();
    "#,
        selector_json = selector_json,
        init_js = init_js,
    );

    let thread_result = execute_interaction_thread(
        html,
        base_url.clone(),
        interaction_js,
        INTERACTION_TIMEOUT_MS,
        "PardusBrowser".to_string(),
        None,
    );

    match thread_result {
        Some(result) => match result.html {
            Some(modified_html) => {
                let new_page = crate::Page::from_html(&modified_html, &page.url);
                Ok(InteractionResult::Navigated(new_page))
            }
            None => Ok(InteractionResult::ElementNotFound {
                selector: selector.to_string(),
                reason: "JS execution failed".to_string(),
            }),
        },
        None => Ok(InteractionResult::EventDispatched {
            selector: selector.to_string(),
            event_type: event_type.to_string(),
        }),
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_page_html(body_content: &str) -> String {
        format!("<html><head></head><body>{}</body></html>", body_content)
    }

    fn run_interaction(html: &str, interaction_js: &str) -> Option<String> {
        let result = execute_interaction_thread(
            html.to_string(),
            "https://example.com".to_string(),
            interaction_js.to_string(),
            5000,
            "TestBot/1.0".to_string(),
            None,
        );
        result.and_then(|r| r.html)
    }

    // ==================== Click Tests ====================

    #[test]
    fn test_window_location_href_detection() {
        let html = test_page_html(
            r#"<button id="btn" onclick="document.getElementById('out').textContent='fired'; window.location.href='/new-page'">Go</button><span id="out">waiting</span>"#,
        );

        let interaction_js = r#"
            var btn = document.querySelector('#btn');
            if (btn) btn.click();
        "#;

        let result = execute_interaction_thread(
            html,
            "https://example.com".to_string(),
            interaction_js.to_string(),
            5000,
            "TestBot/1.0".to_string(),
            None,
        );

        assert!(result.is_some());
        let r = result.unwrap();
        tracing::debug!("[DEBUG] navigation_href: {:?}", r.navigation_href);
        tracing::debug!("[DEBUG] html: {:?}", r.html);
        assert_eq!(
            r.navigation_href.as_deref(),
            Some("/new-page"),
            "Expected navigation_href to be detected from window.location.href setter"
        );
    }

    #[test]
    fn test_location_assign_detection() {
        let html = test_page_html(
            r#"<button id="btn" onclick="location.assign('/assign-target')">Go</button>"#,
        );

        let interaction_js = r#"
            var btn = document.querySelector('#btn');
            if (btn) btn.click();
        "#;

        let result = execute_interaction_thread(
            html,
            "https://example.com".to_string(),
            interaction_js.to_string(),
            5000,
            "TestBot/1.0".to_string(),
            None,
        );

        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(
            r.navigation_href.as_deref(),
            Some("/assign-target"),
            "Expected navigation_href to be detected from location.assign()"
        );
    }

    #[test]
    fn test_location_replace_detection() {
        let html = test_page_html(
            r#"<button id="btn" onclick="location.replace('/replace-target')">Go</button>"#,
        );

        let interaction_js = r#"
            var btn = document.querySelector('#btn');
            if (btn) btn.click();
        "#;

        let result = execute_interaction_thread(
            html,
            "https://example.com".to_string(),
            interaction_js.to_string(),
            5000,
            "TestBot/1.0".to_string(),
            None,
        );

        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(
            r.navigation_href.as_deref(),
            Some("/replace-target"),
            "Expected navigation_href to be detected from location.replace()"
        );
    }

    #[test]
    fn test_location_reload_no_navigation() {
        let html = test_page_html(r#"<button id="btn" onclick="location.reload()">Go</button>"#);

        let interaction_js = r#"
            var btn = document.querySelector('#btn');
            if (btn) btn.click();
        "#;

        let result = execute_interaction_thread(
            html,
            "https://example.com".to_string(),
            interaction_js.to_string(),
            5000,
            "TestBot/1.0".to_string(),
            None,
        );

        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(
            r.navigation_href, None,
            "location.reload() should not trigger navigation detection"
        );
    }

    #[test]
    fn test_location_href_full_url_detection() {
        let html = test_page_html(
            r#"<script>window.location.href = 'https://other-site.com/path';</script>"#,
        );

        let interaction_js = "";

        let result = execute_interaction_thread(
            html,
            "https://example.com".to_string(),
            interaction_js.to_string(),
            5000,
            "TestBot/1.0".to_string(),
            None,
        );

        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(
            r.navigation_href.as_deref(),
            Some("https://other-site.com/path"),
            "Expected navigation_href to be detected from window.location.href in script tag"
        );
    }

    #[test]
    fn test_js_click_modifies_inner_html() {
        // Use textContent instead of innerHTML to avoid HTML parsing issues in attribute values
        let html = test_page_html(
            r#"<button id="btn" onclick="document.getElementById('output').textContent='Dynamic'"></button><div id="output"><p>Static</p></div>"#,
        );

        let interaction_js = r#"
            var btn = document.querySelector('#btn');
            if (btn) btn.click();
        "#;

        let result = run_interaction(&html, interaction_js);
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(
            output.contains("Dynamic"),
            "Expected 'Dynamic' in output, got: {}",
            output
        );
    }

    #[test]
    fn test_js_click_element_not_found() {
        let html = test_page_html("<div>content</div>");

        let interaction_js = r#"
            var target = document.querySelector('#nonexistent');
            // target is null, no action taken
        "#;

        let result = run_interaction(&html, interaction_js);
        assert!(result.is_some());
        assert!(result.unwrap().contains("content"));
    }

    #[test]
    fn test_js_click_link_default_not_prevented() {
        let html = test_page_html(r#"<a id="link" href="https://example.com/page2">Link</a>"#);

        let interaction_js = r#"
            var link = document.querySelector('#link');
            if (link) {
                var href = link.getAttribute('href') || '';
                var docEl = document.documentElement;
                if (docEl) {
                    docEl.setAttribute('data-pardus-clicked-href', href);
                }
                var event = new Event('click', { bubbles: true, cancelable: true });
                var notPrevented = link.dispatchEvent(event);
                if (docEl) {
                    docEl.setAttribute('data-pardus-click-prevented', String(!notPrevented));
                }
            }
        "#;

        let result = execute_interaction_thread(
            html,
            "https://example.com".to_string(),
            interaction_js.to_string(),
            5000,
            "TestBot/1.0".to_string(),
            None,
        );

        assert!(result.is_some());
        let r = result.unwrap();
        assert!(!r.click_prevented, "Click should not be prevented");
        assert_eq!(r.href.as_deref(), Some("https://example.com/page2"));
    }

    #[test]
    fn test_js_click_link_preventdefault_in_interaction_js() {
        let html = test_page_html(r#"<a id="link" href="https://example.com/page2">Link</a>"#);

        // Test that preventDefault works when called directly in interaction JS
        let interaction_js = r#"
            var link = document.querySelector('#link');
            if (link) {
                var href = link.getAttribute('href') || '';
                var docEl = document.documentElement;
                if (docEl) {
                    docEl.setAttribute('data-pardus-clicked-href', href);
                }
                var event = new Event('click', { bubbles: true, cancelable: true });
                // Call preventDefault on the event before dispatching
                event.preventDefault();
                var notPrevented = link.dispatchEvent(event);
                if (docEl) {
                    docEl.setAttribute('data-pardus-click-prevented', String(!notPrevented));
                }
            }
        "#;

        let result = execute_interaction_thread(
            html,
            "https://example.com".to_string(),
            interaction_js.to_string(),
            5000,
            "TestBot/1.0".to_string(),
            None,
        );

        assert!(result.is_some());
        let r = result.unwrap();
        assert!(
            r.click_prevented,
            "Click should be prevented when preventDefault is called before dispatch"
        );
    }

    // ==================== Type Tests ====================

    #[test]
    fn test_js_type_sets_value() {
        let html = test_page_html(r#"<input id="field" type="text" value="" />"#);

        let interaction_js = r#"
            var field = document.querySelector('#field');
            if (field) {
                field.setAttribute('value', 'hello world');
                field.dispatchEvent(new Event('input', { bubbles: true }));
                field.dispatchEvent(new Event('change', { bubbles: true }));
            }
        "#;

        let result = run_interaction(&html, interaction_js);
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(
            output.contains(r#"value="hello world""#),
            "Expected value attribute set, got: {}",
            output
        );
    }

    #[test]
    fn test_js_type_triggers_onchange() {
        let html = test_page_html(
            r#"<input id="field" type="text" onchange="document.getElementById('status').textContent='changed'" /><span id="status">unchanged</span>"#,
        );

        let interaction_js = r#"
            var field = document.querySelector('#field');
            if (field) {
                field.setAttribute('value', 'new value');
                field.dispatchEvent(new Event('input', { bubbles: true }));
                field.dispatchEvent(new Event('change', { bubbles: true }));
            }
        "#;

        let result = run_interaction(&html, interaction_js);
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(
            output.contains("changed"),
            "Expected 'changed' in output, got: {}",
            output
        );
    }

    #[test]
    fn test_js_type_triggers_oninput() {
        let html = test_page_html(
            r#"<input id="field" type="text" oninput="document.getElementById('mirror').textContent=this.getAttribute('value')" /><span id="mirror">empty</span>"#,
        );

        let interaction_js = r#"
            var field = document.querySelector('#field');
            if (field) {
                field.setAttribute('value', 'typed');
                field.dispatchEvent(new Event('input', { bubbles: true }));
            }
        "#;

        let result = run_interaction(&html, interaction_js);
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(
            output.contains("typed"),
            "Expected 'typed' in output, got: {}",
            output
        );
    }

    // ==================== Scroll Tests ====================

    #[test]
    fn test_js_scroll_dispatches_event() {
        let html = test_page_html(r#"<div id="log"></div>"#);

        let interaction_js = r#"
            document.addEventListener('scroll', function() {
                document.getElementById('log').textContent = 'scrolled';
            });
            var scrollEvent = new Event('scroll', { bubbles: true });
            document.dispatchEvent(scrollEvent);
        "#;

        let result = run_interaction(&html, interaction_js);
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(
            output.contains("scrolled"),
            "Expected 'scrolled' in output, got: {}",
            output
        );
    }

    #[test]
    fn test_js_scroll_triggers_onscroll() {
        let html = test_page_html(
            r#"<body onscroll="document.getElementById('log').textContent='scrolled'"><div id="log">not scrolled</div></body>"#,
        );

        let interaction_js = r#"
            var scrollEvent = new Event('scroll', { bubbles: true });
            document.dispatchEvent(scrollEvent);
        "#;

        let result = run_interaction(&html, interaction_js);
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(
            output.contains("scrolled"),
            "Expected 'scrolled' from inline handler, got: {}",
            output
        );
    }

    // ==================== Inline Handler Registration Tests ====================

    #[test]
    fn test_inline_onclick_registered_and_fires() {
        let html = test_page_html(
            r#"<button id="btn" onclick="document.getElementById('out').textContent='fired'"></button><span id="out">waiting</span>"#,
        );

        let interaction_js = r#"
            var btn = document.querySelector('#btn');
            if (btn) btn.click();
        "#;

        let result = run_interaction(&html, interaction_js);
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(
            output.contains("fired"),
            "Expected 'fired' from inline onclick, got: {}",
            output
        );
    }

    #[test]
    fn test_inline_onchange_registered_and_fires() {
        let html = test_page_html(
            r#"<input id="inp" type="text" onchange="document.getElementById('out').textContent='changed'" /><span id="out">waiting</span>"#,
        );

        let interaction_js = r#"
            var inp = document.querySelector('#inp');
            if (inp) {
                inp.setAttribute('value', 'test');
                inp.dispatchEvent(new Event('change', { bubbles: true }));
            }
        "#;

        let result = run_interaction(&html, interaction_js);
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(
            output.contains("changed"),
            "Expected 'changed' from inline onchange, got: {}",
            output
        );
    }

    #[test]
    fn test_multiple_inline_handlers() {
        let html = test_page_html(
            r#"<button id="btn1" onclick="document.getElementById('out').textContent='btn1'">1</button><button id="btn2" onclick="document.getElementById('out').textContent='btn2'">2</button><span id="out">none</span>"#,
        );

        let interaction_js = r#"
            var btn1 = document.querySelector('#btn1');
            if (btn1) btn1.click();
            var btn2 = document.querySelector('#btn2');
            if (btn2) btn2.click();
        "#;

        let result = run_interaction(&html, interaction_js);
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(
            output.contains("btn2"),
            "Expected 'btn2' from last click, got: {}",
            output
        );
    }

    // ==================== Navigation Detection Tests ====================

    #[test]
    fn test_window_location_href_detection_inline() {
        // Step 1: Test that inline handler with window.location.href fires
        let html = test_page_html(
            r#"<button id="btn" onclick="document.getElementById('out').textContent='fired'; window.location.href='/new-page'">Go</button><span id="out">waiting</span>"#,
        );

        let interaction_js = r#"
            var btn = document.querySelector('#btn');
            if (btn) btn.click();
        "#;

        let result = execute_interaction_thread(
            html,
            "https://example.com".to_string(),
            interaction_js.to_string(),
            5000,
            "TestBot/1.0".to_string(),
            None,
        );

        assert!(result.is_some());
        let r = result.unwrap();
        tracing::debug!("[DEBUG] navigation_href: {:?}", r.navigation_href);
        tracing::debug!("[DEBUG] html: {:?}", r.html);
        assert_eq!(
            r.navigation_href.as_deref(),
            Some("/new-page"),
            "Expected navigation_href to be detected from window.location.href setter"
        );
    }

    // ==================== Submit Tests ====================

    #[test]
    fn test_js_submit_prevented() {
        let html = test_page_html(
            r#"<form id="myform" onsubmit="event.preventDefault(); document.getElementById('log').textContent='prevented'"><input name="q" value="" /><button type="submit">Go</button></form><span id="log">waiting</span>"#,
        );

        let interaction_js = r#"
            var form = document.querySelector('#myform');
            if (form) {
                var event = new Event('submit', { bubbles: true, cancelable: true });
                var notPrevented = form.dispatchEvent(event);
                var docEl = document.documentElement;
                if (docEl) {
                    docEl.setAttribute('data-pardus-submit-prevented', String(!notPrevented));
                }
            }
        "#;

        let result = execute_interaction_thread(
            html,
            "https://example.com".to_string(),
            interaction_js.to_string(),
            5000,
            "TestBot/1.0".to_string(),
            None,
        );

        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.submit_prevented, Some(true), "Submit should be prevented");
        assert!(
            r.html.unwrap().contains("prevented"),
            "Handler should have modified DOM"
        );
    }

    // ==================== No-op / Edge Case Tests ====================

    #[test]
    fn test_click_on_element_without_handler() {
        let html = test_page_html(r#"<div id="target">Click me</div>"#);

        let interaction_js = r#"
            var target = document.querySelector('#target');
            if (target) target.click();
        "#;

        let result = run_interaction(&html, interaction_js);
        assert!(result.is_some());
        assert!(result.unwrap().contains("Click me"));
    }

    #[test]
    fn test_type_on_element_without_handler() {
        let html = test_page_html(r#"<input id="field" type="text" />"#);

        let interaction_js = r#"
            var field = document.querySelector('#field');
            if (field) {
                field.setAttribute('value', 'typed');
                field.dispatchEvent(new Event('input', { bubbles: true }));
                field.dispatchEvent(new Event('change', { bubbles: true }));
            }
        "#;

        let result = run_interaction(&html, interaction_js);
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(
            output.contains(r#"value="typed""#),
            "Expected value to be set, got: {}",
            output
        );
    }

    #[test]
    fn test_scroll_no_handler() {
        let html = test_page_html("<div>content</div>");

        let interaction_js = r#"
            var scrollEvent = new Event('scroll', { bubbles: true });
            document.dispatchEvent(scrollEvent);
        "#;

        let result = run_interaction(&html, interaction_js);
        assert!(result.is_some());
        assert!(result.unwrap().contains("content"));
    }

    // ==================== Event Dispatch Tests ====================

    #[tokio::test]
    async fn test_js_dispatch_change_event() {
        let page = crate::Page::from_html(
            r#"<html><head></head><body><input id="field" type="text" onchange="document.getElementById('out').textContent='changed'" /><span id="out">waiting</span></body></html>"#,
            "https://example.com",
        );

        let result = js_dispatch_event(&page, "#field", "change", None).await;
        assert!(result.is_ok());
        match result.unwrap() {
            InteractionResult::Navigated(new_page) => {
                let html = new_page.html.html();
                assert!(
                    html.contains("changed"),
                    "Expected 'changed' in output, got: {}",
                    html
                );
            }
            other => panic!("Expected Navigated, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_js_dispatch_focus_event() {
        let page = crate::Page::from_html(
            r#"<html><head></head><body><input id="field" type="text" onfocus="document.getElementById('out').textContent='focused'" /><span id="out">blurred</span></body></html>"#,
            "https://example.com",
        );

        let result = js_dispatch_event(&page, "#field", "focus", None).await;
        assert!(result.is_ok());
        match result.unwrap() {
            InteractionResult::Navigated(new_page) => {
                let html = new_page.html.html();
                assert!(
                    html.contains("focused"),
                    "Expected 'focused' in output, got: {}",
                    html
                );
            }
            other => panic!("Expected Navigated, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_js_dispatch_custom_event() {
        let page = crate::Page::from_html(
            r#"<html><head></head><body><div id="target"></div><script>document.getElementById('target').addEventListener('myevent', function(e) { document.getElementById('target').textContent = e.detail; });</script><span id="out">waiting</span></body></html>"#,
            "https://example.com",
        );

        let init = r#"{"bubbles":true,"detail":"hello from custom"}"#;
        let result = js_dispatch_event(&page, "#target", "myevent", Some(init)).await;
        assert!(result.is_ok());
        match result.unwrap() {
            InteractionResult::Navigated(new_page) => {
                let html = new_page.html.html();
                assert!(
                    html.contains("hello from custom"),
                    "Expected custom event detail in output, got: {}",
                    html
                );
            }
            other => panic!("Expected Navigated, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_js_dispatch_event_element_not_found() {
        let page = crate::Page::from_html(
            "<html><head></head><body><div>content</div></body></html>",
            "https://example.com",
        );

        let result = js_dispatch_event(&page, "#nonexistent", "click", None).await;
        assert!(result.is_ok());
        match result.unwrap() {
            InteractionResult::ElementNotFound { selector, reason } => {
                assert_eq!(selector, "#nonexistent");
                assert!(reason.contains("no element matches"));
            }
            other => panic!("Expected ElementNotFound, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_js_dispatch_event_with_init_options() {
        let page = crate::Page::from_html(
            r#"<html><head></head><body><input id="field" type="text" onblur="document.getElementById('out').textContent='blurred'" /><span id="out">waiting</span></body></html>"#,
            "https://example.com",
        );

        let init = r#"{"bubbles":true,"cancelable":true}"#;
        let result = js_dispatch_event(&page, "#field", "blur", Some(init)).await;
        assert!(result.is_ok());
        match result.unwrap() {
            InteractionResult::Navigated(new_page) => {
                let html = new_page.html.html();
                assert!(
                    html.contains("blurred"),
                    "Expected 'blurred' in output, got: {}",
                    html
                );
            }
            other => panic!("Expected Navigated, got: {:?}", other),
        }
    }
}
