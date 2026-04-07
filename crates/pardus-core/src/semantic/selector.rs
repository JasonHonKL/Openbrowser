use scraper::{ElementRef, Html, Selector};

/// Build a unique CSS selector for an element.
///
/// - If the element has an `id`, uses `#id`.
/// - Otherwise, prefers attribute-based selectors like `input[name="foo"]` if they are unique in
///   the document.
/// - Falls back to a structural path: `body > div:nth-child(2) > form > input`
pub fn build_unique_selector(el: &ElementRef, html: &Html) -> String {
    if let Some(id) = el.value().attr("id") {
        return format!("#{}", css_escape_id(id));
    }

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

    if let Some(href) = el.value().attr("href") {
        let tag = el.value().name();
        let escaped = css_escape_attr(href);
        let candidate = format!("{}[href=\"{}\"]", tag, escaped);
        let is_unique = match Selector::parse(&candidate) {
            Ok(sel) => html.select(&sel).count() == 1,
            Err(_) => false,
        };
        if is_unique {
            return candidate;
        }
    }

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

    build_structural_selector(el)
}

pub fn css_escape_id(id: &str) -> String {
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

fn css_escape_attr(s: &str) -> String { s.replace('\\', "\\\\").replace('"', "\\\"") }

pub fn build_structural_selector(el: &ElementRef) -> String {
    let mut segments = Vec::new();
    let mut current = Some(*el);

    while let Some(node) = current {
        let tag = node.value().name().to_lowercase();

        if tag == "body" || tag == "html" {
            break;
        }

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
pub fn count_element_position(el: &ElementRef) -> usize {
    if let Some(parent) = el.parent().and_then(ElementRef::wrap) {
        let mut count = 0;

        for child in parent.children() {
            if ElementRef::wrap(child).is_some() {
                count += 1;
            }
            if child == **el {
                return count;
            }
        }
    }
    1
}
