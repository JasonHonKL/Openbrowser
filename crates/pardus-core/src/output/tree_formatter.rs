use crate::semantic::tree::{SemanticNode, SemanticRole, SemanticTree};

/// Format the semantic tree as a unicode tree for terminal output.
pub fn format_tree(tree: &SemanticTree) -> String {
    let mut output = String::new();
    format_node(&tree.root, "", true, &mut output);
    output
}

fn format_node(node: &SemanticNode, prefix: &str, is_last: bool, output: &mut String) {
    // Tree branch characters
    let connector = if is_last { "└── " } else { "├── " };

    // Build the node description
    let desc = node_description(node);

    if prefix.is_empty() {
        // Root node — no prefix/connector
        output.push_str(&desc);
    } else {
        output.push_str(prefix);
        output.push_str(connector);
        output.push_str(&desc);
    }
    output.push('\n');

    // Recurse into children
    let child_prefix = if prefix.is_empty() {
        "       ".to_string()
    } else {
        format!("{}{}", prefix, if is_last { "    " } else { "│   " })
    };

    for (i, child) in node.children.iter().enumerate() {
        let is_last_child = i == node.children.len() - 1;
        format_node(child, &child_prefix, is_last_child, output);
    }
}

fn node_description(node: &SemanticNode) -> String {
    let mut parts = Vec::new();

    // Role/name
    match &node.role {
        SemanticRole::Heading { level } => {
            parts.push(format!("heading (h{level})"));
            if let Some(name) = &node.name {
                parts.push(format!("\"{name}\""));
            }
        }
        SemanticRole::Link => {
            parts.push("link".to_string());
            if let Some(name) = &node.name {
                parts.push(format!("\"{name}\""));
            }
            if let Some(href) = &node.href {
                parts.push(format!("→ {href}"));
            }
        }
        SemanticRole::Button => {
            parts.push("button".to_string());
            if let Some(name) = &node.name {
                parts.push(format!("\"{name}\""));
            }
        }
        SemanticRole::Image => {
            parts.push("img".to_string());
            if let Some(name) = &node.name {
                parts.push(format!("\"{name}\""));
            }
        }
        SemanticRole::StaticText => {
            if let Some(name) = &node.name {
                parts.push(format!("text \"{name}\""));
            }
        }
        role if role.is_landmark() => {
            parts.push(role.role_str().to_string());
            if let Some(name) = &node.name {
                parts.push(format!("\"{name}\""));
            }
        }
        SemanticRole::IFrame => {
            parts.push("iframe".to_string());
            if let Some(name) = &node.name {
                parts.push(format!("\"{name}\""));
            }
            if let Some(href) = &node.href {
                parts.push(format!("→ {href}"));
            }
        }
        _ => {
            parts.push(node.role.to_string());
            if let Some(name) = &node.name {
                parts.push(format!("\"{name}\""));
            }
        }
    }

    // Additional annotations
    if node.is_interactive && !matches!(node.role, SemanticRole::Link | SemanticRole::Button) {
        parts.push("[interactive]".to_string());
    }
    if node.is_disabled {
        parts.push("[disabled]".to_string());
    }

    parts.join("  ")
}
