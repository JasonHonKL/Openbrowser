//! Tests for semantic element extraction (roles, actions, interactivity, names).
//!
//! Exercises the shared `compute_role`, `check_interactive`, `compute_action`,
//! and `compute_name_from_attrs` functions from `semantic::extract`.

use pardus_core::semantic::extract::{
    compute_action, compute_name_from_attrs, compute_role, check_interactive,
    AttrMap,
};
use pardus_core::semantic::tree::SemanticRole;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn map(tag: &str, attrs: &[(&str, &str)]) -> AttrMap {
    AttrMap::new(
        tag.to_string(),
        attrs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
    )
}

// ---------------------------------------------------------------------------
// compute_role
// ---------------------------------------------------------------------------

#[test]
fn role_nav() {
    let m = map("nav", &[]);
    assert_eq!(compute_role("nav", &m, false), SemanticRole::Navigation);
}

#[test]
fn role_main() {
    let m = map("main", &[]);
    assert_eq!(compute_role("main", &m, false), SemanticRole::Main);
}

#[test]
fn role_header_footer() {
    let h = map("header", &[]);
    assert_eq!(compute_role("header", &h, false), SemanticRole::Banner);
    let f = map("footer", &[]);
    assert_eq!(compute_role("footer", &f, false), SemanticRole::ContentInfo);
}

#[test]
fn role_aside() {
    let m = map("aside", &[]);
    assert_eq!(compute_role("aside", &m, false), SemanticRole::Complementary);
}

#[test]
fn role_search() {
    let m = map("search", &[]);
    assert_eq!(compute_role("search", &m, false), SemanticRole::Search);
}

#[test]
fn role_section_with_name() {
    let m = map("section", &[("aria-label", "News")]);
    assert_eq!(compute_role("section", &m, true), SemanticRole::Region);
}

#[test]
fn role_section_without_name() {
    let m = map("section", &[]);
    assert_eq!(compute_role("section", &m, false), SemanticRole::Generic);
}

#[test]
fn role_article() {
    let m = map("article", &[]);
    assert_eq!(compute_role("article", &m, false), SemanticRole::Article);
}

#[test]
fn role_form() {
    let m = map("form", &[]);
    assert_eq!(compute_role("form", &m, false), SemanticRole::Form);
    assert_eq!(compute_role("form", &m, true), SemanticRole::Form);
}

#[test]
fn role_headings() {
    for (tag, level) in [("h1", 1), ("h2", 2), ("h3", 3), ("h4", 4), ("h5", 5), ("h6", 6)] {
        let m = map(tag, &[]);
        assert_eq!(compute_role(tag, &m, false), SemanticRole::Heading { level });
    }
}

#[test]
fn role_link() {
    let m = map("a", &[("href", "/about")]);
    assert_eq!(compute_role("a", &m, false), SemanticRole::Link);
}

#[test]
fn role_button() {
    let m = map("button", &[]);
    assert_eq!(compute_role("button", &m, false), SemanticRole::Button);
}

#[test]
fn role_input_text() {
    let m = map("input", &[("type", "text")]);
    assert_eq!(compute_role("input", &m, false), SemanticRole::TextBox);
}

#[test]
fn role_input_checkbox() {
    let m = map("input", &[("type", "checkbox")]);
    assert_eq!(compute_role("input", &m, false), SemanticRole::Checkbox);
}

#[test]
fn role_input_radio() {
    let m = map("input", &[("type", "radio")]);
    assert_eq!(compute_role("input", &m, false), SemanticRole::Radio);
}

#[test]
fn role_input_file() {
    let m = map("input", &[("type", "file")]);
    assert_eq!(compute_role("input", &m, false), SemanticRole::FileInput);
}

#[test]
fn role_input_submit() {
    let m = map("input", &[("type", "submit")]);
    assert_eq!(compute_role("input", &m, false), SemanticRole::Button);
}

#[test]
fn role_input_no_type_defaults_to_text() {
    let m = map("input", &[]);
    assert_eq!(compute_role("input", &m, false), SemanticRole::TextBox);
}

