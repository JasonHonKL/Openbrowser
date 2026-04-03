use scraper::Selector;
use std::collections::HashMap;
use std::sync::Arc;
use url::Url;

use crate::app::App;
use crate::page::Page;
use crate::navigation::graph::FormDescriptor;
use super::actions::InteractionResult;

/// Accumulated form field values, keyed by field name.
/// Built up via type() calls, then submitted via submit_form().
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct FormState {
    fields: HashMap<String, String>,
}

impl FormState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a field value.
    pub fn set(&mut self, name: &str, value: &str) {
        self.fields.insert(name.to_string(), value.to_string());
    }

    /// Remove a field value.
    pub fn remove(&mut self, name: &str) {
        self.fields.remove(name);
    }

    /// Get a field value.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(|s| s.as_str())
    }

    /// Get all field entries.
    pub fn entries(&self) -> impl Iterator<Item = (&String, &String)> {
        self.fields.iter()
    }

    /// Check if a field is set.
    pub fn contains(&self, name: &str) -> bool {
        self.fields.contains_key(name)
    }

    /// Create a FormState pre-populated from a FormDescriptor.
    pub fn from_descriptor(_descriptor: &FormDescriptor) -> Self {
        Self::new()
    }

    /// Set a field by name (preferred over apply_result).
    pub fn apply_typed(&mut self, name: &str, value: &str) {
        self.set(name, value);
    }

    /// Apply a toggle by name.
    pub fn apply_toggle(&mut self, name: &str, value: &str, checked: bool) {
        if checked {
            self.set(name, value);
        } else {
            self.remove(name);
        }
    }
}

/// Submit a form with the given field values.
///
/// Collects ALL form fields from the HTML (including hidden inputs like CSRF tokens),
/// then overrides with user-provided FormState values.
pub async fn submit_form(
    app: &Arc<App>,
    page: &Page,
    form_selector: &str,
    state: &FormState,
) -> anyhow::Result<InteractionResult> {
    // Find the form element
    let form_el = match Selector::parse(form_selector)
        .ok()
        .and_then(|sel| page.html.select(&sel).next())
    {
        Some(el) => el,
        None => {
            return Ok(InteractionResult::ElementNotFound {
                selector: form_selector.to_string(),
                reason: "form not found in DOM".to_string(),
            });
        }
    };

    // Get form action and method
    let action = form_el
        .value()
        .attr("action")
        .unwrap_or(&page.url);
    let method = form_el
        .value()
        .attr("method")
        .unwrap_or("GET")
        .to_uppercase();

    // Resolve action URL
    let action_url = Url::parse(&page.base_url)
        .and_then(|base| base.join(action))
        .map(|u| u.to_string())
        .unwrap_or_else(|_| action.to_string());

    // Validate action URL against security policy (SSRF protection)
    app.validate_url(&action_url)?;

    // CSP: check form-action directive
    if let Some(ref csp) = page.csp {
        if let Ok(action_parsed) = Url::parse(&action_url) {
            if let Ok(base_parsed) = Url::parse(&page.base_url) {
                let origin = base_parsed.origin();
                let check = csp.check_form_action(&origin, &action_parsed);
                if !check.allowed {
                    if let Some(ref directive) = check.violated_directive {
                        crate::csp::report_violation(&crate::csp::CspViolation {
                            document_uri: page.url.clone(),
                            blocked_uri: action_url.clone(),
                            effective_directive: directive.clone(),
                            original_policy: String::new(),
                            disposition: crate::csp::Disposition::Enforce,
                            status_code: page.status,
                        });
                    }
                    anyhow::bail!(
                        "Form submission to '{}' blocked by CSP form-action",
                        action_url
                    );
                }
            }
        }
    }

    // Collect all form fields from HTML
    let html_fields = collect_form_fields(&form_el);

    // Merge: HTML defaults overridden by user state
    let mut final_fields = html_fields;
    for (name, value) in state.entries() {
        final_fields.insert(name.clone(), value.clone());
    }

    // Build and send HTTP request
    let new_page = if method == "GET" {
        submit_get(app, &action_url, &final_fields).await?
    } else {
        submit_post_urlencoded(app, &action_url, &final_fields).await?
    };

    Ok(InteractionResult::Navigated(new_page))
}

