//! Shared element attribute extraction for both full DOM and streaming parsing.
//!
//! Provides the [`ElementAttrs`] trait that abstracts attribute access over
//! different HTML element types (scraper::ElementRef for full DOM, simple
//! key-value maps for streaming). All semantic role/action/interactivity
//! logic lives here so it can be reused by both `TreeBuilder` and
//! `StreamingHtmlParser`.

use super::tree::SemanticRole;

/// Trait for accessing HTML element attributes.
///
/// Implemented for `scraper::ElementRef` (full DOM path) and a simple
/// attribute-map struct used by the streaming lol_html parser.
pub trait ElementAttrs {
    /// The lowercased tag name (e.g. "input", "a", "div").
    fn tag_name(&self) -> &str;
    /// Get an attribute value by name (case-insensitive for HTML).
    fn attr(&self, name: &str) -> Option<&str>;
}

// ---------------------------------------------------------------------------
// Implementations
// ---------------------------------------------------------------------------

impl ElementAttrs for scraper::ElementRef<'_> {
    fn tag_name(&self) -> &str { scraper::ElementRef::value(self).name() }

    fn attr(&self, name: &str) -> Option<&str> { scraper::ElementRef::value(self).attr(name) }
}

/// Attribute source for the streaming parser (lol_html provides attributes
/// as a borrowed map, which we convert to an owned Vec for simplicity).
pub struct AttrMap {
    tag: String,
    attrs: Vec<(String, String)>,
}

impl AttrMap {
    pub fn new(tag: String, attrs: Vec<(String, String)>) -> Self { Self { tag, attrs } }

    /// Get an attribute value by name (case-insensitive).
    pub fn attr(&self, name: &str) -> Option<&str> {
        let name_lower = name.to_ascii_lowercase();
        self.attrs
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(&name_lower))
            .map(|(_, v)| v.as_str())
    }
}

impl ElementAttrs for AttrMap {
    fn tag_name(&self) -> &str { &self.tag }

    fn attr(&self, name: &str) -> Option<&str> {
        let name_lower = name.to_ascii_lowercase();
        self.attrs
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(&name_lower))
            .map(|(_, v)| v.as_str())
    }
}

// ---------------------------------------------------------------------------
// Shared computation functions
// ---------------------------------------------------------------------------

/// Compute the semantic role from tag name and attributes.
pub fn compute_role(tag: &str, el: &dyn ElementAttrs, has_name: bool) -> SemanticRole {
    // Check explicit role attribute first
    if let Some(role_str) = el.attr("role") {
        return parse_role_str(role_str);
    }

    // Implicit roles based on tag
    match tag {
        "nav" => SemanticRole::Navigation,
        "main" => SemanticRole::Main,
        "header" => SemanticRole::Banner,
        "footer" => SemanticRole::ContentInfo,
        "aside" => SemanticRole::Complementary,
        "search" => SemanticRole::Search,
        "section" if has_name => SemanticRole::Region,
        "article" => SemanticRole::Article,
        "form" if has_name => SemanticRole::Form,
        "form" => SemanticRole::Form,

        "h1" => SemanticRole::Heading { level: 1 },
        "h2" => SemanticRole::Heading { level: 2 },
        "h3" => SemanticRole::Heading { level: 3 },
        "h4" => SemanticRole::Heading { level: 4 },
        "h5" => SemanticRole::Heading { level: 5 },
        "h6" => SemanticRole::Heading { level: 6 },

        "a" => SemanticRole::Link,
        "button" => SemanticRole::Button,
        "input" => match el.attr("type").unwrap_or("text") {
            "checkbox" => SemanticRole::Checkbox,
            "radio" => SemanticRole::Radio,
            "file" => SemanticRole::FileInput,
            "submit" | "reset" | "button" | "image" => SemanticRole::Button,
            _ => SemanticRole::TextBox,
        },
        "select" => SemanticRole::Combobox,
        "textarea" => SemanticRole::TextBox,
        "img" => SemanticRole::Image,
        "ul" | "ol" => SemanticRole::List,
        "li" => SemanticRole::ListItem,
        "table" => SemanticRole::Table,
        "dialog" => SemanticRole::Dialog,

        _ => SemanticRole::Generic,
    }
}

