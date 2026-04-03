use scraper::{Selector, ElementRef};
use std::sync::Arc;
use url::Url;

use crate::app::App;
use crate::page::Page;
use super::element::ElementHandle;
use super::form::FormState;

/// Result of performing an interaction on a page.
pub enum InteractionResult {
    /// Navigation produced a new page (click on link, form submission).
    Navigated(Page),
    /// Typed value into a form field (no HTTP request, local state update).
    Typed {
        selector: String,
        value: String,
    },
    /// Toggled a checkbox/radio (local state).
    Toggled {
        selector: String,
        checked: bool,
    },
    /// Selected an option (local state).
    Selected {
        selector: String,
        value: String,
    },
    /// Element not found or not interactable.
    ElementNotFound {
        selector: String,
        reason: String,
    },
    /// Wait condition satisfied.
    WaitSatisfied {
        selector: String,
        found: bool,
    },
    /// Scroll loaded new content.
    Scrolled {
        url: String,
        page: Page,
    },
}

/// Type text into a form field.
///
/// Returns `InteractionResult::Typed` with the selector and value.
/// The caller should accumulate these into a `FormState` before submitting.
pub fn type_text(
    _page: &Page,
    handle: &ElementHandle,
    value: &str,
) -> anyhow::Result<InteractionResult> {
    if handle.is_disabled {
        return Ok(InteractionResult::ElementNotFound {
            selector: handle.selector.clone(),
            reason: "element is disabled".to_string(),
        });
    }

    // Verify element exists and is fillable
    if let Some(action) = &handle.action {
        if action != "fill" && action != "select" {
            return Ok(InteractionResult::ElementNotFound {
                selector: handle.selector.clone(),
                reason: format!("element action is '{}', not fillable", action),
            });
        }
    } else {
        return Ok(InteractionResult::ElementNotFound {
            selector: handle.selector.clone(),
            reason: "element has no action".to_string(),
        });
    }

    Ok(InteractionResult::Typed {
        selector: handle.selector.clone(),
        value: value.to_string(),
    })
}

/// Toggle a checkbox or radio.
pub fn toggle(
    page: &Page,
    handle: &ElementHandle,
) -> anyhow::Result<InteractionResult> {
    if handle.is_disabled {
        return Ok(InteractionResult::ElementNotFound {
            selector: handle.selector.clone(),
            reason: "element is disabled".to_string(),
        });
    }

    if handle.action.as_deref() != Some("toggle") {
        return Ok(InteractionResult::ElementNotFound {
            selector: handle.selector.clone(),
            reason: "element is not a toggle (checkbox/radio)".to_string(),
        });
    }

    // Check current state from HTML
    let checked = is_checked(page, &handle.selector);

    Ok(InteractionResult::Toggled {
        selector: handle.selector.clone(),
        checked: !checked, // Toggle the state
    })
}

/// Select an option in a <select> element.
pub fn select_option(
    page: &Page,
    handle: &ElementHandle,
    value: &str,
) -> anyhow::Result<InteractionResult> {
    if handle.is_disabled {
        return Ok(InteractionResult::ElementNotFound {
            selector: handle.selector.clone(),
            reason: "element is disabled".to_string(),
        });
    }

    if handle.action.as_deref() != Some("select") {
        return Ok(InteractionResult::ElementNotFound {
            selector: handle.selector.clone(),
            reason: "element is not a <select>".to_string(),
        });
    }

    // Verify the option value exists
    if let Ok(sel) = Selector::parse(&handle.selector) {
        if let Some(el) = page.html.select(&sel).next() {
            if let Ok(opt_sel) = Selector::parse("option") {
                let valid_values: Vec<String> = el
                    .select(&opt_sel)
                    .filter_map(|o| o.value().attr("value").map(|v| v.to_string()))
                    .collect();

                if !valid_values.is_empty() && !valid_values.contains(&value.to_string()) {
                    return Ok(InteractionResult::ElementNotFound {
                        selector: handle.selector.clone(),
                        reason: format!(
                            "option '{}' not found. Valid options: {:?}",
                            value, valid_values
                        ),
                    });
                }
            }
        }
    }

    Ok(InteractionResult::Selected {
        selector: handle.selector.clone(),
        value: value.to_string(),
    })
}

