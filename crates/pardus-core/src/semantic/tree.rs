use std::{collections::HashMap, fmt};

use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;

use super::selector::{build_unique_selector, css_escape_id};
use crate::frame::{FrameData, FrameTree};

// ---------------------------------------------------------------------------
// Semantic Tree
// ---------------------------------------------------------------------------

/// The semantic tree extracted from an HTML page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticTree {
    pub root: SemanticNode,
    pub stats: TreeStats,
}

/// A node in the semantic tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticNode {
    pub role: SemanticRole,
    pub name: Option<String>,
    pub tag: String,
    #[serde(rename = "interactive")]
    pub is_interactive: bool,
    #[serde(skip_serializing_if = "is_false", default)]
    pub is_disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Unique ID for interactive elements (e.g., "1", "2", "3")
    /// Used by AI agents to reference clickable elements like "click #1"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_id: Option<usize>,
    /// Unique CSS selector to locate this element in the DOM.
    /// Used to reliably resolve element_id back to the actual element.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// The input type attribute, if applicable (e.g., "password", "email", "search").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_type: Option<String>,
    /// The placeholder text for input/textarea elements.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// Whether the element has the `required` attribute.
    #[serde(skip_serializing_if = "is_false", default)]
    pub is_required: bool,
    /// Whether the element has the `readonly` attribute.
    #[serde(skip_serializing_if = "is_false", default)]
    pub is_readonly: bool,
    /// The current value attribute for input/textarea/select elements.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_value: Option<String>,
    /// Whether a checkbox/radio has the `checked` attribute.
    #[serde(skip_serializing_if = "is_false", default)]
    pub is_checked: bool,
    /// Available options for <select> elements (value, label, selected).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub options: Vec<SelectOption>,
    /// The pattern attribute for input validation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// The minlength attribute.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<usize>,
    /// The maxlength attribute.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
    /// The min attribute (numeric/date inputs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_val: Option<String>,
    /// The max attribute (numeric/date inputs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_val: Option<String>,
    /// The step attribute.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_val: Option<String>,
    /// The autocomplete attribute hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autocomplete: Option<String>,
    /// The accept attribute for file inputs (e.g., "image/*,.pdf").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept: Option<String>,
    /// Whether the element has the multiple attribute (file inputs, selects).
    #[serde(skip_serializing_if = "is_false", default)]
    pub multiple: bool,
    pub children: Vec<SemanticNode>,
}

fn is_false(v: &bool) -> bool { !v }

/// An option within a <select> element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
    #[serde(skip_serializing_if = "is_false", default)]
    pub is_selected: bool,
}

/// Statistics about the semantic tree.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TreeStats {
    pub landmarks: usize,
    pub links: usize,
    pub headings: usize,
    pub actions: usize,
    pub forms: usize,
    pub images: usize,
    pub iframes: usize,
    pub total_nodes: usize,
}

// ---------------------------------------------------------------------------
// Semantic Role
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum SemanticRole {
    Document,
    Banner,
    Navigation,
    Main,
    ContentInfo,
    Complementary,
    Region,
    Form,
    Search,
    Article,
    Heading { level: u8 },
    Link,
    Button,
    TextBox,
    FileInput,
    Checkbox,
    Radio,
    Combobox,
    List,
    ListItem,
    Table,
    Row,
    Cell,
    ColumnHeader,
    RowHeader,
    Image,
    Dialog,
    IFrame,
    Generic,
    StaticText,
    Other(String),
}

impl Serialize for SemanticRole {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: serde::Serializer {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SemanticRole {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: serde::Deserializer<'de> {
        let s = String::deserialize(deserializer)?;
        Ok(parse_role_str(&s))
    }
}

impl fmt::Display for SemanticRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Heading { level } => write!(f, "heading (h{level})"),
            Self::Other(s) => write!(f, "{s}"),
            _ => write!(f, "{}", self.role_str()),
        }
    }
}