/// Check whether an element is interactive.
pub fn check_interactive(tag: &str, el: &dyn ElementAttrs) -> bool {
    // Native interactive
    if matches!(
        tag,
        "a" | "button" | "input" | "select" | "textarea" | "details"
    ) {
        return !(tag == "a" && el.attr("href").is_none());
    }

    // ARIA interactive
    if let Some(role) = el.attr("role") {
        if matches!(
            role,
            "button"
                | "link"
                | "textbox"
                | "checkbox"
                | "radio"
                | "combobox"
                | "switch"
                | "tab"
                | "menuitem"
                | "option"
        ) {
            return true;
        }
    }

    // Focusable
    if let Some(tabindex) = el.attr("tabindex") {
        if let Ok(idx) = tabindex.parse::<i32>() {
            if idx >= 0 {
                return true;
            }
        }
    }

    false
}

/// Compute the semantic action string for an interactive element.
pub fn compute_action(tag: &str, el: &dyn ElementAttrs, is_interactive: bool) -> Option<String> {
    if !is_interactive {
        return None;
    }

    match tag {
        "a" => Some("navigate".to_string()),
        "button" => Some("click".to_string()),
        "input" => {
            let input_type = el.attr("type").unwrap_or("text");
            Some(match input_type {
                "submit" | "reset" | "button" | "image" => "click".to_string(),
                "checkbox" | "radio" => "toggle".to_string(),
                "file" => "upload".to_string(),
                _ => "fill".to_string(),
            })
        }
        "select" => Some("select".to_string()),
        "textarea" => Some("fill".to_string()),
        _ => {
            if let Some(role) = el.attr("role") {
                match role {
                    "button" => Some("click".to_string()),
                    "link" => Some("navigate".to_string()),
                    "textbox" => Some("fill".to_string()),
                    _ => None,
                }
            } else {
                None
            }
        }
    }
}

/// Compute the accessible name from element attributes (no text content —
/// the streaming parser calls this for elements where the name comes from
/// attributes only; the full DOM path also checks text content separately).
pub fn compute_name_from_attrs(el: &dyn ElementAttrs) -> Option<String> {
    // aria-label
    if let Some(label) = el.attr("aria-label") {
        let trimmed = label.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    // title
    if let Some(title) = el.attr("title") {
        let trimmed = title.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    // alt for images
    if el.tag_name() == "img" {
        if let Some(alt) = el.attr("alt") {
            let trimmed = alt.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }

    // placeholder for inputs
    if matches!(el.tag_name(), "input" | "textarea") {
        if let Some(p) = el.attr("placeholder") {
            let trimmed = p.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }

    // value for submit/reset buttons
    if el.tag_name() == "input" {
        let input_type = el.attr("type").unwrap_or("text");
        if matches!(input_type, "submit" | "reset" | "button" | "image") {
            if let Some(value) = el.attr("value") {
                let trimmed = value.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
            return Some(match input_type {
                "submit" => "Submit".to_string(),
                "reset" => "Reset".to_string(),
                _ => "Button".to_string(),
            });
        }
    }

    // name attribute fallback for form elements
    if matches!(el.tag_name(), "input" | "select" | "textarea") {
        if let Some(n) = el.attr("name") {
            let trimmed = n.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }

    None
}

/// Parse a role string into a [`SemanticRole`].
pub fn parse_role_str(s: &str) -> SemanticRole {
    match s {
        "document" => SemanticRole::Document,
        "banner" => SemanticRole::Banner,
        "navigation" => SemanticRole::Navigation,
        "main" => SemanticRole::Main,
        "contentinfo" => SemanticRole::ContentInfo,
        "complementary" => SemanticRole::Complementary,
        "region" => SemanticRole::Region,
        "form" => SemanticRole::Form,
        "search" => SemanticRole::Search,
        "article" => SemanticRole::Article,
        "link" => SemanticRole::Link,
        "button" => SemanticRole::Button,
        "textbox" => SemanticRole::TextBox,
        "fileinput" => SemanticRole::FileInput,
        "checkbox" => SemanticRole::Checkbox,
        "radio" => SemanticRole::Radio,
        "combobox" => SemanticRole::Combobox,
        "list" => SemanticRole::List,
        "listitem" => SemanticRole::ListItem,
        "table" => SemanticRole::Table,
        "img" => SemanticRole::Image,
        "dialog" => SemanticRole::Dialog,
        "iframe" => SemanticRole::IFrame,
        _ => SemanticRole::Other(s.to_string()),
    }
}