/// Click on an element.
///
/// - Links: Follow href via HTTP GET
/// - Submit buttons: Submit the associated form using accumulated form_state
/// - Other buttons: Returns ElementNotFound (no JS execution)
pub async fn click(
    app: &Arc<App>,
    page: &Page,
    handle: &ElementHandle,
    form_state: &FormState,
) -> anyhow::Result<InteractionResult> {
    if handle.is_disabled {
        return Ok(InteractionResult::ElementNotFound {
            selector: handle.selector.clone(),
            reason: "element is disabled".to_string(),
        });
    }

    match handle.action.as_deref() {
        Some("navigate") => click_link(app, page, handle).await,
        Some("click") => click_button(app, page, handle, form_state).await,
        Some(action) => Ok(InteractionResult::ElementNotFound {
            selector: handle.selector.clone(),
            reason: format!("element action '{}' is not clickable", action),
        }),
        None => Ok(InteractionResult::ElementNotFound {
            selector: handle.selector.clone(),
            reason: "element has no action".to_string(),
        }),
    }
}

async fn click_link(
    app: &Arc<App>,
    page: &Page,
    handle: &ElementHandle,
) -> anyhow::Result<InteractionResult> {
    let href = match &handle.href {
        Some(h) => h,
        None => {
            return Ok(InteractionResult::ElementNotFound {
                selector: handle.selector.clone(),
                reason: "link has no href".to_string(),
            });
        }
    };

    let resolved = Url::parse(&page.base_url)
        .and_then(|base| base.join(href))
        .map(|u| u.to_string())
        .unwrap_or_else(|_| href.clone());

    // CSP: check navigate-to directive
    if let Some(ref csp) = page.csp {
        if let Ok(resolved_url) = Url::parse(&resolved) {
            if let Ok(base_url) = Url::parse(&page.base_url) {
                let origin = base_url.origin();
                let check = csp.check_navigation(&origin, &resolved_url);
                if !check.allowed {
                    if let Some(ref directive) = check.violated_directive {
                        crate::csp::report_violation(&crate::csp::CspViolation {
                            document_uri: page.url.clone(),
                            blocked_uri: resolved.clone(),
                            effective_directive: directive.clone(),
                            original_policy: String::new(),
                            disposition: crate::csp::Disposition::Enforce,
                            status_code: page.status,
                        });
                    }
                    anyhow::bail!(
                        "Navigation to '{}' blocked by CSP navigate-to",
                        resolved
                    );
                }
            }
        }
    }

    let new_page = Page::from_url(app, &resolved).await?;
    Ok(InteractionResult::Navigated(new_page))
}

async fn click_button(
    app: &Arc<App>,
    page: &Page,
    handle: &ElementHandle,
    form_state: &FormState,
) -> anyhow::Result<InteractionResult> {
    // Find the element in the HTML
    let el = match Selector::parse(&handle.selector)
        .ok()
        .and_then(|sel| page.html.select(&sel).next())
    {
        Some(el) => el,
        None => {
            return Ok(InteractionResult::ElementNotFound {
                selector: handle.selector.clone(),
                reason: "element not found in DOM".to_string(),
            });
        }
    };

    // Find enclosing form
    let form_selector = find_enclosing_form(&el);
    match form_selector {
        Some(form_sel) => {
            super::form::submit_form(app, page, &form_sel, form_state).await
        }
        None => Ok(InteractionResult::ElementNotFound {
            selector: handle.selector.clone(),
            reason: "button has no enclosing form and no JS execution".to_string(),
        }),
    }
}

/// Walk up the DOM tree to find an enclosing <form> element.
/// Returns a CSS selector for the form.
fn find_enclosing_form(el: &ElementRef) -> Option<String> {
    let mut current = el.parent().and_then(ElementRef::wrap);
    while let Some(parent) = current {
        if parent.value().name() == "form" {
            let selector = if let Some(id) = parent.value().attr("id") {
                format!("#{}", id)
            } else if let Some(action) = parent.value().attr("action") {
                format!("form[action=\"{}\"]", action)
            } else {
                "form".to_string()
            };
            return Some(selector);
        }
        current = parent.parent().and_then(ElementRef::wrap);
    }
    None
}

/// Check if a checkbox/radio is currently checked.
fn is_checked(page: &Page, selector: &str) -> bool {
    Selector::parse(selector)
        .ok()
        .and_then(|sel| page.html.select(&sel).next())
        .map(|el| el.value().attr("checked").is_some())
        .unwrap_or(false)
}
