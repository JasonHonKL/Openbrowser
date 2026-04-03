use std::collections::BTreeMap;
use std::collections::BTreeSet;

use pardus_core::{SemanticNode, SemanticRole, SemanticTree};
use scraper::Html;
use url::Url;

use crate::state::{Fingerprint, ViewStateId};

/// Compute the full fingerprint for a page.
pub fn compute_fingerprint(
    page_url: &str,
    tree: &SemanticTree,
    resource_urls: &BTreeSet<String>,
) -> (Fingerprint, ViewStateId) {
    let parsed = Url::parse(page_url).ok();
    let url_path = parsed.as_ref().map(|u| u.path().to_string()).unwrap_or_default();
    let fragment = parsed.as_ref().and_then(|u| u.fragment().map(|f| f.to_string()));

    let content_query_params = extract_content_params(parsed.as_ref());

    let tree_hash = hash_tree_structure(tree);
    let resource_set_hash = hash_resource_set(resource_urls);

    let fp = Fingerprint {
        url_path,
        content_query_params,
        tree_hash,
        resource_set_hash,
        fragment,
    };

    let id = compute_view_state_id(&fp);
    (fp, id)
}

/// Discover subresource URLs from HTML.
pub fn discover_resources(html: &Html, base_url: &str) -> BTreeSet<String> {
    let records = pardus_debug::discover::discover_subresources(html, base_url, 0);
    records.into_iter().map(|r| r.url).collect()
}

/// Extract query params that affect page content (pagination params).
fn extract_content_params(url: Option<&Url>) -> BTreeMap<String, String> {
    let Some(url) = url else { return BTreeMap::new() };

    let pagination_keys = ["page", "offset", "start", "p"];
    let mut params = BTreeMap::new();
    for (k, v) in url.query_pairs() {
        let key = k.to_string();
        if pagination_keys.contains(&key.as_str()) {
            params.insert(key, v.to_string());
        }
    }
    params
}

/// Hash the structural skeleton of a semantic tree.
/// For each node: "{role}:{tag}:{is_interactive}:{children_count}"
/// Does NOT include name, href, action, or text content.
fn hash_tree_structure(tree: &SemanticTree) -> String {
    let mut skeleton = String::new();
    walk_skeleton(&tree.root, &mut skeleton);
    let hash = blake3::hash(skeleton.as_bytes());
    hash.to_hex().to_string()
}

fn walk_skeleton(node: &SemanticNode, out: &mut String) {
    out.push_str(&format!(
        "{}:{}:{}:{}\n",
        role_str(&node.role),
        node.tag,
        node.is_interactive,
        node.children.len()
    ));
    for child in &node.children {
        walk_skeleton(child, out);
    }
}

fn role_str(role: &SemanticRole) -> String {
    match role {
        SemanticRole::Document => "document".to_string(),
        SemanticRole::Banner => "banner".to_string(),
        SemanticRole::Navigation => "navigation".to_string(),
        SemanticRole::Main => "main".to_string(),
        SemanticRole::ContentInfo => "contentinfo".to_string(),
        SemanticRole::Complementary => "complementary".to_string(),
        SemanticRole::Region => "region".to_string(),
        SemanticRole::Form => "form".to_string(),
        SemanticRole::Search => "search".to_string(),
        SemanticRole::Article => "article".to_string(),
        SemanticRole::Heading { .. } => "heading".to_string(),
        SemanticRole::Link => "link".to_string(),
        SemanticRole::Button => "button".to_string(),
        SemanticRole::TextBox => "textbox".to_string(),
        SemanticRole::Checkbox => "checkbox".to_string(),
        SemanticRole::Radio => "radio".to_string(),
        SemanticRole::Combobox => "combobox".to_string(),
        SemanticRole::List => "list".to_string(),
        SemanticRole::ListItem => "listitem".to_string(),
        SemanticRole::Table => "table".to_string(),
        SemanticRole::Row => "row".to_string(),
        SemanticRole::Cell => "cell".to_string(),
        SemanticRole::ColumnHeader => "columnheader".to_string(),
        SemanticRole::RowHeader => "rowheader".to_string(),
        SemanticRole::Image => "img".to_string(),
        SemanticRole::Dialog => "dialog".to_string(),
        SemanticRole::IFrame => "iframe".to_string(),
        SemanticRole::Generic => "generic".to_string(),
        SemanticRole::StaticText => "text".to_string(),
        SemanticRole::Other(s) => s.clone(),
    }
}

