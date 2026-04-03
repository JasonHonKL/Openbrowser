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

    // Extract tables from positioned text
    let table_nodes = extract_tables(bytes);
    for table_node in &table_nodes {
        stats.total_nodes += count_nodes(table_node);
    }
    page_nodes.extend(table_nodes);

    // Extract form fields (AcroForm)
    let form_nodes = extract_form_fields(bytes);
    for form_node in &form_nodes {
        stats.total_nodes += count_nodes(form_node);
        stats.forms += 1;
    }
    page_nodes.extend(form_nodes);

    // Extract image metadata
    let image_nodes = extract_images(bytes);
    stats.images += image_nodes.len();
    for image_node in &image_nodes {
        stats.total_nodes += count_nodes(image_node);
    }
    page_nodes.extend(image_nodes);

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

fn count_nodes(node: &SemanticNode) -> usize {
    1 + node.children.iter().map(count_nodes).sum::<usize>()
}

// ---------------------------------------------------------------------------
// Table extraction from PDF text positions
// ---------------------------------------------------------------------------

/// Detect tabular data by analyzing per-page text with column alignment heuristics.
fn extract_tables(bytes: &[u8]) -> Vec<SemanticNode> {
    let doc = match lopdf::Document::load_mem(bytes) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let mut tables = Vec::new();
    let pages = doc.get_pages();

    for (_page_num, page_id) in &pages {
        if let Ok(content_data) = doc.get_page_content(*page_id) {
            if let Ok(text_ops) = parse_text_positions(&content_data) {
                if let Some(table) = build_table_from_positions(&text_ops) {
                    tables.push(table);
                }
            }
        }
    }

    tables
}

/// A positioned text fragment extracted from PDF content stream.
#[derive(Debug, Clone)]
struct TextFragment {
    x: f64,
    y: f64,
    text: String,
}

/// Parse text-showing operators from a content stream to get positioned fragments.
fn parse_text_positions(data: &[u8]) -> anyhow::Result<Vec<TextFragment>> {
    let content = lopdf::content::Content::decode(data)?;
    let mut fragments = Vec::new();
    let mut cur_x: f64 = 0.0;
    let mut cur_y: f64 = 0.0;
    let mut text_buffer = String::new();
    let mut text_start_x: f64 = 0.0;

    for operation in &content.operations {
        match operation.operator.as_str() {
            "Td" | "TD" => {
                flush_text(&mut text_buffer, &mut fragments, text_start_x, cur_y);
                if operation.operands.len() >= 2 {
                    if let Ok(tx) = obj_to_f64(&operation.operands[0]) {
                        if let Ok(ty) = obj_to_f64(&operation.operands[1]) {
                            cur_x += tx;
                            cur_y += ty;
                        }
                    }
                }
            }
            "Tm" => {
                flush_text(&mut text_buffer, &mut fragments, text_start_x, cur_y);
                if operation.operands.len() >= 6 {
                    if let Ok(x) = obj_to_f64(&operation.operands[4]) {
                        if let Ok(y) = obj_to_f64(&operation.operands[5]) {
                            cur_x = x;
                            cur_y = y;
                        }
                    }
                }
            }
            "Tj" => {
                if operation.operands.len() == 1 {
                    if text_buffer.is_empty() {
                        text_start_x = cur_x;
                    }
                    if let Ok(s) = operation.operands[0].as_str() {
                        text_buffer.push_str(&String::from_utf8_lossy(s));
                    }
                }
            }
            "TJ" => {
                if let Ok(arr) = operation.operands[0].as_array() {
                    if text_buffer.is_empty() {
                        text_start_x = cur_x;
                    }
                    for item in arr {
                        if let Ok(s) = item.as_str() {
                            text_buffer.push_str(&String::from_utf8_lossy(s));
                        } else if let Ok(kern) = item.as_float() {
                            // Kerning > 100 likely indicates a column gap
                            let kern_f64 = kern as f64;
                            if kern_f64.abs() > 100.0 && !text_buffer.is_empty() {
                                flush_text(&mut text_buffer, &mut fragments, text_start_x, cur_y);
                                text_start_x = cur_x - kern_f64;
                            }
                        }
                    }
                }
            }
            "ET" => {
                flush_text(&mut text_buffer, &mut fragments, text_start_x, cur_y);
            }
            _ => {}
        }
    }

    flush_text(&mut text_buffer, &mut fragments, text_start_x, cur_y);
    Ok(fragments)
}

/// Convert an lopdf Object to f64, handling both Integer and Real variants.
fn obj_to_f64(obj: &lopdf::Object) -> Result<f64, ()> {
    match obj {
        lopdf::Object::Integer(i) => Ok(*i as f64),
        lopdf::Object::Real(f) => Ok(*f as f64),
        _ => Err(()),
    }
}

fn flush_text(buf: &mut String, frags: &mut Vec<TextFragment>, x: f64, y: f64) {
    let trimmed = buf.trim().to_string();
    if !trimmed.is_empty() {
        frags.push(TextFragment {
            x,
            y,
            text: trimmed,
        });
    }
    buf.clear();
}

