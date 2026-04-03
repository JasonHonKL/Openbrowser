use crate::semantic::tree::{SemanticNode, SemanticRole, SemanticTree};

/// Format a semantic tree as a compact, LLM-token-optimized string.
///
/// Goals:
/// - Minimize token count by using single-char tags and terse formatting.
/// - Preserve all actionable information (element IDs, hrefs, actions).
/// - Omit decorative whitespace, verbose labels, and non-interactive static text.
/// - Structure: flat bullet list with indentation hints, not a deep tree.
pub fn format_llm(tree: &SemanticTree) -> String {
    let mut buf = String::with_capacity(4096);

    let title = find_title(&tree.root);
    if let Some(t) = title {
        buf.push_str("# ");
        buf.push_str(t.trim());
        buf.push('\n');
    }

    let mut actions = Vec::new();
    let mut links = Vec::new();
    let mut inputs = Vec::new();
    let mut headings = Vec::new();
    let mut landmarks = Vec::new();
    let mut frames = Vec::new();

    collect_flat(
        &tree.root,
        &mut actions,
        &mut links,
        &mut inputs,
        &mut headings,
        &mut landmarks,
        &mut frames,
    );

    for (level, text) in &headings {
        let prefix = match level {
            1..=2 => "## ",
            3..=4 => "### ",
            _ => "#### ",
        };
        buf.push_str(prefix);
        buf.push_str(text);
        buf.push('\n');
    }

    if !landmarks.is_empty() {
        buf.push_str("-- Regions --\n");
        for lm in &landmarks {
            buf.push_str(lm);
            buf.push('\n');
        }
    }

    if !actions.is_empty() {
        buf.push_str("-- Actions --\n");
        for a in &actions {
            buf.push_str(a);
            buf.push('\n');
        }
    }

    if !links.is_empty() {
        buf.push_str("-- Links --\n");
        for l in &links {
            buf.push_str(l);
            buf.push_str("\n");
        }
    }

    if !inputs.is_empty() {
        buf.push_str("-- Inputs --\n");
        for i in &inputs {
            buf.push_str(i);
            buf.push('\n');
        }
    }

    if !frames.is_empty() {
        buf.push_str("-- Frames --\n");
        for f in &frames {
            buf.push_str(f);
            buf.push('\n');
        }
    }

    let s = &tree.stats;
    buf.push_str(&format!(
        "\n[{}L {}Li {}H {}F {}I {}Fr {}N total]",
        s.landmarks, s.links, s.headings, s.forms, s.images, s.iframes, s.total_nodes
    ));

    buf
}

fn find_title(node: &SemanticNode) -> Option<String> {
    if matches!(node.role, SemanticRole::Heading { level: 1 }) {
        return node.name.clone();
    }
    for child in &node.children {
        if let Some(t) = find_title(child) {
            return Some(t);
        }
    }
    None
}

