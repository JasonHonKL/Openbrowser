use std::fmt;

use scraper::{ElementRef, Html};
use serde::Serialize;

use crate::semantic::build_unique_selector;

/// A stable reference to an element within a Page.
///
/// Stores a CSS selector that uniquely identifies the element,
/// plus cached attribute data extracted at creation time.
/// `Clone + Send + Sync` with no lifetime parameters.
#[derive(Debug, Clone, Serialize)]
pub struct ElementHandle {
    /// CSS selector that re-locates this element in the Page's HTML.
    pub selector: String,
    /// The tag name (lowercase).
    pub tag: String,
    /// Element id attribute, if present.
    pub id: Option<String>,
    /// Element name attribute, if present.
    pub name: Option<String>,
    /// The semantic action: "navigate", "click", "fill", "toggle", "select", "upload".
    pub action: Option<String>,
    /// Whether the element is disabled.
    pub is_disabled: bool,
    /// The href for links.
    pub href: Option<String>,
    /// The name/label text.
    pub label: Option<String>,
    /// The input type, if applicable.
    pub input_type: Option<String>,
    /// The current value attribute, if present.
    pub value: Option<String>,
    /// The accept attribute for file inputs (e.g., "image/*,.pdf").
    pub accept: Option<String>,
    /// Whether the element has the multiple attribute (file inputs).
    pub multiple: bool,
}

/// Create an ElementHandle from a scraper ElementRef.
pub fn element_to_handle(el: &ElementRef, html: &Html) -> ElementHandle {
    let selector = build_unique_selector(el, html);
    let tag = el.value().name().to_lowercase();

    let name_attr = el.value().attr("name").map(|s| s.to_string());
    let href = el.value().attr("href").map(|s| s.to_string());
    let input_type = el.value().attr("type").map(|s| s.to_string());
    let value = el.value().attr("value").map(|s| s.to_string());
    let id = el.value().attr("id").map(|s| s.to_string());
    let is_disabled = el.value().attr("disabled").is_some();
    let accept = el.value().attr("accept").map(|s| s.to_string());
    let multiple = el.value().attr("multiple").is_some();

    let action = compute_action(&tag, input_type.as_deref());
    let label = compute_label(&tag, el);

    ElementHandle {
        selector,
        tag,
        id,
        name: name_attr,
        action,
        is_disabled,
        href,
        label,
        input_type,
        value,
        accept,
        multiple,
    }
}

pub fn compute_action(tag: &str, input_type: Option<&str>) -> Option<String> {
    match tag {
        "a" => Some("navigate".to_string()),
        "button" => Some("click".to_string()),
        "input" => {
            let itype = input_type.unwrap_or("text");
            // Hidden inputs are not interactive
            if itype.eq_ignore_ascii_case("hidden") {
                return None;
            }
            Some(
                match itype {
                    "submit" | "reset" | "button" | "image" => "click",
                    "checkbox" | "radio" => "toggle",
                    "file" => "upload",
                    _ => "fill",
                }
                .to_string(),
            )
        }
        "select" => Some("select".to_string()),
        "textarea" => Some("fill".to_string()),
        _ => None,
    }
}

pub fn compute_label(tag: &str, el: &ElementRef) -> Option<String> {
    if let Some(label) = el.value().attr("aria-label") {
        let trimmed = label.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    if let Some(title) = el.value().attr("title") {
        let trimmed = title.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    if matches!(tag, "input" | "textarea") {
        if let Some(p) = el.value().attr("placeholder") {
            let trimmed = p.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }

    if matches!(tag, "a" | "button") {
        let text = el.text().collect::<String>().trim().to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }

    if tag == "input" {
        let itype = el.value().attr("type").unwrap_or("text");
        if matches!(itype, "submit" | "reset" | "button") {
            if let Some(val) = el.value().attr("value") {
                let trimmed = val.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
    }

    None
}

impl fmt::Display for ElementHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", self.tag)?;
        if let Some(id) = &self.id {
            write!(f, "#{}", id)?;
        }
        if let Some(name) = &self.name {
            write!(f, "[name=\"{}\"]", name)?;
        }
        if let Some(label) = &self.label {
            write!(f, " \"{}\"", label)?;
        }
        if let Some(action) = &self.action {
            write!(f, " [{}]", action)?;
        }
        Ok(())
    }
}