impl SemanticRole {
    pub fn role_str(&self) -> &str {
        match self {
            Self::Document => "document",
            Self::Banner => "banner",
            Self::Navigation => "navigation",
            Self::Main => "main",
            Self::ContentInfo => "contentinfo",
            Self::Complementary => "complementary",
            Self::Region => "region",
            Self::Form => "form",
            Self::Search => "search",
            Self::Article => "article",
            Self::Heading { .. } => "heading",
            Self::Link => "link",
            Self::Button => "button",
            Self::TextBox => "textbox",
            Self::FileInput => "fileinput",
            Self::Checkbox => "checkbox",
            Self::Radio => "radio",
            Self::Combobox => "combobox",
            Self::List => "list",
            Self::ListItem => "listitem",
            Self::Table => "table",
            Self::Row => "row",
            Self::Cell => "cell",
            Self::ColumnHeader => "columnheader",
            Self::RowHeader => "rowheader",
            Self::Image => "img",
            Self::Dialog => "dialog",
            Self::IFrame => "iframe",
            Self::Generic => "generic",
            Self::StaticText => "text",
            Self::Other(s) => s.as_str(),
        }
    }

    pub fn is_landmark(&self) -> bool {
        matches!(
            self,
            Self::Banner
                | Self::Navigation
                | Self::Main
                | Self::ContentInfo
                | Self::Complementary
                | Self::Region
                | Self::Form
                | Self::Search
        )
    }

    pub fn is_heading(&self) -> bool { matches!(self, Self::Heading { .. }) }
}

impl SemanticTree {
    /// Build a semantic tree from parsed HTML (no iframe recursion).
    pub fn build(html: &Html, base_url: &str) -> Self {
        let mut stats = TreeStats::default();
        let mut builder = TreeBuilder {
            base_url,
            html,
            stats: &mut stats,
            next_element_id: 1,
            iframe_map: &HashMap::new(),
        };

        let root = builder.build_from_html(html);
        stats.total_nodes = count_nodes(&root);
        Self { root, stats }
    }

    /// Build a semantic tree with iframe-aware recursive parsing.
    ///
    /// When a `<iframe>` or `<frame>` element is encountered, its child frame
    /// content from the FrameTree is recursively walked into, and element IDs
    /// continue with global flat numbering across all frames.
    pub fn build_with_frames(html: &Html, base_url: &str, frame_tree: &FrameTree) -> Self {
        let mut stats = TreeStats::default();

        // Build a map: selector -> FrameData for iframe lookup
        let iframe_map = build_iframe_selector_map(&frame_tree.root);

        let mut builder = TreeBuilder {
            base_url,
            html,
            stats: &mut stats,
            next_element_id: 1,
            iframe_map: &iframe_map,
        };

        let root = builder.build_from_html(html);
        stats.total_nodes = count_nodes(&root);
        Self { root, stats }
    }
}