#[test]
fn role_select() {
    let m = map("select", &[]);
    assert_eq!(compute_role("select", &m, false), SemanticRole::Combobox);
}

#[test]
fn role_textarea() {
    let m = map("textarea", &[]);
    assert_eq!(compute_role("textarea", &m, false), SemanticRole::TextBox);
}

#[test]
fn role_img() {
    let m = map("img", &[]);
    assert_eq!(compute_role("img", &m, false), SemanticRole::Image);
}

#[test]
fn role_list() {
    assert_eq!(compute_role("ul", &map("ul", &[]), false), SemanticRole::List);
    assert_eq!(compute_role("ol", &map("ol", &[]), false), SemanticRole::List);
}

#[test]
fn role_listitem() {
    assert_eq!(compute_role("li", &map("li", &[]), false), SemanticRole::ListItem);
}

#[test]
fn role_table() {
    assert_eq!(compute_role("table", &map("table", &[]), false), SemanticRole::Table);
}

#[test]
fn role_dialog() {
    assert_eq!(compute_role("dialog", &map("dialog", &[]), false), SemanticRole::Dialog);
}

#[test]
fn role_div_is_generic() {
    assert_eq!(compute_role("div", &map("div", &[]), false), SemanticRole::Generic);
    assert_eq!(compute_role("span", &map("span", &[]), false), SemanticRole::Generic);
}

#[test]
fn role_explicit_role_overrides_tag() {
    let m = map("div", &[("role", "button")]);
    assert_eq!(compute_role("div", &m, false), SemanticRole::Button);
}

#[test]
fn role_explicit_link() {
    let m = map("span", &[("role", "link")]);
    assert_eq!(compute_role("span", &m, false), SemanticRole::Link);
}

// ---------------------------------------------------------------------------
// check_interactive
// ---------------------------------------------------------------------------

#[test]
fn interactive_anchor_with_href() {
    let m = map("a", &[("href", "/page")]);
    assert!(check_interactive("a", &m));
}

#[test]
fn anchor_without_href_not_interactive() {
    let m = map("a", &[]);
    assert!(!check_interactive("a", &m));
}

#[test]
fn interactive_button() {
    assert!(check_interactive("button", &map("button", &[])));
}

#[test]
fn interactive_input() {
    assert!(check_interactive("input", &map("input", &[("type", "text")])));
}

#[test]
fn interactive_select() {
    assert!(check_interactive("select", &map("select", &[])));
}

#[test]
fn interactive_textarea() {
    assert!(check_interactive("textarea", &map("textarea", &[])));
}

#[test]
fn interactive_details() {
    assert!(check_interactive("details", &map("details", &[])));
}

#[test]
fn not_interactive_div() {
    assert!(!check_interactive("div", &map("div", &[])));
}

#[test]
fn not_interactive_span() {
    assert!(!check_interactive("span", &map("span", &[])));
}

#[test]
fn interactive_aria_button() {
    let m = map("div", &[("role", "button")]);
    assert!(check_interactive("div", &m));
}

#[test]
fn interactive_aria_link() {
    let m = map("span", &[("role", "link")]);
    assert!(check_interactive("span", &m));
}

#[test]
fn interactive_aria_textbox() {
    let m = map("div", &[("role", "textbox")]);
    assert!(check_interactive("div", &m));
}

#[test]
fn interactive_aria_checkbox() {
    let m = map("div", &[("role", "checkbox")]);
    assert!(check_interactive("div", &m));
}

#[test]
fn interactive_aria_radio() {
    let m = map("div", &[("role", "radio")]);
    assert!(check_interactive("div", &m));
}

#[test]
fn interactive_aria_combobox() {
    let m = map("div", &[("role", "combobox")]);
    assert!(check_interactive("div", &m));
}

#[test]
fn interactive_aria_switch() {
    let m = map("div", &[("role", "switch")]);
    assert!(check_interactive("div", &m));
}

