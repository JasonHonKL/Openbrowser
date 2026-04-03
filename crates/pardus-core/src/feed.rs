use crate::semantic::tree::{SemanticNode, SemanticRole, SemanticTree, TreeStats};

/// Detect whether the given bytes represent an RSS or Atom feed.
/// Checks for common feed root elements and content-type hints.
pub fn is_feed_content(body: &[u8], content_type: Option<&str>) -> bool {
    // Content-type based detection
    if let Some(ct) = content_type {
        let ct_lower = ct.to_lowercase();
        if ct_lower.contains("application/rss+xml")
            || ct_lower.contains("application/atom+xml")
            || ct_lower.contains("application/feed+json")
        {
            return true;
        }
    }

    // Content-based detection: look for feed root elements in the first 1KB
    let head = if body.len() > 1024 {
        &body[..1024]
    } else {
        body
    };
    let Ok(text) = std::str::from_utf8(head) else {
        return false;
    };

    let lower = text.to_lowercase();

    // RSS: <rss or <rdf:RDF with namespace
    if lower.contains("<rss") || lower.contains("<rdf:rdf") {
        return true;
    }

    // Atom: <feed with xmlns="http://www.w3.org/2005/Atom"
    if lower.contains("<feed") && lower.contains("http://www.w3.org/2005/atom") {
        return true;
    }

    false
}

/// Parse RSS/Atom feed bytes into a semantic tree.
///
/// Returns the tree and the feed title.
pub fn extract_feed_tree(bytes: &[u8]) -> anyhow::Result<(SemanticTree, Option<String>)> {
    let cursor = std::io::Cursor::new(bytes);
    let feed = feed_rs::parser::parse(cursor)
        .map_err(|e| anyhow::anyhow!("Failed to parse feed: {}", e))?;

    let mut stats = TreeStats::default();
    let mut item_nodes = Vec::new();

    let feed_title = feed.title.as_ref().map(|t| t.content.clone());

    for entry in &feed.entries {
        let mut child_nodes = Vec::new();

        // Title
        if let Some(title) = &entry.title {
            child_nodes.push(make_node(
                SemanticRole::Heading { level: 3 },
                Some(title.content.clone()),
                "h3".to_string(),
                Vec::new(),
            ));
            stats.headings += 1;
        }

        // Link
        for link in &entry.links {
            let link_node = SemanticNode {
                role: SemanticRole::Link,
                name: Some(link.title.clone().unwrap_or_else(|| "Link".to_string())),
                tag: "a".to_string(),
                is_interactive: true,
                is_disabled: false,
                href: Some(link.href.clone()),
                action: Some("navigate".to_string()),
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
                children: Vec::new(),
            };
            child_nodes.push(link_node);
            stats.links += 1;
        }

        // Published date
        if let Some(published) = &entry.published {
            let date_str = published.to_rfc3339();
            child_nodes.push(make_node(
                SemanticRole::StaticText,
                Some(format!("Published: {}", date_str)),
                "p".to_string(),
                Vec::new(),
            ));
        }
        if let Some(updated) = &entry.updated {
            if entry.published.is_none() {
                let date_str = updated.to_rfc3339();
                child_nodes.push(make_node(
                    SemanticRole::StaticText,
                    Some(format!("Updated: {}", date_str)),
                    "p".to_string(),
                    Vec::new(),
                ));
            }
        }

        // Summary / description
        if let Some(summary) = &entry.summary {
            let content = summary.content.trim().to_string();
            if !content.is_empty() {
                child_nodes.push(make_node(
                    SemanticRole::StaticText,
                    Some(content),
                    "p".to_string(),
                    Vec::new(),
                ));
            }
        }

        // Content (full)
        if let Some(content) = &entry.content {
            if let Some(body) = &content.body {
                let trimmed = body.trim().to_string();
                if !trimmed.is_empty() {
                    child_nodes.push(make_node(
                        SemanticRole::StaticText,
                        Some(trimmed),
                        "p".to_string(),
                        Vec::new(),
                    ));
                }
            }
        }

        // Authors
        for person in &entry.authors {
            child_nodes.push(make_node(
                SemanticRole::StaticText,
                Some(format!("Author: {}", person.name)),
                "p".to_string(),
                Vec::new(),
            ));
        }

        // Categories as generic nodes
        for category in &entry.categories {
            child_nodes.push(make_node(
                SemanticRole::Generic,
                Some(category.term.clone()),
                "span".to_string(),
                Vec::new(),
            ));
        }

        if !child_nodes.is_empty() {
            stats.total_nodes += child_nodes.len();
            let item_node = make_node(
                SemanticRole::Article,
                entry.title.as_ref().map(|t| t.content.clone()),
                "article".to_string(),
                child_nodes,
            );
            stats.total_nodes += 1;
            item_nodes.push(item_node);
        }
    }

    if item_nodes.is_empty() {
        anyhow::bail!("Feed contains no entries");
    }

    let root = make_node(
        SemanticRole::List,
        feed_title.clone(),
        "feed".to_string(),
        item_nodes,
    );
    stats.total_nodes += 1;

    Ok((SemanticTree { root, stats }, feed_title))
}

