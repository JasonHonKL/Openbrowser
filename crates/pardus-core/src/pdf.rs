use crate::semantic::tree::{SemanticNode, SemanticRole, SemanticTree, TreeStats};

const HEADING_MAX_LEN: usize = 80;
const SHORT_HEADING_MAX_LEN: usize = 60;

pub fn extract_pdf_tree(bytes: &[u8]) -> anyhow::Result<(SemanticTree, Option<String>)> {
    let pages = pdf_extract::extract_text_from_mem_by_pages(bytes)
        .map_err(|e| anyhow::anyhow!("Failed to extract PDF text: {}", e))?;

    if pages.is_empty() {
        anyhow::bail!("PDF contains no extractable text");
    }

    let mut stats = TreeStats::default();
    let mut page_nodes = Vec::new();
    let mut first_heading_seen = false;
    let mut title: Option<String> = None;

    for (page_idx, page_text) in pages.iter().enumerate() {
        let blocks = split_into_blocks(page_text);

        if blocks.is_empty() {
            continue;
        }

        let mut block_nodes = Vec::new();

        for block in &blocks {
            let trimmed = block.trim();
            if trimmed.is_empty() {
                continue;
            }

            let is_first_candidate = !first_heading_seen;
            let (node, is_heading) = classify_block(trimmed, is_first_candidate, &mut title);

            if is_heading && !first_heading_seen {
                first_heading_seen = true;
            }

            if is_heading {
                stats.headings += 1;
            }
            stats.total_nodes += 1;
            block_nodes.push(node);
        }

        if block_nodes.is_empty() {
            continue;
        }

        let page_name = if pages.len() > 1 {
            Some(format!("Page {}", page_idx + 1))
        } else {
            None
        };

        let page_node = make_node(
            SemanticRole::Region,
            page_name,
            "section".to_string(),
            block_nodes,
        );

        stats.total_nodes += 1;
        page_nodes.push(page_node);
    }

    if page_nodes.is_empty() {
        anyhow::bail!("PDF contains no extractable text content");
    }

    let root = make_node(
        SemanticRole::Document,
        title.clone(),
        "document".to_string(),
        page_nodes,
    );

    stats.total_nodes += 1;

    Ok((SemanticTree { root, stats }, title))
}

fn split_into_blocks(text: &str) -> Vec<String> {
    text.split("\n\n")
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .collect()
}

fn classify_block(
    text: &str,
    is_first_candidate: bool,
    title: &mut Option<String>,
) -> (SemanticNode, bool) {
    if is_heading_candidate(text, is_first_candidate) {
        if title.is_none() {
            *title = Some(text.to_string());
        }

        let level = if title.as_deref() == Some(text) { 1 } else { 2 };

        return (make_heading_node(text, level), true);
    }

    (make_text_node(text), false)
}

fn is_heading_candidate(text: &str, is_first: bool) -> bool {
    let trimmed = text.trim();

    if trimmed.is_empty() || trimmed.contains('\n') {
        return false;
    }

    let len = trimmed.len();

    if is_first && len >= 3 {
        return true;
    }

    if len > HEADING_MAX_LEN {
        return false;
    }

    if len > 2 && trimmed == trimmed.to_uppercase() && trimmed.chars().any(|c| c.is_alphabetic()) {
        return true;
    }

    if len <= SHORT_HEADING_MAX_LEN
        && !trimmed.ends_with('.')
        && !trimmed.ends_with('?')
        && !trimmed.ends_with('!')
    {
        if len <= 3 && trimmed.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return false;
        }
        return true;
    }

    false
}

fn make_text_node(text: &str) -> SemanticNode {
    make_node(
        SemanticRole::StaticText,
        Some(text.to_string()),
        "p".to_string(),
        Vec::new(),
    )
}