fn collect_flat(
    node: &SemanticNode,
    actions: &mut Vec<String>,
    links: &mut Vec<String>,
    inputs: &mut Vec<String>,
    headings: &mut Vec<(u8, String)>,
    landmarks: &mut Vec<String>,
    frames: &mut Vec<String>,
) {
    match &node.role {
        SemanticRole::Heading { level } => {
            if let Some(name) = &node.name {
                let text = if name.len() > 80 {
                    format!("{}…", &name[..79])
                } else {
                    name.clone()
                };
                headings.push((*level, text));
            }
        }
        SemanticRole::Link => {
            if node.is_interactive {
                if let (Some(id), Some(name)) = (node.element_id, &node.name) {
                    let mut s = format!("[#{}] link \"{}\"", id, name);
                    if let Some(href) = &node.href {
                        s.push_str(&format!(" -> {}", truncate(href, 120)));
                    }
                    links.push(s);
                } else if let Some(href) = &node.href {
                    links.push(format!("link -> {}", truncate(href, 120)));
                }
            }
        }
        SemanticRole::Button => {
            if node.is_interactive {
                if let Some(id) = node.element_id {
                    let name = node.name.as_deref().unwrap_or("");
                    let mut s = format!("[#{}] btn \"{}\"", id, name);
                    if node.is_disabled {
                        s.push_str(" [off]");
                    }
                    actions.push(s);
                }
            }
        }
        SemanticRole::TextBox => {
            if node.is_interactive {
                if let Some(id) = node.element_id {
                    let name = node.name.as_deref().unwrap_or("");
                    let mut s = format!("[#{}] text \"{}\"", id, name);
                    if node.is_disabled {
                        s.push_str(" [off]");
                    }
                    inputs.push(s);
                }
            }
        }
        SemanticRole::Combobox => {
            if node.is_interactive {
                if let Some(id) = node.element_id {
                    let name = node.name.as_deref().unwrap_or("");
                    let mut s = format!("[#{}] select \"{}\"", id, name);
                    if node.is_disabled {
                        s.push_str(" [off]");
                    }
                    inputs.push(s);
                }
            }
        }
        SemanticRole::Checkbox => {
            if node.is_interactive {
                if let Some(id) = node.element_id {
                    let name = node.name.as_deref().unwrap_or("");
                    let mut s = format!("[#{}] check \"{}\"", id, name);
                    if node.is_disabled {
                        s.push_str(" [off]");
                    }
                    inputs.push(s);
                }
            }
        }
        SemanticRole::Radio => {
            if node.is_interactive {
                if let Some(id) = node.element_id {
                    let name = node.name.as_deref().unwrap_or("");
                    let mut s = format!("[#{}] radio \"{}\"", id, name);
                    if node.is_disabled {
                        s.push_str(" [off]");
                    }
                    inputs.push(s);
                }
            }
        }
        SemanticRole::Form => {
            let name = node.name.as_deref().unwrap_or("");
            let s = format!("form \"{}\" [{} fields]", name, count_inputs(node));
            landmarks.push(s);
        }
        SemanticRole::Dialog => {
            let name = node.name.as_deref().unwrap_or("");
            landmarks.push(format!("dialog \"{}\"", name));
        }
        role if role.is_landmark() && !matches!(role, SemanticRole::Form) => {
            let name = node.name.as_deref().unwrap_or("");
            landmarks.push(format!("{} \"{}\"", role.role_str(), name));
        }
        SemanticRole::IFrame => {
            let name = node.name.as_deref().unwrap_or("iframe");
            let mut s = format!("iframe \"{}\"", name);
            if let Some(href) = &node.href {
                s.push_str(&format!(" -> {}", truncate(href, 120)));
            }
            let child_actions = count_actions_in(node);
            if child_actions > 0 {
                s.push_str(&format!(" [{} actions]", child_actions));
            }
            frames.push(s);
        }
        _ => {}
    }

    for child in &node.children {
        collect_flat(child, actions, links, inputs, headings, landmarks, frames);
    }
}

fn count_inputs(node: &SemanticNode) -> usize {
    let mut count = 0;
    if matches!(
        node.role,
        SemanticRole::TextBox
            | SemanticRole::Checkbox
            | SemanticRole::Radio
            | SemanticRole::Combobox
    ) && node.is_interactive
    {
        count += 1;
    }
    for child in &node.children {
        count += count_inputs(child);
    }
    count
}

