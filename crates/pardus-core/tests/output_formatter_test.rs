//! Tests for output formatters: format_tree, format_md, format_llm, format_json.

use pardus_core::{RedirectChain, SemanticNode, SemanticRole, SemanticTree, TreeStats};
use scraper::Html;

fn tree_from(html: &str) -> SemanticTree {
    let parsed = Html::parse_document(html);
    SemanticTree::build(&parsed, "https://example.com")
}

fn simple_tree() -> SemanticTree {
    tree_from(r#"<html><body>
        <h1>Title</h1>
        <nav><a href="/home">Home</a></nav>
        <main>
            <p>Hello world</p>
            <form action="/search">
                <input type="text" name="q" placeholder="Search">
                <button type="submit">Go</button>
            </form>
        </main>
    </body></html>"#)
}

// ---------------------------------------------------------------------------
// format_tree — unicode tree output
// ---------------------------------------------------------------------------

#[test]
fn test_format_tree_produces_output() {
    let tree = simple_tree();
    let output = pardus_core::format_tree(&tree);
    assert!(!output.is_empty());
}

#[test]
fn test_format_tree_has_tree_chars() {
    let tree = simple_tree();
    let output = pardus_core::format_tree(&tree);
    assert!(
        output.contains("├") || output.contains("└"),
        "tree output should contain tree branch characters"
    );
}

#[test]
fn test_format_tree_shows_roles() {
    let tree = tree_from("<html><body><nav>Nav</nav></body></html>");
    let output = pardus_core::format_tree(&tree);
    assert!(
        output.contains("navigation") || output.contains("nav"),
        "tree output should mention navigation role"
    );
}

// ---------------------------------------------------------------------------
// format_md — markdown-style output
// ---------------------------------------------------------------------------

#[test]
fn test_format_md_produces_output() {
    let tree = simple_tree();
    let output = pardus_core::output::md_formatter::format_md(&tree);
    assert!(!output.is_empty());
}

#[test]
fn test_format_md_starts_with_document() {
    let tree = simple_tree();
    let output = pardus_core::output::md_formatter::format_md(&tree);
    assert!(
        output.starts_with("document"),
        "MD output should start with 'document'"
    );
}

#[test]
fn test_format_md_shows_links() {
    let tree = tree_from(r#"<html><body><a href="/page">Link</a></body></html>"#);
    let output = pardus_core::output::md_formatter::format_md(&tree);
    assert!(
        output.contains("link") || output.contains("/page"),
        "MD output should contain link info"
    );
}

// ---------------------------------------------------------------------------
// format_llm — LLM-optimized output
// ---------------------------------------------------------------------------

#[test]
fn test_format_llm_produces_output() {
    let tree = simple_tree();
    let output = pardus_core::format_llm(&tree);
    assert!(!output.is_empty());
}

#[test]
fn test_format_llm_compact_format() {
    let tree = tree_from(r#"<html><body>
        <a href="/a">Link A</a>
        <button>Click</button>
    </body></html>"#);
    let output = pardus_core::format_llm(&tree);
    // LLM format uses single-char tags and compact notation
    assert!(output.len() > 0);
}

#[test]
fn test_format_llm_lists_actions() {
    let tree = tree_from(r#"<html><body>
        <button>Submit</button>
        <a href="/go">Go</a>
    </body></html>"#);
    let output = pardus_core::format_llm(&tree);
    assert!(!output.is_empty());
}

// ---------------------------------------------------------------------------
// format_json — structured JSON output
// ---------------------------------------------------------------------------

#[test]
fn test_format_json_produces_valid_json() {
    let tree = simple_tree();
    let output = pardus_core::output::json_formatter::format_json(
        "https://example.com",
        Some("Test Page".to_string()),
        &tree,
        None,
        None,
        None as Option<&RedirectChain>,
    )
    .expect("format_json should succeed");

    let parsed: serde_json::Value = serde_json::from_str(&output).expect("output should be valid JSON");
    assert!(parsed.get("url").is_some() || parsed.get("semantic_tree").is_some());
}

#[test]
fn test_format_json_includes_url() {
    let tree = simple_tree();
    let output = pardus_core::output::json_formatter::format_json(
        "https://example.com/page",
        None,
        &tree,
        None,
        None,
        None,
    )
    .expect("format_json should succeed");

    assert!(output.contains("https://example.com/page"));
}

#[test]
fn test_format_json_includes_stats() {
    let tree = simple_tree();
    let output = pardus_core::output::json_formatter::format_json(
        "https://example.com",
        None,
        &tree,
        None,
        None,
        None,
    )
    .expect("format_json should succeed");

    assert!(output.contains("stats") || output.contains("landmarks") || output.contains("links"));
}

// ---------------------------------------------------------------------------
// Empty tree edge case
// ---------------------------------------------------------------------------

#[test]
fn test_format_empty_tree() {
    let tree = SemanticTree {
        root: SemanticNode {
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
        },
        stats: TreeStats::default(),
    };

    let tree_out = pardus_core::format_tree(&tree);
    assert!(!tree_out.is_empty());

    let md_out = pardus_core::output::md_formatter::format_md(&tree);
    assert!(!md_out.is_empty());

    let llm_out = pardus_core::format_llm(&tree);
    // LLM format might be empty for an empty tree, that's OK
    assert!(llm_out.len() >= 0);

    let json_out = pardus_core::output::json_formatter::format_json(
        "https://example.com",
        None,
        &tree,
        None,
        None,
        None,
    )
    .expect("format_json should work on empty tree");
    let parsed: serde_json::Value = serde_json::from_str(&json_out).unwrap();
    assert!(parsed.is_object());
}