fn make_heading_node(text: &str, level: u8) -> SemanticNode {
    make_node(
        SemanticRole::Heading { level },
        Some(text.to_string()),
        format!("h{}", level),
        Vec::new(),
    )
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
        children,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_into_blocks_double_newline() {
        let text = "First block\n\nSecond block\n\nThird block";
        let blocks = split_into_blocks(text);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0], "First block");
        assert_eq!(blocks[1], "Second block");
        assert_eq!(blocks[2], "Third block");
    }

    #[test]
    fn split_into_blocks_trims_empty() {
        let text = "Hello\n\n\n\nWorld\n\n   \n\nEnd";
        let blocks = split_into_blocks(text);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0], "Hello");
        assert_eq!(blocks[1], "World");
        assert_eq!(blocks[2], "End");
    }

    #[test]
    fn split_into_blocks_empty_input() {
        let blocks = split_into_blocks("");
        assert!(blocks.is_empty());
    }

    #[test]
    fn split_into_blocks_no_double_newlines() {
        let text = "Line one\nLine two\nLine three";
        let blocks = split_into_blocks(text);
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn is_heading_first_block() {
        assert!(is_heading_candidate("My Document Title", true));
        assert!(is_heading_candidate("A", true));
        assert!(!is_heading_candidate("", true));
    }

    #[test]
    fn is_heading_all_caps() {
        assert!(is_heading_candidate("CHAPTER 1", false));
        assert!(is_heading_candidate("ABSTRACT", false));
        assert!(is_heading_candidate("INTRODUCTION", false));
        assert!(!is_heading_candidate("123.", false));
        assert!(!is_heading_candidate("123", false));
    }

    #[test]
    fn is_heading_short_text() {
        assert!(is_heading_candidate("Introduction", false));
        assert!(is_heading_candidate("Summary", false));
        assert!(is_heading_candidate("Background", false));
        assert!(!is_heading_candidate("This is a normal sentence.", false));
        assert!(!is_heading_candidate("What is this?", false));
    }

    #[test]
    fn is_heading_too_long() {
        let long_text = "This is a very long sentence that exceeds the maximum heading length and should not be classified as a heading by any heuristic";
        assert!(!is_heading_candidate(long_text, false));
    }

    #[test]
    fn is_heading_multiline_rejected() {
        assert!(!is_heading_candidate("Line one\nLine two", true));
    }

    #[test]
    fn is_heading_trailing_punctuation() {
        assert!(!is_heading_candidate("This ends with a period.", false));
        assert!(!is_heading_candidate("Is this a question?", false));
        assert!(!is_heading_candidate("Watch out!", false));
    }

    #[test]
    fn classify_block_first_is_heading() {
        let mut title = None;
        let (node, is_heading) = classify_block("My Report", true, &mut title);

        assert!(is_heading);
        assert_eq!(title, Some("My Report".to_string()));
        assert!(matches!(node.role, SemanticRole::Heading { level: 1 }));
        assert_eq!(node.tag, "h1");
    }

    #[test]
    fn classify_block_second_is_h2() {
        let mut title = Some("First Title".to_string());
        let (node, is_heading) = classify_block("Introduction", false, &mut title);

        assert!(is_heading);
        assert_eq!(title, Some("First Title".to_string()));
        assert!(matches!(node.role, SemanticRole::Heading { level: 2 }));
        assert_eq!(node.tag, "h2");
    }

    #[test]
    fn classify_block_regular_text() {
        let mut title = None;
        let (node, is_heading) = classify_block(
            "This is a regular paragraph of text that goes on for a while.",
            false,
            &mut title,
        );

        assert!(!is_heading);
        assert_eq!(node.role, SemanticRole::StaticText);
        assert_eq!(node.tag, "p");
    }

    #[test]
    fn extract_pdf_tree_basic() {
        let pdf_bytes = make_test_pdf();
        let (tree, title) = extract_pdf_tree(&pdf_bytes).unwrap();

        assert_eq!(title, Some("Test Document Title".to_string()));
        assert!(matches!(tree.root.role, SemanticRole::Document));
        assert!(tree.stats.headings > 0);
        assert!(tree.stats.total_nodes > 0);
    }

    #[test]
    fn extract_pdf_tree_empty_bytes() {
        let result = extract_pdf_tree(b"");
        assert!(result.is_err());
    }

    #[test]
    fn extract_pdf_tree_invalid_bytes() {
        let result = extract_pdf_tree(b"not a pdf");
        assert!(result.is_err());
    }

    fn make_test_pdf() -> Vec<u8> {
        let mut objects = Vec::new();
        let mut offsets = Vec::new();

        let catalog = b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj";
        offsets.push(9);
        objects.push(catalog.as_slice());

        let pages = b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj";
        offsets.push(offsets.last().unwrap() + catalog.len() as usize + 1);
        objects.push(pages.as_slice());

        let content_stream = b"BT\n/F1 24 Tf\n100 700 Td\n(Test Document Title) Tj\n/F1 18 Tf\n0 -40 Td\n(Chapter 1 Introduction) Tj\n/F1 12 Tf\n0 -30 Td\n(This is the first paragraph of the test document.) Tj\n0 -20 Td\n(It contains some sample text for testing purposes.) Tj\n0 -40 Td\n(Chapter 2 Methods) Tj\n0 -30 Td\n(We used the following methods in our research.) Tj\n0 -20 Td\n(Data was collected from various sources.) Tj\nET";
        let content_len = content_stream.len();

        let page = format!(
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792]\n   /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj"
        );
        offsets.push(offsets.last().unwrap() + pages.len() as usize + 1);
        objects.push(page.as_bytes());

        let content_obj = format!(
            "4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj",
            content_len,
            String::from_utf8_lossy(content_stream)
        );
        offsets.push(offsets.last().unwrap() + page.len() + 1);
        objects.push(content_obj.as_bytes());

        let font = b"5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj";
        offsets.push(offsets.last().unwrap() + content_obj.len() + 1);
        objects.push(font.as_slice());

        let mut pdf = Vec::new();
        pdf.extend_from_slice(b"%PDF-1.4\n");

        for obj in &objects {
            pdf.extend_from_slice(obj);
            pdf.push(b'\n');
        }

        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n");
        pdf.extend_from_slice(format!("0 {}\n", objects.len() + 1).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        for &off in &offsets {
            pdf.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
        }

        pdf.extend_from_slice(b"trailer\n");
        pdf.extend_from_slice(
            format!("<< /Size {} /Root 1 0 R >>\n", objects.len() + 1).as_bytes(),
        );
        pdf.extend_from_slice(b"startxref\n");
        pdf.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
        pdf.extend_from_slice(b"%%EOF");

        pdf
    }
}
