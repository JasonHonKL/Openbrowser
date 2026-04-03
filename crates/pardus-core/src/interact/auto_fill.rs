use std::collections::HashMap;

use crate::interact::form::FormState;
use crate::page::Page;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};

/// A set of values to auto-fill into form fields, keyed by field name or label.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoFillValues {
    pub entries: HashMap<String, String>,
}

impl AutoFillValues {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn set(mut self, key: &str, value: &str) -> Self {
        self.entries.insert(key.to_lowercase(), value.to_string());
        self
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(&key.to_lowercase()).map(|s| s.as_str())
    }
}

/// Result of auto-filling a form.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AutoFillResult {
    pub form_state: FormState,
    pub filled_fields: Vec<AutoFillFieldResult>,
    pub unmatched_fields: Vec<UnmatchedField>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AutoFillFieldResult {
    pub field_name: String,
    pub value: String,
    pub matched_by: MatchMethod,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UnmatchedField {
    pub field_name: Option<String>,
    pub field_type: String,
    pub label: Option<String>,
    pub placeholder: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum MatchMethod {
    ByName,
    ByLabel,
    ByPlaceholder,
    ByType,
}

/// Validation status for a form field value.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationStatus {
    Valid,
    EmptyRequired,
    TooShort { min: usize, actual: usize },
    PatternMismatch { pattern: String },
    EmailInvalid,
    UrlInvalid,
}

/// Auto-fill a form on a page using the provided values.
///
/// Strategy:
/// 1. Match values to fields by field name (case-insensitive).
/// 2. Match by label text.
/// 3. Match by placeholder text.
/// 4. Match by input type (e.g., "email" key -> type="email" field).
///
/// Returns an `AutoFillResult` with filled fields and unmatched fields.
pub fn auto_fill(values: &AutoFillValues, page: &Page) -> AutoFillResult {
    let mut form_state = FormState::new();
    let mut filled_fields = Vec::new();
    let mut unmatched_fields = Vec::new();

    let all_inputs = collect_all_inputs(&page.html);

    for input in &all_inputs {
        let field_name = input.name.clone().unwrap_or_default();
        let field_label = input.label.clone();
        let field_placeholder = input.placeholder.clone();
        let field_type = input.field_type.clone();
        let required = input.required;

        let value = resolve_value(
            values,
            &field_name,
            &field_label,
            &field_placeholder,
            &field_type,
        );

        if let Some(value) = value {
            if !field_name.is_empty() {
                form_state.set(&field_name, &value);

                let matched_by = if values.entries.contains_key(&field_name.to_lowercase()) {
                    MatchMethod::ByName
                } else if let Some(ref label) = field_label {
                    if values.entries.contains_key(&label.to_lowercase()) {
                        MatchMethod::ByLabel
                    } else if let Some(ref ph) = field_placeholder {
                        if values.entries.contains_key(&ph.to_lowercase()) {
                            MatchMethod::ByPlaceholder
                        } else {
                            MatchMethod::ByType
                        }
                    } else {
                        MatchMethod::ByType
                    }
                } else if let Some(ref ph) = field_placeholder {
                    if values.entries.contains_key(&ph.to_lowercase()) {
                        MatchMethod::ByPlaceholder
                    } else {
                        MatchMethod::ByType
                    }
                } else {
                    MatchMethod::ByType
                };

                filled_fields.push(AutoFillFieldResult {
                    field_name: field_name.clone(),
                    value,
                    matched_by,
                });
            }
        } else {
            unmatched_fields.push(UnmatchedField {
                field_name: input.name.clone(),
                field_type,
                label: field_label,
                placeholder: field_placeholder,
                required,
            });
        }
    }

    AutoFillResult {
        form_state,
        filled_fields,
        unmatched_fields,
    }
}

/// Resolve a value for a field using multiple matching strategies.
fn resolve_value(
    values: &AutoFillValues,
    field_name: &str,
    field_label: &Option<String>,
    field_placeholder: &Option<String>,
    field_type: &str,
) -> Option<String> {
    if !field_name.is_empty() {
        if let Some(v) = values.get(field_name) {
            return Some(v.to_string());
        }
    }

    if let Some(label) = field_label {
        if !label.is_empty() {
            if let Some(v) = values.get(label) {
                return Some(v.to_string());
            }
            let clean = label.trim_end_matches(':').trim();
            if clean != label {
                if let Some(v) = values.get(clean) {
                    return Some(v.to_string());
                }
            }
        }
    }

    if let Some(ph) = field_placeholder {
        if !ph.is_empty() {
            if let Some(v) = values.get(ph) {
                return Some(v.to_string());
            }
        }
    }

    match field_type {
        "email" => values.get("email").map(|s| s.to_string()),
        "password" => values.get("password").map(|s| s.to_string()),
        "tel" => values
            .get("phone")
            .or_else(|| values.get("telephone"))
            .map(|s| s.to_string()),
        "url" => values
            .get("website")
            .or_else(|| values.get("url"))
            .map(|s| s.to_string()),
        "search" => values
            .get("query")
            .or_else(|| values.get("search").or_else(|| values.get("q")))
            .map(|s| s.to_string()),
        "hidden" => None,
        _ => None,
    }
}

/// Validate the auto-fill result, checking required fields and common patterns.
pub fn validate_auto_fill(result: &AutoFillResult) -> Vec<(String, ValidationStatus)> {
    let mut issues = Vec::new();

    for unmatched in &result.unmatched_fields {
        if unmatched.required {
            issues.push((
                unmatched
                    .field_name
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                ValidationStatus::EmptyRequired,
            ));
        }
    }

    for filled in &result.filled_fields {
        let lower_type = filled.field_name.to_lowercase();
        if lower_type.contains("email") && !is_valid_email(&filled.value) {
            issues.push((filled.field_name.clone(), ValidationStatus::EmailInvalid));
        }
        if lower_type.contains("url")
            || lower_type.contains("website")
            || lower_type.contains("link")
        {
            if !is_valid_url(&filled.value) {
                issues.push((filled.field_name.clone(), ValidationStatus::UrlInvalid));
            }
        }
    }

    issues
}

fn is_valid_email(s: &str) -> bool {
    s.contains('@') && s.contains('.') && s.len() > 5
}

fn is_valid_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://") || s.starts_with("/")
}