#[test]
fn interactive_aria_tab() {
    let m = map("div", &[("role", "tab")]);
    assert!(check_interactive("div", &m));
}

#[test]
fn interactive_aria_menuitem() {
    let m = map("div", &[("role", "menuitem")]);
    assert!(check_interactive("div", &m));
}

#[test]
fn interactive_aria_option() {
    let m = map("li", &[("role", "option")]);
    assert!(check_interactive("li", &m));
}

#[test]
fn not_interactive_non_aria_role() {
    let m = map("div", &[("role", "heading")]);
    assert!(!check_interactive("div", &m));
}

#[test]
fn interactive_tabindex_zero() {
    let m = map("div", &[("tabindex", "0")]);
    assert!(check_interactive("div", &m));
}

#[test]
fn interactive_tabindex_positive() {
    let m = map("div", &[("tabindex", "5")]);
    assert!(check_interactive("div", &m));
}

#[test]
fn not_interactive_tabindex_negative() {
    let m = map("div", &[("tabindex", "-1")]);
    assert!(!check_interactive("div", &m));
}

#[test]
fn not_interactive_tabindex_invalid() {
    let m = map("div", &[("tabindex", "abc")]);
    assert!(!check_interactive("div", &m));
}

// ---------------------------------------------------------------------------
// compute_action
// ---------------------------------------------------------------------------

#[test]
fn action_link() {
    let m = map("a", &[("href", "/page")]);
    assert_eq!(compute_action("a", &m, true), Some("navigate".into()));
}

#[test]
fn action_button() {
    let m = map("button", &[]);
    assert_eq!(compute_action("button", &m, true), Some("click".into()));
}

#[test]
fn action_input_text() {
    let m = map("input", &[("type", "text")]);
    assert_eq!(compute_action("input", &m, true), Some("fill".into()));
}

#[test]
fn action_input_submit() {
    let m = map("input", &[("type", "submit")]);
    assert_eq!(compute_action("input", &m, true), Some("click".into()));
}

#[test]
fn action_input_checkbox() {
    let m = map("input", &[("type", "checkbox")]);
    assert_eq!(compute_action("input", &m, true), Some("toggle".into()));
}

#[test]
fn action_input_radio() {
    let m = map("input", &[("type", "radio")]);
    assert_eq!(compute_action("input", &m, true), Some("toggle".into()));
}

#[test]
fn action_input_file() {
    let m = map("input", &[("type", "file")]);
    assert_eq!(compute_action("input", &m, true), Some("upload".into()));
}

#[test]
fn action_input_reset() {
    let m = map("input", &[("type", "reset")]);
    assert_eq!(compute_action("input", &m, true), Some("click".into()));
}

#[test]
fn action_input_image() {
    let m = map("input", &[("type", "image")]);
    assert_eq!(compute_action("input", &m, true), Some("click".into()));
}

#[test]
fn action_select() {
    let m = map("select", &[]);
    assert_eq!(compute_action("select", &m, true), Some("select".into()));
}

#[test]
fn action_textarea() {
    let m = map("textarea", &[]);
    assert_eq!(compute_action("textarea", &m, true), Some("fill".into()));
}

#[test]
fn action_div_not_interactive() {
    let m = map("div", &[]);
    assert_eq!(compute_action("div", &m, false), None);
}

#[test]
fn action_div_with_role_button() {
    let m = map("div", &[("role", "button")]);
    assert_eq!(compute_action("div", &m, true), Some("click".into()));
}

#[test]
fn action_div_with_role_link() {
    let m = map("div", &[("role", "link")]);
    assert_eq!(compute_action("div", &m, true), Some("navigate".into()));
}

#[test]
fn action_div_with_role_textbox() {
    let m = map("div", &[("role", "textbox")]);
    assert_eq!(compute_action("div", &m, true), Some("fill".into()));
}

#[test]
fn action_div_with_unknown_role() {
    let m = map("div", &[("role", "article")]);
    assert_eq!(compute_action("div", &m, true), None);
}