fn count_nodes(node: &SemanticNode) -> usize {
    1 + node.children.iter().map(count_nodes).sum::<usize>()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check whether an inline `style` attribute makes the element invisible.
///
/// Handles the most common CSS patterns that hide content:
/// - `display: none` / `display:none`
/// - `visibility: hidden` / `visibility:hidden` / `visibility: collapse`
/// - `opacity: 0` / `opacity:0` (also catches `opacity: 0.0`, `opacity:0.00`)
/// - `clip-path: inset(100%)` / `clip: rect(0,0,0,0)` (accessibility hiding)
///
/// This is intentionally conservative: it only matches the most common patterns
/// that unambiguously hide content. Complex CSS rules (classes, external stylesheets)
/// are not handled here — this catches only inline `style` attribute hiding.
fn is_css_hidden(style: Option<&str>) -> bool {
    let style = match style {
        Some(s) => s,
        None => return false,
    };

    let lower = style.to_ascii_lowercase();

    // Fast path: if none of these substrings appear, no need to check further
    if !lower.contains("display")
        && !lower.contains("visibility")
        && !lower.contains("opacity")
        && !lower.contains("clip")
    {
        return false;
    }

    for decl in lower.split(';') {
        let decl = decl.trim();
        if let Some((prop, val)) = decl.split_once(':') {
            let prop = prop.trim();
            let val = val.trim();
            match prop {
                "display" if val == "none" => return true,
                "visibility" if val == "hidden" || val == "collapse" => return true,
                "opacity" => {
                    // Match 0, 0.0, 0.00 etc.
                    if val.starts_with('0') && (val.len() == 1 || val.chars().nth(1) == Some('.')) {
                        return true;
                    }
                }
                "clip-path" if val.contains("inset(100%)") => return true,
                "clip" if val.starts_with("rect(0") => return true,
                _ => {}
            }
        }
    }

    false
}

fn make_static_text(content: &str) -> SemanticNode {
    SemanticNode {
        role: SemanticRole::StaticText,
        name: Some(content.to_string()),
        tag: "#text".to_string(),
        is_interactive: false,
        is_disabled: false,
        href: None,
        action: None,
        element_id: None,
        selector: None,
        input_type: None,
        placeholder: None,
        is_required: false,
        is_readonly: false,
        current_value: None,
        is_checked: false,
        options: Vec::new(),
        pattern: None,
        min_length: None,
        max_length: None,
        min_val: None,
        max_val: None,
        step_val: None,
        autocomplete: None,
        accept: None,
        multiple: false,
        children: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Tree Builder
// ---------------------------------------------------------------------------

struct TreeBuilder<'a> {
    base_url: &'a str,
    html: &'a Html,
    stats: &'a mut TreeStats,
    next_element_id: usize,
    /// Map from CSS selector (of the <iframe> element in the parent) -> child FrameData.
    iframe_map: &'a HashMap<String, &'a FrameData>,
}

impl<'a> TreeBuilder<'a> {
    fn build_from_html(&mut self, html: &Html) -> SemanticNode {
        let body_selector = Selector::parse("body").unwrap();
        let root = SemanticNode {
            role: SemanticRole::Document,
            name: None,
            tag: "document".to_string(),
            is_interactive: false,
            is_disabled: false,
            href: None,
            action: None,
            element_id: None,
            selector: None,
            input_type: None,
            placeholder: None,
            is_required: false,
            is_readonly: false,
            current_value: None,
            is_checked: false,
            options: Vec::new(),
            pattern: None,
            min_length: None,
            max_length: None,
            min_val: None,
            max_val: None,
            step_val: None,
            autocomplete: None,
            accept: None,
            multiple: false,
            children: Vec::new(),
        };

        let mut children = Vec::new();
        if let Some(body_el) = html.select(&body_selector).next() {
            for child_node in body_el.children() {
                if let Some(child_el) = ElementRef::wrap(child_node) {
                    if let Some(node) = self.walk_element(&child_el) {
                        children.push(node);
                    }
                } else if let scraper::Node::Text(text) = child_node.value() {
                    let content = text.trim();
                    if !content.is_empty() {
                        children.push(make_static_text(content));
                    }
                }
            }
        }

        SemanticNode { children, ..root }
    }

    fn walk_element(&mut self, el: &ElementRef) -> Option<SemanticNode> {
        let tag = el.value().name().to_lowercase();
        let tag_str = tag.as_str();

        // Skip metadata elements
        if matches!(
            tag_str,
            "script" | "style" | "link" | "meta" | "noscript" | "head"
        ) {
            return None;
        }

        // Skip hidden elements
        if el.value().attr("hidden").is_some() || el.value().attr("aria-hidden") == Some("true") {
            return None;
        }

        // Skip CSS-hidden elements (display:none, visibility:hidden, opacity:0)
        if is_css_hidden(el.value().attr("style")) {
            return None;
        }

        // Skip hidden form inputs — they carry data, not UI
        if tag_str == "input" {
            if let Some(t) = el.value().attr("type") {
                if t.eq_ignore_ascii_case("hidden") {
                    return None;
                }
            }
        }

        // Handle iframe/frame elements
        if tag_str == "iframe" || tag_str == "frame" {
            return self.walk_iframe(el, tag_str);
        }

        // Compute role
        let name = self.compute_name(el);
        let has_name = name.is_some();
        let role = self.compute_role(tag_str, el, has_name);

        // Check interactivity
        let is_interactive = self.check_interactive(tag_str, el);
        let action = self.compute_action(tag_str, el, is_interactive);
        let is_disabled = el.value().attr("disabled").is_some();

        // Get href for links
        let href = if tag_str == "a" {
            el.value().attr("href").map(|h| self.resolve_url(h))
        } else {
            None
        };

        // Walk children
        let mut child_nodes = Vec::new();
        for child_node in el.children() {
            if let Some(child_el) = ElementRef::wrap(child_node) {
                if let Some(child) = self.walk_element(&child_el) {
                    child_nodes.push(child);
                }
            } else if let scraper::Node::Text(text) = child_node.value() {
                let content = text.trim();
                if !content.is_empty() {
                    child_nodes.push(make_static_text(content));
                }
            }
        }

        // Prune structural nodes without names
        let is_structural = matches!(role, SemanticRole::Generic);
        if is_structural && !has_name && href.is_none() && !is_interactive {
            if child_nodes.is_empty() {
                return None;
            }
            if child_nodes.len() == 1 {
                return Some(child_nodes.remove(0));
            }
        }

        // Update stats
        // Per ARIA spec: form and region are only landmarks when they have an accessible name
        let is_named_form_or_region =
            matches!(role, SemanticRole::Form | SemanticRole::Region) && has_name;
        let is_other_landmark =
            role.is_landmark() && !matches!(role, SemanticRole::Form | SemanticRole::Region);
        if is_other_landmark || is_named_form_or_region {
            self.stats.landmarks += 1;
        }
        if matches!(role, SemanticRole::Link) {
            self.stats.links += 1;
        }
        if role.is_heading() {
            self.stats.headings += 1;
        }
        if matches!(role, SemanticRole::Form) {
            self.stats.forms += 1;
        }
        if matches!(role, SemanticRole::Image) {
            self.stats.images += 1;
        }
        if is_interactive {
            self.stats.actions += 1;
        }

        // Assign element ID to interactive elements (including disabled ones)
        let element_id = if is_interactive {
            let id = self.next_element_id;
            self.next_element_id += 1;
            Some(id)
        } else {
            None
        };

        // Compute unique CSS selector for this element
        let selector = build_unique_selector(el, self.html);

        // Extract input type for input elements
        let input_type = if tag_str == "input" {
            el.value().attr("type").map(|s| s.to_string())
        } else {
            None
        };

        // Extract form element metadata
        let is_form_element = matches!(tag_str, "input" | "textarea" | "select");
        let placeholder = if is_form_element {
            el.value().attr("placeholder").map(|s| s.to_string())
        } else {
            None
        };
        let is_required = el.value().attr("required").is_some();
        let is_readonly = el.value().attr("readonly").is_some();
        let current_value = if is_form_element {
            el.value().attr("value").map(|s| s.to_string())
        } else {
            None
        };
        let is_checked = el.value().attr("checked").is_some();
        let pattern = el.value().attr("pattern").map(|s| s.to_string());
        let min_length = el
            .value()
            .attr("minlength")
            .and_then(|s| s.parse::<usize>().ok());
        let max_length = el
            .value()
            .attr("maxlength")
            .and_then(|s| s.parse::<usize>().ok());
        let min_val = el.value().attr("min").map(|s| s.to_string());
        let max_val = el.value().attr("max").map(|s| s.to_string());
        let step_val = el.value().attr("step").map(|s| s.to_string());
        let autocomplete = el.value().attr("autocomplete").map(|s| s.to_string());
        let accept = if tag_str == "input" && input_type.as_deref() == Some("file") {
            el.value().attr("accept").map(|s| s.to_string())
        } else {
            None
        };
        let multiple = el.value().attr("multiple").is_some();

        // Extract select options
        let options = if tag_str == "select" {
            let opt_selector = Selector::parse("option").unwrap();
            el.select(&opt_selector)
                .map(|opt| {
                    let val = opt.value().attr("value").unwrap_or("");
                    let label = opt.text().collect::<String>();
                    let label = label.trim().to_string();
                    let selected = opt.value().attr("selected").is_some();
                    SelectOption {
                        value: val.to_string(),
                        label,
                        is_selected: selected,
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        Some(SemanticNode {
            role,
            name,
            tag: tag_str.to_string(),
            is_interactive,
            is_disabled,
            href,
            action,
            element_id,
            selector: Some(selector),
            input_type,
            placeholder,
            is_required,
            is_readonly,
            current_value,
            is_checked,
            options,
            pattern,
            min_length,
            max_length,
            min_val,
            max_val,
            step_val,
            autocomplete,
            accept,
            multiple,
            children: child_nodes,
        })
    }

    /// Handle iframe/frame elements: look up child frame content and recurse.
    fn walk_iframe(&mut self, el: &ElementRef, tag_str: &str) -> Option<SemanticNode> {
        let selector = build_unique_selector(el, self.html);

        // Try to find the child frame in our iframe map
        let child_frame = self.iframe_map.get(&selector).copied();

        let src = el.value().attr("src").map(|s| self.resolve_url(s));
        let title = el
            .value()
            .attr("title")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let name = el
            .value()
            .attr("name")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let frame_name = title.or(name).or(src.clone()).unwrap_or_else(|| {
            child_frame
                .map(|f| f.url.clone())
                .unwrap_or_else(|| "iframe".to_string())
        });

        self.stats.iframes += 1;

        // Walk into child frame content if available
        let mut child_nodes = Vec::new();
        if let Some(frame_data) = child_frame {
            if let Some(frame_html) = frame_data.parsed_html() {
                let frame_base_url = &frame_data.url;
                let _ = frame_base_url;
                let body_selector = Selector::parse("body").unwrap();
                if let Some(body_el) = frame_html.select(&body_selector).next() {
                    for child_node in body_el.children() {
                        if let Some(child_el) = ElementRef::wrap(child_node) {
                            if let Some(child) = self.walk_element(&child_el) {
                                child_nodes.push(child);
                            }
                        }
                    }
                }
            }
        }

        Some(SemanticNode {
            role: SemanticRole::IFrame,
            name: Some(frame_name),
            tag: tag_str.to_string(),
            is_interactive: false,
            is_disabled: false,
            href: src,
            action: None,
            element_id: None,
            selector: Some(selector),
            input_type: None,
            placeholder: None,
            is_required: false,
            is_readonly: false,
            current_value: None,
            is_checked: false,
            options: Vec::new(),
            pattern: None,
            min_length: None,
            max_length: None,
            min_val: None,
            max_val: None,
            step_val: None,
            autocomplete: None,
            accept: None,
            multiple: false,
            children: child_nodes,
        })
    }

    fn compute_name(&self, el: &ElementRef) -> Option<String> {
        // aria-labelledby: resolve element IDs and concatenate their text
        if let Some(ids) = el.value().attr("aria-labelledby") {
            let text = self.resolve_aria_labelledby(ids);
            if !text.is_empty() {
                return Some(text);
            }
        }

        // aria-label
        if let Some(label) = el.value().attr("aria-label") {
            let trimmed = label.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }

        // title
        if let Some(title) = el.value().attr("title") {
            let trimmed = title.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }

        // alt for images
        if el.value().name() == "img" {
            if let Some(alt) = el.value().attr("alt") {
                let trimmed = alt.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }

        // placeholder for inputs
        if matches!(el.value().name(), "input" | "textarea") {
            if let Some(p) = el.value().attr("placeholder") {
                let trimmed = p.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }

        // text content for buttons, links, headings
        let tag = el.value().name();
        if matches!(
            tag,
            "a" | "button" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "summary" | "span"
        ) {
            let text = el.text().collect::<String>().trim().to_string();
            if !text.is_empty() {
                return Some(text);
            }
        }

        // name attribute for inputs (fallback)
        if matches!(tag, "input" | "textarea" | "select") {
            if let Some(n) = el.value().attr("name") {
                let trimmed = n.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }

        // value for submit/reset buttons
        if tag == "input" {
            let input_type = el.value().attr("type").unwrap_or("text");
            if matches!(input_type, "submit" | "reset" | "button" | "image") {
                if let Some(value) = el.value().attr("value") {
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

        // fallback: use the name attribute for inputs, selects, and textareas
        if matches!(tag, "input" | "select" | "textarea") {
            if let Some(name_attr) = el.value().attr("name") {
                let trimmed = name_attr.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }

        None
    }

    fn compute_role(&self, tag: &str, el: &ElementRef, has_name: bool) -> SemanticRole {
        super::extract::compute_role(tag, el, has_name)
    }

    fn check_interactive(&self, tag: &str, el: &ElementRef) -> bool {
        super::extract::check_interactive(tag, el)
    }

    fn compute_action(&self, tag: &str, el: &ElementRef, is_interactive: bool) -> Option<String> {
        super::extract::compute_action(tag, el, is_interactive)
    }

    fn resolve_url(&self, href: &str) -> String {
        Url::parse(self.base_url)
            .and_then(|base| base.join(href))
            .map(|u| u.to_string())
            .unwrap_or_else(|_| href.to_string())
    }

    /// Resolve `aria-labelledby` by looking up each referenced element ID
    /// and concatenating their text content.
    fn resolve_aria_labelledby(&self, ids: &str) -> String {
        ids.split_whitespace()
            .filter_map(|id| {
                let sel = format!("#{}", css_escape_id(id));
                Selector::parse(&sel).ok().and_then(|s| {
                    self.html
                        .select(&s)
                        .next()
                        .map(|el| el.text().collect::<String>().trim().to_string())
                })
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn parse_role_str(s: &str) -> SemanticRole { super::extract::parse_role_str(s) }

// ---------------------------------------------------------------------------
// Unique Selector Builder
// ---------------------------------------------------------------------------

/// Build a unique CSS selector for an element.
// ---------------------------------------------------------------------------
// IFrame Selector Map
// ---------------------------------------------------------------------------

/// Build a map from CSS selectors (of <iframe> elements in a parent frame) to child FrameData.
///
/// This is used by TreeBuilder to look up child frame content when it encounters
/// an <iframe> element in the parent HTML. The selector must match how
/// `build_unique_selector` would identify the <iframe> element.
fn build_iframe_selector_map(root_frame: &FrameData) -> HashMap<String, &FrameData> {
    let mut map = HashMap::new();
    populate_iframe_map(root_frame, &mut map);
    map
}

fn populate_iframe_map<'a>(frame: &'a FrameData, map: &mut HashMap<String, &'a FrameData>) {
    if let Some(html) = frame.parsed_html() {
        let iframe_selector = Selector::parse("iframe, frame").unwrap();
        let mut child_idx = 0;

        for el in html.select(&iframe_selector) {
            let selector = build_unique_selector(&el, &html);
            map.insert(selector, &frame.child_frames[child_idx]);
            child_idx += 1;
        }
    }

    for child_frame in &frame.child_frames {
        populate_iframe_map(child_frame, map);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_css_hidden_display_none() {
        assert!(is_css_hidden(Some("display:none")));
        assert!(is_css_hidden(Some("display: none")));
        assert!(is_css_hidden(Some("display: none; color: red")));
        assert!(is_css_hidden(Some("color: red; display: none")));
        assert!(is_css_hidden(Some("display:NONE")));
    }

    #[test]
    fn test_css_hidden_visibility_hidden() {
        assert!(is_css_hidden(Some("visibility:hidden")));
        assert!(is_css_hidden(Some("visibility: hidden")));
        assert!(is_css_hidden(Some("visibility: collapse")));
    }

    #[test]
    fn test_css_hidden_opacity_zero() {
        assert!(is_css_hidden(Some("opacity:0")));
        assert!(is_css_hidden(Some("opacity: 0")));
        assert!(is_css_hidden(Some("opacity: 0.0")));
        assert!(is_css_hidden(Some("opacity:0.00")));
        assert!(!is_css_hidden(Some("opacity: 0.1")));
        assert!(!is_css_hidden(Some("opacity: 1")));
    }

    #[test]
    fn test_css_hidden_clip_path() {
        assert!(is_css_hidden(Some("clip-path: inset(100%)")));
    }

    #[test]
    fn test_css_not_hidden() {
        assert!(!is_css_hidden(None));
        assert!(!is_css_hidden(Some("")));
        assert!(!is_css_hidden(Some("color: red")));
        assert!(!is_css_hidden(Some("display: block")));
        assert!(!is_css_hidden(Some("display: flex")));
        assert!(!is_css_hidden(Some("visibility: visible")));
        assert!(!is_css_hidden(Some("opacity: 1")));
        assert!(!is_css_hidden(Some("font-size: 14px")));
    }
}