struct InputInfo {
    name: Option<String>,
    field_type: String,
    label: Option<String>,
    placeholder: Option<String>,
    required: bool,
}

fn collect_all_inputs(html: &Html) -> Vec<InputInfo> {
    let mut inputs = Vec::new();

    if let Ok(form_sel) = Selector::parse("form") {
        for form_el in html.select(&form_sel) {
            if let Ok(input_sel) = Selector::parse("input, select, textarea") {
                for field_el in form_el.select(&input_sel) {
                    let field_type = field_el
                        .value()
                        .attr("type")
                        .unwrap_or_else(|| field_el.value().name())
                        .to_string();
                    if matches!(
                        field_type.as_str(),
                        "submit" | "reset" | "button" | "image" | "hidden"
                    ) {
                        continue;
                    }

                    let name = field_el.value().attr("name").map(|s| s.to_string());
                    let placeholder = field_el.value().attr("placeholder").map(|s| s.to_string());
                    let required = field_el.value().attr("required").is_some();

                    let label = find_label_for_element(&form_el, name.as_deref());

                    inputs.push(InputInfo {
                        name,
                        field_type,
                        label,
                        placeholder,
                        required,
                    });
                }
            }
        }
    }

    if let Ok(input_sel) = Selector::parse("input, select, textarea") {
        for field_el in html.select(&input_sel) {
            let field_type = field_el
                .value()
                .attr("type")
                .unwrap_or_else(|| field_el.value().name())
                .to_string();
            if matches!(
                field_type.as_str(),
                "submit" | "reset" | "button" | "image" | "hidden"
            ) {
                continue;
            }
            let name = field_el.value().attr("name").map(|s| s.to_string());

            let already_in_form = inputs.iter().any(|i| i.name == name);
            if already_in_form {
                continue;
            }

            let placeholder = field_el.value().attr("placeholder").map(|s| s.to_string());
            let required = field_el.value().attr("required").is_some();
            inputs.push(InputInfo {
                name,
                field_type,
                label: None,
                placeholder,
                required,
            });
        }
    }

    inputs
}