/// Hash a sorted set of resource URLs.
fn hash_resource_set(resources: &BTreeSet<String>) -> String {
    let concatenated: String = resources.iter().map(|u| u.as_str()).collect::<Vec<_>>().join("\n");
    let hash = blake3::hash(concatenated.as_bytes());
    hash.to_hex().to_string()
}

/// Compute ViewStateId from fingerprint components.
fn compute_view_state_id(fp: &Fingerprint) -> ViewStateId {
    let mut composite = String::new();
    composite.push_str(&fp.url_path);
    composite.push('|');
    for (k, v) in &fp.content_query_params {
        composite.push_str(&format!("{}={}", k, v));
    }
    composite.push('|');
    composite.push_str(&fp.tree_hash);
    composite.push('|');
    composite.push_str(&fp.resource_set_hash);
    if let Some(ref frag) = fp.fragment {
        composite.push('|');
        composite.push_str(frag);
    }
    let hash = blake3::hash(composite.as_bytes());
    ViewStateId(hash.to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use scraper::Html;

    fn build_tree(html: &str) -> SemanticTree {
        let doc = Html::parse_document(html);
        SemanticTree::build(&doc, "https://example.com")
    }

    #[test]
    fn test_same_structure_same_hash() {
        let t1 = build_tree(r#"<html><body><nav><a href="/x">A</a></nav><main><h1>Hello</h1></main></body></html>"#);
        let t2 = build_tree(r#"<html><body><nav><a href="/x">B</a></nav><main><h1>World</h1></main></body></html>"#);
        // Same structure, different text → same hash
        assert_eq!(hash_tree_structure(&t1), hash_tree_structure(&t2));
    }

    #[test]
    fn test_different_structure_different_hash() {
        let t1 = build_tree(r#"<html><body><nav><a href="/x">A</a></nav></body></html>"#);
        let t2 = build_tree(r#"<html><body><nav><a href="/x">A</a><a href="/y">B</a></nav></body></html>"#);
        // Different structure (1 link vs 2 links)
        assert_ne!(hash_tree_structure(&t1), hash_tree_structure(&t2));
    }

    #[test]
    fn test_resource_set_hash_consistent() {
        let mut set1 = BTreeSet::new();
        set1.insert("https://example.com/a.css".to_string());
        set1.insert("https://example.com/b.js".to_string());

        let mut set2 = BTreeSet::new();
        set2.insert("https://example.com/b.js".to_string());
        set2.insert("https://example.com/a.css".to_string());

        assert_eq!(hash_resource_set(&set1), hash_resource_set(&set2));
    }

    #[test]
    fn test_view_state_id_deterministic() {
        let tree = build_tree("<html><body><h1>Test</h1></body></html>");
        let mut resources = BTreeSet::new();
        resources.insert("https://example.com/style.css".to_string());

        let (fp1, id1) = compute_fingerprint("https://example.com/", &tree, &resources);
        let (fp2, id2) = compute_fingerprint("https://example.com/", &tree, &resources);

        assert_eq!(id1, id2);
        assert_eq!(fp1.tree_hash, fp2.tree_hash);
    }

    #[test]
    fn test_different_urls_different_ids() {
        let tree = build_tree("<html><body><h1>Test</h1></body></html>");
        let resources = BTreeSet::new();

        let (_, id1) = compute_fingerprint("https://example.com/", &tree, &resources);
        let (_, id2) = compute_fingerprint("https://example.com/about", &tree, &resources);

        assert_ne!(id1, id2);
    }

    #[test]
    fn test_fragment_creates_different_id() {
        let tree = build_tree("<html><body><h1>Test</h1></body></html>");
        let resources = BTreeSet::new();

        let (_, id1) = compute_fingerprint("https://example.com/#section1", &tree, &resources);
        let (_, id2) = compute_fingerprint("https://example.com/#section2", &tree, &resources);

        assert_ne!(id1, id2);
    }
}