/// Collect all form fields from a <form> element.
fn collect_form_fields<'a>(form: &'a scraper::ElementRef<'a>) -> HashMap<String, String> {
    let mut fields = HashMap::new();

    // Collect input fields
    if let Ok(sel) = Selector::parse("input") {
        for el in form.select(&sel) {
            let input_type = el.value().attr("type").unwrap_or("text");
            let name = match el.value().attr("name") {
                Some(n) => n.to_string(),
                None => continue,
            };

            match input_type {
                "submit" | "reset" | "button" | "image" => continue,
                "checkbox" | "radio" => {
                    if el.value().attr("checked").is_some() {
                        let value = el.value().attr("value").unwrap_or("on").to_string();
                        fields.insert(name, value);
                    }
                }
                _ => {
                    let value = el.value().attr("value").unwrap_or("").to_string();
                    fields.insert(name, value);
                }
            }
        }
    }

    // Collect select fields
    if let Ok(sel) = Selector::parse("select") {
        for el in form.select(&sel) {
            let name = match el.value().attr("name") {
                Some(n) => n.to_string(),
                None => continue,
            };

            if let Ok(opt_sel) = Selector::parse("option") {
                let mut found_selected = false;
                for opt in el.select(&opt_sel) {
                    if opt.value().attr("selected").is_some() {
                        let value = opt.value().attr("value").unwrap_or("").to_string();
                        fields.insert(name.clone(), value);
                        found_selected = true;
                        break;
                    }
                }
                if !found_selected {
                    if let Some(first) = el.select(&opt_sel).next() {
                        let value = first.value().attr("value").unwrap_or("").to_string();
                        fields.insert(name, value);
                    }
                }
            }
        }
    }

    // Collect textarea fields
    if let Ok(sel) = Selector::parse("textarea") {
        for el in form.select(&sel) {
            let name = match el.value().attr("name") {
                Some(n) => n.to_string(),
                None => continue,
            };
            let value = el.text().collect::<String>();
            fields.insert(name, value);
        }
    }

    fields
}

async fn submit_get(
    app: &Arc<App>,
    action_url: &str,
    fields: &HashMap<String, String>,
) -> anyhow::Result<Page> {
    let mut url = Url::parse(action_url)?;
    {
        let mut query_pairs = url.query_pairs_mut();
        query_pairs.clear();
        for (name, value) in fields {
            query_pairs.append_pair(name, value);
        }
    }

    Page::from_url(app, url.as_str()).await
}

async fn submit_post_urlencoded(
    app: &Arc<App>,
    action_url: &str,
    fields: &HashMap<String, String>,
) -> anyhow::Result<Page> {
    use std::time::Instant;
    let start = Instant::now();

    let field_pairs: Vec<(&String, &String)> = fields.iter().collect();
    let response = app
        .http_client
        .post(action_url)
        .form(&field_pairs)
        .send()
        .await?;

    let status = response.status().as_u16();
    let final_url = response.url().to_string();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let body = response.text().await?;
    let timing_ms = start.elapsed().as_millis();

    let record = pardus_debug::NetworkRecord::fetched(
        {
            let log = app.network_log.lock().unwrap_or_else(|e| e.into_inner());
            log.next_id()
        },
        "POST".to_string(),
        pardus_debug::ResourceType::Document,
        "document · form submission".to_string(),
        final_url.clone(),
        pardus_debug::Initiator::Other,
    );
    {
        let mut log = app.network_log.lock().unwrap_or_else(|e| e.into_inner());
        let mut r = record;
        r.status = Some(status);
        r.content_type = content_type.clone();
        r.body_size = Some(body.len());
        r.timing_ms = Some(timing_ms);
        r.response_headers = response_headers_from_content_type(&content_type);
        log.push(r);
    }

    crate::page::validate_content_type_pub(content_type.as_deref(), &final_url)?;
    let html = scraper::Html::parse_document(&body);
    let base_url = Page::extract_base_url_static(&html, &final_url);

    Ok(Page {
        url: final_url,
        status,
        content_type,
        html,
        base_url,
        csp: None,
        frame_tree: None,
        cached_tree: None,
    })
}

fn response_headers_from_content_type(ct: &Option<String>) -> Vec<(String, String)> {
    match ct {
        Some(c) => vec![("content-type".to_string(), c.clone())],
        None => vec![],
    }
}