fn find_label_for_element(form: &ElementRef, field_name: Option<&str>) -> Option<String> {
    if let Some(name) = field_name {
        if let Ok(label_sel) = Selector::parse("label") {
            for label_el in form.select(&label_sel) {
                if label_el.value().attr("for") == Some(name) {
                    let text: String = label_el.text().collect();
                    let trimmed = text.trim().to_string();
                    if !trimmed.is_empty() {
                        return Some(trimmed);
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_fill_by_name() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form action="/login" method="post">
                    <input type="text" name="username">
                    <input type="password" name="password">
                    <button type="submit">Login</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new()
            .set("username", "testuser")
            .set("password", "secret123");

        let result = auto_fill(&values, &page);

        assert_eq!(result.filled_fields.len(), 2);
        assert_eq!(result.form_state.get("username"), Some("testuser"));
        assert_eq!(result.form_state.get("password"), Some("secret123"));
        assert!(result.unmatched_fields.is_empty());
    }

    #[test]
    fn test_auto_fill_partial_match() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form>
                    <input type="text" name="email" placeholder="Enter your email">
                    <input type="text" name="phone">
                    <button type="submit">Go</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new().set("email", "user@example.com");

        let result = auto_fill(&values, &page);

        assert_eq!(result.filled_fields.len(), 1);
        assert_eq!(result.unmatched_fields.len(), 1);
    }

    #[test]
    fn test_auto_fill_by_type_fallback() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form>
                    <input type="email" name="contact">
                    <button type="submit">Go</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new().set("email", "user@example.com");

        let result = auto_fill(&values, &page);

        assert_eq!(result.filled_fields.len(), 1);
        assert_eq!(result.filled_fields[0].matched_by, MatchMethod::ByType);
    }

    #[test]
    fn test_validate_email() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form>
                    <input type="email" name="user_email" required>
                    <button type="submit">Go</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new().set("user_email", "not-an-email");

        let result = auto_fill(&values, &page);
        let issues = validate_auto_fill(&result);

        assert!(!issues.is_empty());
        assert!(matches!(issues[0].1, ValidationStatus::EmailInvalid));
    }

    #[test]
    fn test_validate_valid_email() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form>
                    <input type="email" name="user_email" required>
                    <button type="submit">Go</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new().set("user_email", "user@example.com");

        let result = auto_fill(&values, &page);
        let issues = validate_auto_fill(&result);

        assert!(issues.is_empty());
    }

    #[test]
    fn test_auto_fill_values_case_insensitive() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form>
                    <input type="text" name="UserName">
                    <button type="submit">Go</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new().set("USERNAME", "john");

        let result = auto_fill(&values, &page);

        assert_eq!(result.filled_fields.len(), 1);
        assert_eq!(result.form_state.get("UserName"), Some("john"));
    }

    #[test]
    fn test_auto_fill_empty_form() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form>
                    <button type="submit">Go</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new();
        let result = auto_fill(&values, &page);

        assert!(result.filled_fields.is_empty());
        assert!(result.unmatched_fields.is_empty());
    }

    #[test]
    fn test_auto_fill_no_form() {
        let html = Html::parse_document("<html><body><p>No form here</p></body></html>");
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new().set("name", "value");
        let result = auto_fill(&values, &page);

        assert!(result.filled_fields.is_empty());
    }

    #[test]
    fn test_auto_fill_password_type_fallback() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form>
                    <input type="password" name="pw_field">
                    <button type="submit">Go</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new().set("password", "secret");

        let result = auto_fill(&values, &page);

        assert_eq!(result.filled_fields.len(), 1);
        assert_eq!(result.filled_fields[0].field_name, "pw_field");
        assert_eq!(result.filled_fields[0].matched_by, MatchMethod::ByType);
    }

    #[test]
    fn test_auto_fill_tel_type_fallback() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form>
                    <input type="tel" name="mobile">
                    <button type="submit">Go</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new().set("phone", "555-1234");

        let result = auto_fill(&values, &page);

        assert_eq!(result.filled_fields.len(), 1);
        assert_eq!(result.filled_fields[0].matched_by, MatchMethod::ByType);
    }

    #[test]
    fn test_auto_fill_url_type_fallback() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form>
                    <input type="url" name="homepage">
                    <button type="submit">Go</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new().set("website", "https://example.com");

        let result = auto_fill(&values, &page);

        assert_eq!(result.filled_fields.len(), 1);
        assert_eq!(result.filled_fields[0].matched_by, MatchMethod::ByType);
    }

    #[test]
    fn test_auto_fill_hidden_ignored() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form>
                    <input type="hidden" name="csrf_token" value="abc123">
                    <input type="text" name="visible" value="">
                    <button type="submit">Go</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new().set("visible", "hello");
        let result = auto_fill(&values, &page);

        assert_eq!(result.filled_fields.len(), 1);
        assert_eq!(result.filled_fields[0].field_name, "visible");
    }

    #[test]
    fn test_auto_fill_textarea() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form>
                    <textarea name="comment"></textarea>
                    <button type="submit">Go</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new().set("comment", "Hello world");

        let result = auto_fill(&values, &page);

        assert_eq!(result.filled_fields.len(), 1);
        assert_eq!(result.form_state.get("comment"), Some("Hello world"));
    }

    #[test]
    fn test_auto_fill_select() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form>
                    <select name="country">
                        <option value="us">US</option>
                        <option value="uk">UK</option>
                    </select>
                    <button type="submit">Go</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new().set("country", "uk");

        let result = auto_fill(&values, &page);

        assert_eq!(result.filled_fields.len(), 1);
        assert_eq!(result.form_state.get("country"), Some("uk"));
    }

    #[test]
    fn test_validate_required_empty_field() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form>
                    <input type="text" name="mandatory" required>
                    <button type="submit">Go</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new();
        let result = auto_fill(&values, &page);
        let issues = validate_auto_fill(&result);

        assert_eq!(issues.len(), 1);
        assert!(matches!(issues[0].1, ValidationStatus::EmptyRequired));
        assert_eq!(issues[0].0, "mandatory");
    }

    #[test]
    fn test_validate_optional_empty_no_error() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form>
                    <input type="text" name="optional">
                    <button type="submit">Go</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new();
        let result = auto_fill(&values, &page);
        let issues = validate_auto_fill(&result);

        let empty_required: Vec<_> = issues
            .iter()
            .filter(|(_, s)| matches!(s, ValidationStatus::EmptyRequired))
            .collect();
        assert!(empty_required.is_empty());
    }

    #[test]
    fn test_validate_url_invalid() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form>
                    <input type="url" name="my_url" required>
                    <button type="submit">Go</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new().set("my_url", "not-a-url");

        let result = auto_fill(&values, &page);
        let issues = validate_auto_fill(&result);

        assert!(!issues.is_empty());
        assert!(matches!(issues[0].1, ValidationStatus::UrlInvalid));
    }

    #[test]
    fn test_validate_url_valid() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form>
                    <input type="url" name="my_url" required>
                    <button type="submit">Go</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new().set("my_url", "https://example.com");

        let result = auto_fill(&values, &page);
        let issues = validate_auto_fill(&result);

        let url_issues: Vec<_> = issues
            .iter()
            .filter(|(_, s)| matches!(s, ValidationStatus::UrlInvalid))
            .collect();
        assert!(url_issues.is_empty());
    }

    #[test]
    fn test_auto_fill_multiple_forms() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form action="/login">
                    <input type="text" name="user">
                </form>
                <form action="/search">
                    <input type="text" name="query">
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new()
            .set("user", "john")
            .set("query", "test");

        let result = auto_fill(&values, &page);

        assert_eq!(result.filled_fields.len(), 2);
        assert_eq!(result.form_state.get("user"), Some("john"));
        assert_eq!(result.form_state.get("query"), Some("test"));
    }

    #[test]
    fn test_auto_fill_values_serialization() {
        let values = AutoFillValues::new()
            .set("email", "user@example.com")
            .set("name", "John");

        let json = serde_json::to_string(&values).unwrap();
        let deserialized: AutoFillValues = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.get("email"), Some("user@example.com"));
        assert_eq!(deserialized.get("NAME"), Some("John"));
        assert_eq!(deserialized.get("name"), Some("John"));
    }

    #[test]
    fn test_auto_fill_result_matched_by_name() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <form>
                    <input type="text" name="first_name">
                    <button type="submit">Go</button>
                </form>
            </body></html>
        "#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new().set("first_name", "John");

        let result = auto_fill(&values, &page);

        assert_eq!(result.filled_fields[0].matched_by, MatchMethod::ByName);
    }

    #[test]
    fn test_auto_fill_orphan_input() {
        let html = Html::parse_document(
            r#"<html><body>
                <input type="text" name="orphan_field">
            </body></html>"#,
        );
        let page = Page::from_html(&html.html(), "https://example.com");

        let values = AutoFillValues::new().set("orphan_field", "value");

        let result = auto_fill(&values, &page);

        assert_eq!(result.filled_fields.len(), 1);
        assert_eq!(result.form_state.get("orphan_field"), Some("value"));
    }
}