fn count_actions_in(node: &SemanticNode) -> usize {
    let mut count = 0;
    if node.is_interactive {
        count += 1;
    }
    for child in &node.children {
        count += count_actions_in(child);
    }
    count
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() > max {
        &s[..max]
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scraper::Html;

    #[test]
    fn test_llm_format_basic() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <h1>Test Page</h1>
                <nav><a href="/home">Home</a><a href="/about">About</a></nav>
                <main>
                    <h2>Section</h2>
                    <p>Hello world</p>
                    <form action="/search" method="get">
                        <input type="text" name="q" placeholder="Search">
                        <button type="submit">Go</button>
                    </form>
                    <button>Click Me</button>
                </main>
            </body></html>
        "#,
        );
        let tree = SemanticTree::build(&html, "https://example.com");
        let out = format_llm(&tree);

        assert!(out.contains("# Test Page"));
        assert!(out.contains("[#1]"));
        assert!(out.contains("link"));
        assert!(out.contains("text"));
        assert!(out.contains("btn"));
        assert!(out.contains("form"));
        assert!(out.contains("navigation"));
    }

    #[test]
    fn test_llm_format_no_interactive() {
        let html =
            Html::parse_document("<html><body><h1>No Actions</h1><p>Just text.</p></body></html>");
        let tree = SemanticTree::build(&html, "https://example.com");
        let out = format_llm(&tree);

        assert!(out.contains("# No Actions"));
        assert!(!out.contains("-- Actions --"));
        assert!(!out.contains("-- Links --"));
    }

    #[test]
    fn test_llm_format_compact() {
        let html = Html::parse_document(
            r#"
            <html><body>
                <a href="https://example.com/very/long/path/that/goes/on/and/on">Long Link</a>
                <input type="text" name="field_with_long_name_placeholder_attribute">
            </body></html>
        "#,
        );
        let tree = SemanticTree::build(&html, "https://example.com");
        let out = format_llm(&tree);

        let line_count = out.lines().count();
        assert!(
            line_count < 15,
            "LLM output should be compact, got {} lines",
            line_count
        );
    }

    #[test]
    fn test_llm_title_from_h1() {
        let html =
            Html::parse_document("<html><body><h1>My Page Title</h1><p>Body</p></body></html>");
        let tree = SemanticTree::build(&html, "https://example.com");
        let out = format_llm(&tree);

        assert!(out.starts_with("# My Page Title"));
    }

    #[test]
    fn test_llm_no_title_when_no_h1() {
        let html = Html::parse_document("<html><body><h2>Subtitle</h2><p>Body</p></body></html>");
        let tree = SemanticTree::build(&html, "https://example.com");
        let out = format_llm(&tree);

        assert!(!out
            .lines()
            .any(|l| l.starts_with("# ") && !l.starts_with("## ")));
        assert!(out.contains("## Subtitle"));
    }

    #[test]
    fn test_llm_heading_levels() {
        let html = Html::parse_document(
            "<html><body><h1>One</h1><h2>Two</h2><h3>Three</h3><h5>Five</h5></body></html>",
        );
        let tree = SemanticTree::build(&html, "https://example.com");
        let out = format_llm(&tree);

        assert!(out.contains("## One"));
        assert!(out.contains("## Two"));
        assert!(out.contains("### Three"));
        assert!(out.contains("#### Five"));
    }

    #[test]
    fn test_llm_links_section() {
        let html = Html::parse_document(
            r#"<html><body>
                <a href="/home">Home</a>
                <a href="/about">About</a>
                <a href="https://external.com">External</a>
            </body></html>"#,
        );
        let tree = SemanticTree::build(&html, "https://example.com");
        let out = format_llm(&tree);

        assert!(out.contains("-- Links --"));
        assert!(out.contains("[#1]"));
        assert!(out.contains("Home"));
        assert!(out.contains("-> "));
    }

    #[test]
    fn test_llm_inputs_section() {
        let html = Html::parse_document(
            r#"<html><body>
                <form>
                    <input type="text" name="q">
                    <input type="email" name="email">
                    <select name="lang"><option value="en">English</option></select>
                </form>
            </body></html>"#,
        );
        let tree = SemanticTree::build(&html, "https://example.com");
        let out = format_llm(&tree);

        assert!(out.contains("-- Inputs --"));
        assert!(out.contains("[#1] text"));
        assert!(out.contains("[#2] text"));
        assert!(out.contains("[#3] select"));
    }

    #[test]
    fn test_llm_checkbox_and_radio() {
        let html = Html::parse_document(
            r#"<html><body>
                <form>
                    <label><input type="checkbox" name="agree"> I agree</label>
                    <label><input type="radio" name="plan" value="free"> Free</label>
                </form>
            </body></html>"#,
        );
        let tree = SemanticTree::build(&html, "https://example.com");
        let out = format_llm(&tree);

        assert!(out.contains("check"));
        assert!(out.contains("radio"));
    }

    #[test]
    fn test_llm_disabled_elements() {
        let html = Html::parse_document(
            r#"<html><body>
                <button disabled>Disabled Btn</button>
                <input type="text" disabled name="off_field">
            </body></html>"#,
        );
        let tree = SemanticTree::build(&html, "https://example.com");
        let out = format_llm(&tree);

        assert!(out.contains("[off]"));
    }

    #[test]
    fn test_llm_stats_line() {
        let html = Html::parse_document(
            r#"<html><body>
                <h1>Title</h1>
                <nav><a href="/a">A</a><a href="/b">B</a></nav>
                <form><input name="x"><button type="submit">Go</button></form>
                <img src="pic.png" alt="Photo">
            </body></html>"#,
        );
        let tree = SemanticTree::build(&html, "https://example.com");
        let out = format_llm(&tree);

        assert!(out.contains("["));
        assert!(out.contains("L "));
        assert!(out.contains("Li "));
        assert!(out.contains("H "));
        assert!(out.contains("F "));
        assert!(out.contains("I "));
        assert!(out.contains("N total]"));
    }

    #[test]
    fn test_llm_dialog_in_regions() {
        let html = Html::parse_document(
            "<html><body><dialog open><h2>Dialog Title</h2><button>Close</button></dialog></body></html>"
        );
        let tree = SemanticTree::build(&html, "https://example.com");
        let out = format_llm(&tree);

        assert!(out.contains("dialog"));
        assert!(out.contains("Dialog Title"));
    }

    #[test]
    fn test_llm_form_with_field_count() {
        let html = Html::parse_document(
            r#"<html><body>
                <form action="/submit" method="post">
                    <input type="text" name="a">
                    <input type="email" name="b">
                    <input type="password" name="c">
                    <button type="submit">Submit</button>
                </form>
            </body></html>"#,
        );
        let tree = SemanticTree::build(&html, "https://example.com");
        let out = format_llm(&tree);

        assert!(out.contains("[3 fields]"));
    }

    #[test]
    fn test_llm_link_with_long_url_truncated() {
        let long_url = "https://example.com/very/long/path/that/keeps/going/and/going/and/going/and/going/and/going";
        let html = Html::parse_document(&format!(
            r#"<html><body><a href="{}">Link</a></body></html>"#,
            long_url
        ));
        let tree = SemanticTree::build(&html, "https://example.com");
        let out = format_llm(&tree);

        assert!(out.contains("-> "));
        let link_line: Vec<_> = out.lines().filter(|l| l.contains("-> ")).collect();
        assert!(!link_line.is_empty());
        for line in link_line {
            assert!(
                line.len() < 200,
                "link line should be reasonably short: {}",
                line.len()
            );
        }
    }

    #[test]
    fn test_llm_empty_html() {
        let html = Html::parse_document("<html><body></body></html>");
        let tree = SemanticTree::build(&html, "https://example.com");
        let out = format_llm(&tree);

        assert!(out.contains("[0L"));
        assert!(!out.contains("--"));
    }

    #[test]
    fn test_llm_multiple_buttons_all_shown() {
        let html = Html::parse_document(
            r#"<html><body>
                <button>Save</button>
                <button>Cancel</button>
                <button>Delete</button>
            </body></html>"#,
        );
        let tree = SemanticTree::build(&html, "https://example.com");
        let out = format_llm(&tree);

        assert!(out.contains("Save"));
        assert!(out.contains("Cancel"));
        assert!(out.contains("Delete"));
    }

    #[test]
    fn test_llm_no_duplicate_regions() {
        let html = Html::parse_document(
            r#"<html><body>
                <nav><a href="/home">Home</a></nav>
                <footer><a href="/privacy">Privacy</a></footer>
            </body></html>"#,
        );
        let tree = SemanticTree::build(&html, "https://example.com");
        let out = format_llm(&tree);

        let region_count = out
            .lines()
            .filter(|l| l.contains("navigation") || l.contains("contentinfo"))
            .count();
        assert_eq!(region_count, 2);
    }
}