/// Group text fragments into rows and detect tabular alignment.
fn build_table_from_positions(fragments: &[TextFragment]) -> Option<SemanticNode> {
    if fragments.len() < 4 {
        return None;
    }

    // Group by y-coordinate (rows) with tolerance
    let mut rows: Vec<Vec<&TextFragment>> = Vec::new();
    let y_tolerance = 2.0;

    for frag in fragments {
        let mut found_row = false;
        for row in &mut rows {
            if let Some(first) = row.first() {
                if (first.y - frag.y).abs() < y_tolerance {
                    row.push(frag);
                    found_row = true;
                    break;
                }
            }
        }
        if !found_row {
            rows.push(vec![frag]);
        }
    }

    // Sort each row by x position
    for row in &mut rows {
        row.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    }

    // Filter out single-cell rows (not tabular)
    rows.retain(|row| row.len() >= 2);

    if rows.len() < 2 {
        return None;
    }

    // Check that rows have consistent column counts (at least 60% consistency)
    let col_counts: Vec<usize> = rows.iter().map(|r| r.len()).collect();
    let max_cols = *col_counts.iter().max().unwrap_or(&0);
    if max_cols < 2 {
        return None;
    }
    let most_common = mode(&col_counts).unwrap_or(2);
    let consistent_rows = col_counts.iter().filter(|&&c| c == most_common).count();
    if (consistent_rows as f64) / (col_counts.len() as f64) < 0.6 {
        return None;
    }

    // Build table node
    let mut row_nodes = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let mut cell_nodes = Vec::new();
        for cell in row {
            let cell_node = make_node(
                if i == 0 {
                    SemanticRole::ColumnHeader
                } else {
                    SemanticRole::Cell
                },
                Some(cell.text.clone()),
                if i == 0 {
                    "th".to_string()
                } else {
                    "td".to_string()
                },
                Vec::new(),
            );
            cell_nodes.push(cell_node);
        }
        let row_node = make_node(SemanticRole::Row, None, "tr".to_string(), cell_nodes);
        row_nodes.push(row_node);
    }

    Some(make_node(
        SemanticRole::Table,
        None,
        "table".to_string(),
        row_nodes,
    ))
}

fn mode(vals: &[usize]) -> Option<usize> {
    use std::collections::HashMap;
    let mut counts: HashMap<usize, usize> = HashMap::new();
    for &v in vals {
        *counts.entry(v).or_insert(0) += 1;
    }
    counts.into_iter().max_by_key(|&(_, c)| c).map(|(v, _)| v)
}

// ---------------------------------------------------------------------------
// Form field extraction (AcroForm)
// ---------------------------------------------------------------------------

fn extract_form_fields(bytes: &[u8]) -> Vec<SemanticNode> {
    let doc = match lopdf::Document::load_mem(bytes) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let catalog = match doc.catalog() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let acro_form_obj = match catalog.get(b"AcroForm") {
        Ok(obj) => obj,
        Err(_) => return Vec::new(),
    };

    // Get the AcroForm dictionary — it might be a direct dict or a reference
    let form_dict = match acro_form_obj {
        lopdf::Object::Reference(id) => match doc.get_dictionary(*id) {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        },
        lopdf::Object::Dictionary(_) => match acro_form_obj.as_dict() {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        },
        _ => return Vec::new(),
    };

    let fields_obj = match form_dict.get(b"Fields") {
        Ok(obj) => obj,
        Err(_) => return Vec::new(),
    };

    let field_ids: Vec<lopdf::ObjectId> = match fields_obj {
        lopdf::Object::Array(arr) => arr.iter().filter_map(|o| o.as_reference().ok()).collect(),
        lopdf::Object::Reference(id) => match doc.get_object(*id).and_then(|o| o.as_array()) {
            Ok(a) => a.iter().filter_map(|o| o.as_reference().ok()).collect(),
            Err(_) => return Vec::new(),
        },
        _ => return Vec::new(),
    };

    if field_ids.is_empty() {
        return Vec::new();
    }

    let mut field_nodes = Vec::new();
    let mut next_id = 1usize;

    for field_id in &field_ids {
        if let Some(node) = extract_field_node(&doc, *field_id, &mut next_id) {
            field_nodes.push(node);
        }
    }

    if field_nodes.is_empty() {
        return Vec::new();
    }

    vec![make_node(
        SemanticRole::Form,
        Some("PDF Form Fields".to_string()),
        "form".to_string(),
        field_nodes,
    )]
}

