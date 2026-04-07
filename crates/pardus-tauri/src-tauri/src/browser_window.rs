use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// JavaScript injected into every browser window page to add a navigation toolbar.
pub const BROWSER_TOOLBAR_JS: &str = r#"
(function() {
    if (window.__pardusToolbar) return;
    window.__pardusToolbar = true;

    var INSTANCE_ID = '__INSTANCE_ID__';

    function emit(name, data) {
        try { window.__TAURI__.event.emit(name, data); } catch(e) {
            try { window.__TAURI_INTERNALS__.postMessage({ cmd: 'event', event: name, payload: JSON.stringify(data) }); } catch(e2) {}
        }
    }

    function createToolbar() {
        var bar = document.createElement('div');
        bar.id = 'pardus-toolbar';
        bar.style.cssText = 'position:fixed;top:0;left:0;right:0;height:40px;z-index:2147483647;'
            + 'background:#161b22;border-bottom:1px solid #30363d;display:flex;align-items:center;'
            + 'padding:0 8px;gap:6px;font-family:-apple-system,BlinkMacSystemFont,sans-serif;';

        var btnStyle = 'background:#30363d;border:none;color:#e6edf3;border-radius:4px;padding:4px 10px;'
            + 'font-size:12px;cursor:pointer;height:28px;line-height:20px;';

        // Back button
        var back = document.createElement('button');
        back.textContent = '\u2190';
        back.style.cssText = btnStyle;
        back.onclick = function() { window.history.back(); };
        bar.appendChild(back);

        // Forward button
        var fwd = document.createElement('button');
        fwd.textContent = '\u2192';
        fwd.style.cssText = btnStyle;
        fwd.onclick = function() { window.history.forward(); };
        bar.appendChild(fwd);

        // Refresh button
        var ref = document.createElement('button');
        ref.textContent = '\u21BB';
        ref.style.cssText = btnStyle;
        ref.onclick = function() { window.location.reload(); };
        bar.appendChild(ref);

        // URL input
        var input = document.createElement('input');
        input.type = 'text';
        input.value = window.location.href;
        input.style.cssText = 'flex:1;height:28px;background:#0d1117;border:1px solid #30363d;'
            + 'border-radius:4px;color:#e6edf3;padding:0 8px;font-size:12px;'
            + 'font-family:\'SF Mono\',\'Cascadia Code\',monospace;outline:none;';
        input.onfocus = function() { input.select(); };
        input.onkeydown = function(e) {
            if (e.key === 'Enter') {
                var url = input.value.trim();
                if (url && !url.match(/^https?:\/\//)) url = 'https://' + url;
                emit('browser-navigate', { instance_id: INSTANCE_ID, url: url });
            }
        };
        bar.appendChild(input);

        // Update input on URL change
        var origPush = history.pushState;
        history.pushState = function() {
            origPush.apply(this, arguments);
            input.value = window.location.href;
            emit('browser-url-changed', { instance_id: INSTANCE_ID, url: window.location.href });
        };
        var origReplace = history.replaceState;
        history.replaceState = function() {
            origReplace.apply(this, arguments);
            input.value = window.location.href;
            emit('browser-url-changed', { instance_id: INSTANCE_ID, url: window.location.href });
        };
        window.addEventListener('popstate', function() {
            input.value = window.location.href;
            emit('browser-url-changed', { instance_id: INSTANCE_ID, url: window.location.href });
        });

        document.documentElement.appendChild(bar);
        document.body.style.paddingTop = '40px';

        emit('browser-url-changed', { instance_id: INSTANCE_ID, url: window.location.href });
    }

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', createToolbar);
    } else {
        createToolbar();
    }
})();
"#;

/// JavaScript injected into every browser window to intercept user interactions
/// and forward them to the Pardus headless browser via Tauri events.
pub const INTERACTION_INTERCEPTOR_JS: &str = r#"
(function() {
    if (window.__pardusInterceptor) return;
    window.__pardusInterceptor = true;

    var INSTANCE_ID = '__INSTANCE_ID__';
    var PENDING_NAVIGATE = null; // track if we're waiting for a headless navigation result

    function emit(name, data) {
        try { window.__TAURI__.event.emit(name, data); } catch(e) {
            try { window.__TAURI_INTERNALS__.postMessage({ cmd: 'event', event: name, payload: JSON.stringify(data) }); } catch(e2) {}
        }
    }

    // -------------------------------------------------------------------------
    // CSS selector generator — produces a unique selector for any element
    // -------------------------------------------------------------------------
    function getUniqueSelector(el) {
        if (el.id) return '#' + CSS.escape(el.id);

        var path = [];
        var current = el;
        while (current && current.nodeType === Node.ELEMENT_NODE) {
            var selector = current.tagName.toLowerCase();

            if (current.id) {
                selector = '#' + CSS.escape(current.id);
                path.unshift(selector);
                break;
            }

            // Use name attribute for form elements
            if (current.name && (current.tagName === 'INPUT' || current.tagName === 'SELECT' || current.tagName === 'TEXTAREA' || current.tagName === 'BUTTON')) {
                selector += '[name=' + CSS.escape(current.name) + ']';
                path.unshift(selector);
                break;
            }

            // Use type attribute for inputs
            if (current.type && current.tagName === 'INPUT') {
                selector += '[type=' + CSS.escape(current.type) + ']';
            }

            // nth-child to disambiguate siblings
            var parent = current.parentElement;
            if (parent) {
                var siblings = Array.prototype.filter.call(parent.children, function(s) {
                    return s.tagName === current.tagName;
                });
                if (siblings.length > 1) {
                    var index = siblings.indexOf(current) + 1;
                    selector += ':nth-of-type(' + index + ')';
                }
            }

            path.unshift(selector);
            current = current.parentElement;
        }

        return path.join(' > ');
    }

    // -------------------------------------------------------------------------
    // Determine the action type for an element
    // -------------------------------------------------------------------------
    function getActionForElement(el, eventType) {
        var tag = el.tagName.toUpperCase();

        // Links — always intercept navigation
        if (tag === 'A' && el.href) {
            return { action: 'click', href: el.href };
        }

        // Submit buttons
        if ((tag === 'BUTTON' || tag === 'INPUT') && el.type === 'submit') {
            return { action: 'click', href: null };
        }

        // Regular buttons
        if (tag === 'BUTTON') {
            return { action: 'click', href: null };
        }

        // Checkboxes and radios
        if (tag === 'INPUT' && (el.type === 'checkbox' || el.type === 'radio')) {
            return { action: 'toggle', href: null };
        }

        // Select changes
        if (tag === 'SELECT') {
            return { action: 'select', href: null };
        }

        // Generic click on any element with role="button" or tabindex
        if (el.getAttribute('role') === 'button' || el.getAttribute('role') === 'link') {
            return { action: 'click', href: el.href || null };
        }

        // Default: click
        return { action: 'click', href: null };
    }

    // -------------------------------------------------------------------------
    // Check if element is inside the Pardus toolbar or challenge banner
    // -------------------------------------------------------------------------
    function isPardusUI(el) {
        var id = el.id;
        if (id === 'pardus-toolbar' || id === 'pardus-challenge-banner') return true;
        var parent = el.parentElement;
        while (parent) {
            if (parent.id === 'pardus-toolbar' || parent.id === 'pardus-challenge-banner') return true;
            parent = parent.parentElement;
        }
        return false;
    }

    // -------------------------------------------------------------------------
    // Click interceptor
    // -------------------------------------------------------------------------
    document.addEventListener('click', function(e) {
        var el = e.target;

        // Walk up to find the nearest interactive element
        while (el && el !== document.body) {
            if (isPardusUI(el)) return; // let toolbar clicks pass through
            var tag = el.tagName;
            if (tag === 'A' || tag === 'BUTTON' || tag === 'INPUT' || tag === 'SELECT' ||
                tag === 'TEXTAREA' || el.getAttribute('role') === 'button' ||
                el.getAttribute('role') === 'link' || el.getAttribute('role') === 'tab') {
                break;
            }
            el = el.parentElement;
        }

        if (!el || el === document.body) return;
        if (isPardusUI(el)) return;

        var actionInfo = getActionForElement(el, 'click');
        var selector = getUniqueSelector(el);

        var payload = {
            instance_id: INSTANCE_ID,
            action: actionInfo.action,
            selector: selector,
            value: el.value || '',
            href: actionInfo.href || '',
            tag: el.tagName.toLowerCase(),
            text: (el.textContent || '').trim().substring(0, 200)
        };

        // For links and submit buttons, prevent the webview from navigating
        // The headless browser will handle navigation and we'll sync back
        if (actionInfo.href || (el.type === 'submit') || el.closest('form')) {
            e.preventDefault();
            e.stopPropagation();
            PENDING_NAVIGATE = true;
        }

        emit('browser-interaction', payload);
    }, true); // capture phase to intercept before the page handles it

    // -------------------------------------------------------------------------
    // Form input tracking — sync values to headless browser as user types
    // -------------------------------------------------------------------------
    var inputDebounce = null;
    document.addEventListener('input', function(e) {
        var el = e.target;
        if (!el || !el.tagName) return;
        var tag = el.tagName.toUpperCase();
        if (tag !== 'INPUT' && tag !== 'TEXTAREA') return;
        if (isPardusUI(el)) return;

        // Debounce to avoid flooding CDP commands
        clearTimeout(inputDebounce);
        inputDebounce = setTimeout(function() {
            var selector = getUniqueSelector(el);
            emit('browser-interaction', {
                instance_id: INSTANCE_ID,
                action: 'type',
                selector: selector,
                value: el.value || '',
                tag: el.tagName.toLowerCase(),
                input_type: el.type || 'text',
                href: '',
                text: ''
            });
        }, 300);
    }, true);

    // -------------------------------------------------------------------------
    // Select/checkbox/radio change tracking
    // -------------------------------------------------------------------------
    document.addEventListener('change', function(e) {
        var el = e.target;
        if (!el || !el.tagName) return;
        if (isPardusUI(el)) return;
        var tag = el.tagName.toUpperCase();

        var action = null;
        var value = '';

        if (tag === 'SELECT') {
            action = 'select';
            value = el.value;
        } else if (tag === 'INPUT' && (el.type === 'checkbox' || el.type === 'radio')) {
            action = 'toggle';
            value = el.checked ? 'true' : 'false';
        }

        if (!action) return;

        var selector = getUniqueSelector(el);
        emit('browser-interaction', {
            instance_id: INSTANCE_ID,
            action: action,
            selector: selector,
            value: value,
            tag: el.tagName.toLowerCase(),
            href: '',
            text: ''
        });
    }, true);

    // -------------------------------------------------------------------------
    // Form submission interceptor
    // -------------------------------------------------------------------------
    document.addEventListener('submit', function(e) {
        var form = e.target;
        if (!form || form.tagName.toUpperCase() !== 'FORM') return;
        if (isPardusUI(form)) return;

        e.preventDefault();
        e.stopPropagation();

        var selector = getUniqueSelector(form);
        emit('browser-interaction', {
            instance_id: INSTANCE_ID,
            action: 'submit',
            selector: selector,
            value: '',
            tag: 'form',
            href: form.action || '',
            text: ''
        });
    }, true);
})();
"#;

/// JavaScript injected when a CAPTCHA challenge is detected — adds an urgent banner.
pub const CHALLENGE_BANNER_JS: &str = r#"
(function() {
    if (window.__pardusChallengeBanner) return;
    window.__pardusChallengeBanner = true;

    var banner = document.createElement('div');
    banner.id = 'pardus-challenge-banner';
    banner.style.cssText = 'position:fixed;top:40px;left:0;right:0;z-index:2147483646;'
        + 'background:linear-gradient(135deg,#ff6b35,#f7931e);color:#fff;'
        + 'padding:10px 20px;font-family:system-ui,sans-serif;font-size:14px;font-weight:600;'
        + 'display:flex;align-items:center;justify-content:space-between;'
        + 'box-shadow:0 4px 12px rgba(0,0,0,0.3);';
    banner.innerHTML = '<span>\u26A0\uFE0F CAPTCHA DETECTED \u2014 Solve the challenge to let the agent continue</span>'
        + '<button onclick="this.parentElement.remove();document.body.style.paddingTop=\'40px\';" '
        + 'style="background:rgba(255,255,255,0.2);border:none;color:#fff;padding:4px 12px;'
        + 'border-radius:4px;cursor:pointer;font-size:12px;">Dismiss</button>';
    document.documentElement.appendChild(banner);
    document.body.style.paddingTop = '80px';
})();
"#;

/// Open a browser window for the given instance.
pub fn open_browser_window(
    app_handle: &AppHandle,
    instance_id: &str,
    url: &str,
) -> Result<String, String> {
    let label = format!("browser-{}", instance_id);

    // Close existing window if any
    if let Some(existing) = app_handle.get_webview_window(&label) {
        let _ = existing.close();
    }

    let parsed_url: url::Url = url.parse().map_err(|e: url::ParseError| e.to_string())?;

    let escaped_id = instance_id.replace('\\', "\\\\").replace('\'', "\\'");
    let toolbar_js = BROWSER_TOOLBAR_JS.replace("__INSTANCE_ID__", &escaped_id);
    let interceptor_js = INTERACTION_INTERCEPTOR_JS.replace("__INSTANCE_ID__", &escaped_id);

    let init_script = format!("{}\n{}", toolbar_js, interceptor_js);

    let _window = WebviewWindowBuilder::new(app_handle, &label, WebviewUrl::External(parsed_url))
        .title("Pardus Browser")
        .inner_size(1200.0, 800.0)
        .resizable(true)
        .initialization_script(&init_script)
        .user_agent(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like \
             Gecko) Version/18.0 Safari/605.1.15",
        )
        .build()
        .map_err(|e| e.to_string())?;

    Ok(label)
}

/// Close a browser window for the given instance.
pub fn close_browser_window(app_handle: &AppHandle, instance_id: &str) -> Result<(), String> {
    let label = format!("browser-{}", instance_id);
    if let Some(window) = app_handle.get_webview_window(&label) {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Inject the challenge banner into a browser window.
pub fn inject_challenge_banner(app_handle: &AppHandle, instance_id: &str) -> Result<(), String> {
    let label = format!("browser-{}", instance_id);
    if let Some(window) = app_handle.get_webview_window(&label) {
        window
            .eval(CHALLENGE_BANNER_JS)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