fn make_node(
    role: SemanticRole,
    name: Option<String>,
    tag: String,
    children: Vec<SemanticNode>,
) -> SemanticNode {
    SemanticNode {
        role,
        name,
        tag,
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
        children,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_rss_by_content_type() {
        assert!(is_feed_content(b"", Some("application/rss+xml")));
        assert!(is_feed_content(b"", Some("application/atom+xml")));
        assert!(is_feed_content(
            b"",
            Some("application/rss+xml; charset=utf-8")
        ));
    }

    #[test]
    fn detect_rss_by_content() {
        let rss = r#"<?xml version="1.0"?><rss version="2.0"><channel></channel></rss>"#;
        assert!(is_feed_content(rss.as_bytes(), None));
    }

    #[test]
    fn detect_atom_by_content() {
        let atom = r#"<?xml version="1.0"?><feed xmlns="http://www.w3.org/2005/Atom"></feed>"#;
        assert!(is_feed_content(atom.as_bytes(), None));
    }

    #[test]
    fn not_feed_html() {
        let html = b"<html><body>Hello</body></html>";
        assert!(!is_feed_content(html, None));
    }

    #[test]
    fn not_feed_binary() {
        assert!(!is_feed_content(&[0x89, 0x50, 0x4e, 0x47], None));
    }

    #[test]
    fn parse_rss_feed() {
        let rss = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Test Feed</title>
            <item>
              <title>First Post</title>
              <link>https://example.com/1</link>
              <description>This is the first post description.</description>
              <pubDate>Mon, 01 Jan 2024 00:00:00 +0000</pubDate>
            </item>
            <item>
              <title>Second Post</title>
              <link>https://example.com/2</link>
              <description>This is the second post.</description>
              <pubDate>Tue, 02 Jan 2024 00:00:00 +0000</pubDate>
            </item>
          </channel>
        </rss>"#;

        let (tree, title) = extract_feed_tree(rss.as_bytes()).unwrap();
        assert_eq!(title, Some("Test Feed".to_string()));
        assert!(matches!(tree.root.role, SemanticRole::List));
        // Two articles (feed items)
        assert_eq!(tree.root.children.len(), 2);
        // First item should have heading, link, date, description
        let first = &tree.root.children[0];
        assert!(matches!(first.role, SemanticRole::Article));
        assert!(tree.stats.links >= 2);
    }

    #[test]
    fn parse_atom_feed() {
        let atom = r#"<?xml version="1.0" encoding="UTF-8"?>
        <feed xmlns="http://www.w3.org/2005/Atom">
          <title>Atom Feed</title>
          <entry>
            <title>An Entry</title>
            <link href="https://example.com/entry1"/>
            <updated>2024-01-01T00:00:00Z</updated>
            <summary>Summary text here.</summary>
          </entry>
        </feed>"#;

        let (tree, title) = extract_feed_tree(atom.as_bytes()).unwrap();
        assert_eq!(title, Some("Atom Feed".to_string()));
        assert_eq!(tree.root.children.len(), 1);
    }

    #[test]
    fn parse_empty_feed_errors() {
        let atom = r#"<?xml version="1.0" encoding="UTF-8"?>
        <feed xmlns="http://www.w3.org/2005/Atom">
          <title>Empty</title>
        </feed>"#;

        let result = extract_feed_tree(atom.as_bytes());
        assert!(result.is_err());
    }

    #[test]
    fn parse_invalid_xml_errors() {
        let result = extract_feed_tree(b"not xml at all");
        assert!(result.is_err());
    }

    #[test]
    fn feed_entries_have_links() {
        let rss = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Link Test</title>
            <item>
              <title>With Link</title>
              <link>https://example.com/a</link>
            </item>
          </channel>
        </rss>"#;

        let (tree, _) = extract_feed_tree(rss.as_bytes()).unwrap();
        let item = &tree.root.children[0];

        // Should contain a link node
        let has_link = item
            .children
            .iter()
            .any(|c| matches!(c.role, SemanticRole::Link));
        assert!(has_link);
        let link_node = item
            .children
            .iter()
            .find(|c| matches!(c.role, SemanticRole::Link))
            .unwrap();
        assert_eq!(link_node.href.as_deref(), Some("https://example.com/a"));
        assert!(link_node.is_interactive);
    }
}
