use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use url::Url;

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
    pub children: Vec<SemanticNode>,
}

fn is_false(v: &bool) -> bool {
    !v
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
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SemanticRole {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
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

    pub fn is_heading(&self) -> bool {
        matches!(self, Self::Heading { .. })
    }
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
            children: Vec::new(),
        };

        let mut children = Vec::new();
        if let Some(body_el) = html.select(&body_selector).next() {
            for child_node in body_el.children() {
                if let Some(child_el) = ElementRef::wrap(child_node) {
                    if let Some(node) = self.walk_element(&child_el) {
                        children.push(node);
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
        if role.is_landmark() {
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
            children: child_nodes,
        })
    }

    fn compute_name(&self, el: &ElementRef) -> Option<String> {
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
            "a" | "button" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "summary"
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
        // Check explicit role attribute first
        if let Some(role_str) = el.value().attr("role") {
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
            "input" => match el.value().attr("type").unwrap_or("text") {
                "checkbox" => SemanticRole::Checkbox,
                "radio" => SemanticRole::Radio,
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

    fn check_interactive(&self, tag: &str, el: &ElementRef) -> bool {
        // Native interactive
        if matches!(
            tag,
            "a" | "button" | "input" | "select" | "textarea" | "details"
        ) {
            return !(tag == "a" && el.value().attr("href").is_none());
        }

        // ARIA interactive
        if let Some(role) = el.value().attr("role") {
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
        if let Some(tabindex) = el.value().attr("tabindex") {
            if let Ok(idx) = tabindex.parse::<i32>() {
                if idx >= 0 {
                    return true;
                }
            }
        }

        false
    }

    fn compute_action(&self, tag: &str, el: &ElementRef, is_interactive: bool) -> Option<String> {
        if !is_interactive {
            return None;
        }

        match tag {
            "a" => Some("navigate".to_string()),
            "button" => Some("click".to_string()),
            "input" => {
                let input_type = el.value().attr("type").unwrap_or("text");
                Some(match input_type {
                    "submit" | "reset" | "button" | "image" => "click".to_string(),
                    "checkbox" | "radio" => "toggle".to_string(),
                    _ => "fill".to_string(),
                })
            }
            "select" => Some("select".to_string()),
            "textarea" => Some("fill".to_string()),
            _ => {
                if let Some(role) = el.value().attr("role") {
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

    fn resolve_url(&self, href: &str) -> String {
        Url::parse(self.base_url)
            .and_then(|base| base.join(href))
            .map(|u| u.to_string())
            .unwrap_or_else(|_| href.to_string())
    }
}

fn parse_role_str(s: &str) -> SemanticRole {
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

// ---------------------------------------------------------------------------
// Unique Selector Builder
// ---------------------------------------------------------------------------

/// Build a unique CSS selector for an element.
///
/// - If the element has an `id`, uses `#id`.
/// - Otherwise, prefers attribute-based selectors like `input[name="foo"]`
///   if they are unique in the document.
/// - Falls back to a structural path: `body > div:nth-child(2) > form > input`
fn build_unique_selector(el: &ElementRef, html: &Html) -> String {
    // Prefer id
    if let Some(id) = el.value().attr("id") {
        return format!("#{}", css_escape_id(id));
    }

    // Try name-based selector
    if let Some(name) = el.value().attr("name") {
        let tag = el.value().name();
        let candidate = format!("{}[name=\"{}\"]", tag, name);
        let is_unique = match Selector::parse(&candidate) {
            Ok(sel) => html.select(&sel).count() == 1,
            Err(_) => false,
        };
        if is_unique {
            return candidate;
        }
    }

    // Try href-based selector for links
    if let Some(href) = el.value().attr("href") {
        let tag = el.value().name();
        let escaped = css_escape_attr(href);
        let candidate = format!("{}[href=\"{}\"]", tag, escaped);
        // Check if this selector is valid and unique
        let is_unique = match Selector::parse(&candidate) {
            Ok(sel) => html.select(&sel).count() == 1,
            Err(_) => false,
        };
        if is_unique {
            return candidate;
        }
    }

    // Try type + name combination for inputs
    if let Some(itype) = el.value().attr("type") {
        let tag = el.value().name();
        let candidate = format!("{}[type=\"{}\"]", tag, itype);
        let is_unique = match Selector::parse(&candidate) {
            Ok(sel) => html.select(&sel).count() == 1,
            Err(_) => false,
        };
        if is_unique {
            return candidate;
        }
    }

    // Build structural path
    build_structural_selector(el)
}

fn css_escape_id(id: &str) -> String {
    if id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        id.to_string()
    } else {
        id.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c.to_string()
                } else {
                    format!("\\{:X}", c as u32)
                }
            })
            .collect()
    }
}

fn css_escape_attr(s: &str) -> String {
    // For attribute values, we don't need to escape # since we're inside quotes
    // We just need to escape the quote character and backslash
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn build_structural_selector(el: &ElementRef) -> String {
    let mut segments = Vec::new();
    let mut current = Some(*el);

    while let Some(node) = current {
        let tag = node.value().name().to_lowercase();

        if tag == "body" || tag == "html" {
            break;
        }

        // Count position among all sibling elements
        let nth = count_element_position(&node);
        segments.push(format!("{}:nth-child({})", tag, nth));

        current = node.parent().and_then(ElementRef::wrap);
    }

    segments.reverse();
    if segments.is_empty() {
        el.value().name().to_string()
    } else {
        segments.join(" > ")
    }
}

/// Count the 1-based position of this element among its parent's children.
fn count_element_position(el: &ElementRef) -> usize {
    if let Some(parent) = el.parent().and_then(ElementRef::wrap) {
        let target_id = el.value().attr("id");
        let target_name = el.value().name();
        let mut count = 0;

        for child in parent.children() {
            if let Some(child_el) = ElementRef::wrap(child) {
                count += 1;
                if child_el.value().name() == target_name
                    && child_el.value().attr("id") == target_id
                {
                    return count;
                }
            }
        }
    }
    1
}

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