fn extract_field_node(
    doc: &lopdf::Document,
    field_id: lopdf::ObjectId,
    next_id: &mut usize,
) -> Option<SemanticNode> {
    let dict = doc.get_dictionary(field_id).ok()?;

    // Get field type
    let ft_obj = dict.get(b"FT").ok()?;
    let ft_name_bytes = match ft_obj {
        lopdf::Object::Reference(id) => {
            let obj = doc.get_object(*id).ok()?;
            obj.as_name().ok()?
        }
        _ => ft_obj.as_name().ok()?,
    };
    let ft_name = String::from_utf8_lossy(ft_name_bytes).to_string();

    let (role, input_type, tag) = match ft_name.as_str() {
        "Tx" => (
            SemanticRole::TextBox,
            Some("text".to_string()),
            "input".to_string(),
        ),
        "Btn" => (
            SemanticRole::Checkbox,
            Some("checkbox".to_string()),
            "input".to_string(),
        ),
        "Ch" => (
            SemanticRole::Combobox,
            Some("select".to_string()),
            "select".to_string(),
        ),
        "Sig" => (
            SemanticRole::Button,
            Some("signature".to_string()),
            "input".to_string(),
        ),
        _ => (SemanticRole::TextBox, None, "input".to_string()),
    };

    // Get field name (T)
    let name = dict
        .get(b"T")
        .ok()
        .and_then(|o| o.as_str().ok())
        .map(|b| String::from_utf8_lossy(b).to_string())
        .or_else(|| {
            dict.get(b"TU")
                .ok()
                .and_then(|o| o.as_str().ok())
                .map(|b| String::from_utf8_lossy(b).to_string())
        });

    // Get default value (V)
    let value = dict
        .get(b"V")
        .ok()
        .and_then(|o| o.as_str().ok())
        .map(|b| String::from_utf8_lossy(b).to_string())
        .or_else(|| {
            // Value might be a name
            dict.get(b"V")
                .ok()
                .and_then(|o| o.as_name().ok())
                .map(|b| String::from_utf8_lossy(b).to_string())
        });

    // Build display name: "FieldName: Value" or just "FieldName"
    let display_name = match (&name, &value) {
        (Some(n), Some(v)) => Some(format!("{}: {}", n, v)),
        (Some(n), None) => Some(n.clone()),
        (None, Some(v)) => Some(v.clone()),
        _ => None,
    };

    let element_id = *next_id;
    *next_id += 1;

    // Check for kids (radio buttons, choice lists)
    if let Ok(kids) = dict.get(b"Kids") {
        if let Ok(arr) = kids.as_array() {
            let mut child_nodes = Vec::new();
            for kid in arr {
                if let Ok(kid_id) = kid.as_reference() {
                    if let Some(child) = extract_field_node(doc, kid_id, next_id) {
                        child_nodes.push(child);
                    }
                }
            }
            if !child_nodes.is_empty() {
                return Some(SemanticNode {
                    role,
                    name: display_name,
                    tag,
                    is_interactive: true,
                    is_disabled: false,
                    href: None,
                    action: Some("fill".to_string()),
                    element_id: Some(element_id),
                    selector: None,
                    input_type,
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
                    children: child_nodes,
                });
            }
        }
    }

    Some(SemanticNode {
        role,
        name: display_name,
        tag,
        is_interactive: true,
        is_disabled: false,
        href: None,
        action: Some("fill".to_string()),
        element_id: Some(element_id),
        selector: None,
        input_type,
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
    })
}

// ---------------------------------------------------------------------------
// Image extraction (metadata only — dimensions and format)
// ---------------------------------------------------------------------------

fn extract_images(bytes: &[u8]) -> Vec<SemanticNode> {
    let doc = match lopdf::Document::load_mem(bytes) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let mut images = Vec::new();
    let pages = doc.get_pages();

    for (_page_num, page_id) in &pages {
        if let Ok(page_images) = doc.get_page_images(*page_id) {
            for (idx, img) in page_images.iter().enumerate() {
                let mut label_parts = Vec::new();
                label_parts.push(format!("Image {}", images.len() + idx + 1));
                label_parts.push(format!("{}x{}", img.width, img.height));

                let format_hint = img
                    .filters
                    .as_ref()
                    .and_then(|f| f.last())
                    .map(|s| s.as_str())
                    .unwrap_or("Raw");
                let format_name = match format_hint {
                    "DCTDecode" => "JPEG",
                    "JPXDecode" => "JPEG2000",
                    "CCITTFaxDecode" => "CCITT (TIFF)",
                    "JBIG2Decode" => "JBIG2",
                    "FlateDecode" => "Lossless",
                    "LZWDecode" => "LZW",
                    _ => format_hint,
                };
                label_parts.push(format_name.to_string());

                if let Some(cs) = &img.color_space {
                    label_parts.push(cs.clone());
                }

                let name = Some(label_parts.join(" — "));
                images.push(make_node(
                    SemanticRole::Image,
                    name,
                    "img".to_string(),
                    Vec::new(),
                ));
            }
        }
    }

    images
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

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

    #[test]
    fn extract_images_from_non_pdf() {
        let images = extract_images(b"not a pdf");
        assert!(images.is_empty());
    }

    #[test]
    fn extract_form_fields_from_non_pdf() {
        let forms = extract_form_fields(b"not a pdf");
        assert!(forms.is_empty());
    }

    #[test]
    fn extract_tables_from_non_pdf() {
        let tables = extract_tables(b"not a pdf");
        assert!(tables.is_empty());
    }

    #[test]
    fn extract_images_from_test_pdf() {
        let pdf_bytes = make_test_pdf();
        let images = extract_images(&pdf_bytes);
        // Our test PDF has no image XObjects
        assert!(images.is_empty());
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