// ---------------------------------------------------------------------------
// compute_name_from_attrs
// ---------------------------------------------------------------------------

#[test]
fn name_aria_label() {
    let m = map("button", &[("aria-label", "Close")]);
    assert_eq!(compute_name_from_attrs(&m), Some("Close".into()));
}

#[test]
fn name_aria_label_whitespace_only() {
    let m = map("button", &[("aria-label", "   ")]);
    assert_eq!(compute_name_from_attrs(&m), None);
}

#[test]
fn name_title() {
    let m = map("a", &[("title", "Go to docs")]);
    assert_eq!(compute_name_from_attrs(&m), Some("Go to docs".into()));
}

#[test]
fn name_aria_label_over_title() {
    let m = map("a", &[("aria-label", "Docs"), ("title", "Documentation")]);
    assert_eq!(compute_name_from_attrs(&m), Some("Docs".into()));
}

#[test]
fn name_img_alt() {
    let m = map("img", &[("alt", "Logo")]);
    assert_eq!(compute_from_attrs(&m), Some("Logo".into()));
}

#[test]
fn name_img_empty_alt() {
    let m = map("img", &[("alt", "")]);
    assert_eq!(compute_name_from_attrs(&m), None);
}

#[test]
fn name_input_placeholder() {
    let m = map("input", &[("type", "text"), ("placeholder", "Search")]);
    assert_eq!(compute_name_from_attrs(&m), Some("Search".into()));
}

#[test]
fn name_textarea_placeholder() {
    let m = map("textarea", &[("placeholder", "Comment")]);
    assert_eq!(compute_name_from_attrs(&m), Some("Comment".into()));
}

#[test]
fn name_input_submit_value() {
    let m = map("input", &[("type", "submit"), ("value", "Send")]);
    assert_eq!(compute_name_from_attrs(&m), Some("Send".into()));
}

#[test]
fn name_input_submit_no_value() {
    let m = map("input", &[("type", "submit")]);
    assert_eq!(compute_name_from_attrs(&m), Some("Submit".into()));
}

#[test]
fn name_input_reset_no_value() {
    let m = map("input", &[("type", "reset")]);
    assert_eq!(compute_name_from_attrs(&m), Some("Reset".into()));
}

#[test]
fn name_input_button_no_value() {
    let m = map("input", &[("type", "button")]);
    assert_eq!(compute_name_from_attrs(&m), Some("Button".into()));
}

#[test]
fn name_input_name_fallback() {
    let m = map("input", &[("type", "text"), ("name", "email")]);
    assert_eq!(compute_name_from_attrs(&m), Some("email".into()));
}

#[test]
fn name_select_name_fallback() {
    let m = map("select", &[("name", "country")]);
    assert_eq!(compute_name_from_attrs(&m), Some("country".into()));
}

#[test]
fn name_textarea_name_fallback() {
    let m = map("textarea", &[("name", "comment")]);
    assert_eq!(compute_name_from_attrs(&m), Some("comment".into()));
}

#[test]
fn name_no_attrs() {
    let m = map("div", &[]);
    assert_eq!(compute_name_from_attrs(&m), None);
}

// ---------------------------------------------------------------------------
// AttrMap case-insensitivity
// ---------------------------------------------------------------------------

#[test]
fn attrmap_case_insensitive() {
    let m = AttrMap::new("input".into(), vec![
        ("Type".into(), "Text".into()),
        ("NAME".into(), "email".into()),
    ]);
    assert_eq!(m.attr("type"), Some("Text"));
    assert_eq!(m.attr("TYPE"), Some("Text"));
    assert_eq!(m.attr("name"), Some("email"));
    assert_eq!(m.attr("Name"), Some("email"));
}

#[test]
fn attrmap_missing_attr() {
    let m = AttrMap::new("div".into(), vec![]);
    assert_eq!(m.attr("id"), None);
}

#[test]
fn attrmap_tag_name() {
    let m = AttrMap::new("button".into(), vec![]);
    assert_eq!(m.tag_name(), "button");
}
