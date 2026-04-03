use crate::semantic::tree::{SemanticNode, SemanticRole, SemanticTree};

/// Format the semantic tree as a clean Markdown-style tree for AI agents.
pub fn format_md(tree: &SemanticTree) -> String {
    let mut output = String::new();

    // Root line
    output.push_str("document  [role: document]\n");

    // Format children with tree branches
    let visible: Vec<&SemanticNode> = tree
        .root
        .children
        .iter()
        .filter(|c| should_show(c))
        .collect();
    for (i, child) in visible.iter().enumerate() {
        let is_last = i == visible.len() - 1;
        format_node(child, "", is_last, &mut output);
    }

    output
}

fn format_node(node: &SemanticNode, prefix: &str, is_last: bool, output: &mut String) {
    let connector = if is_last { "└── " } else { "├── " };
    let desc = node_description(node);

    output.push_str(prefix);
    output.push_str(connector);
    output.push_str(&desc);
    output.push('\n');

    let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });

    let visible: Vec<&SemanticNode> = node.children.iter().filter(|c| should_show(c)).collect();
    for (i, child) in visible.iter().enumerate() {
        let is_last_child = i == visible.len() - 1;
        format_node(child, &child_prefix, is_last_child, output);
    }
}

/// Whether a node is worth showing in the tree.
fn should_show(node: &SemanticNode) -> bool {
    if node.role.is_landmark() || node.role.is_heading() || node.is_interactive {
        return true;
    }
    if matches!(
        node.role,
        SemanticRole::Image | SemanticRole::StaticText | SemanticRole::Article
    ) {
        return true;
    }
    if node.name.is_some() {
        return true;
    }
    // Show generic only if it carries visible children
    node.children.iter().any(|c| should_show(c))
}

fn node_description(node: &SemanticNode) -> String {
    // Helper to add element ID prefix for interactive elements
    let id_prefix = node
        .element_id
        .map(|id| format!("[#{}] ", id))
        .unwrap_or_default();

    match &node.role {
        SemanticRole::Heading { level } => {
            let name = node.name.as_deref().unwrap_or("");
            format!("{id_prefix}heading (h{level})  \"{name}\"")
        }
        SemanticRole::Link => {
            let name = node.name.as_deref().unwrap_or("");
            let mut s = format!("{id_prefix}link  \"{name}\"");
            if let Some(href) = &node.href {
                s.push_str(&format!("  → {href}"));
            }
            s
        }
        SemanticRole::Button => {
            let name = node.name.as_deref().unwrap_or("");
            let mut s = format!("{id_prefix}button  \"{name}\"");
            if node.is_disabled {
                s.push_str("  [disabled]");
            }
            s
        }
        SemanticRole::Image => {
            let name = node.name.as_deref().unwrap_or("image");
            format!("img  \"{name}\"")
        }
        SemanticRole::StaticText => {
            let name = node.name.as_deref().unwrap_or("");
            let display = if name.len() > 80 {
                format!("{}…", &name[..79])
            } else {
                name.to_string()
            };
            format!("text  \"{display}\"")
        }
        SemanticRole::TextBox => {
            let name = node.name.as_deref().unwrap_or("");
            let mut s = if name.is_empty() {
                format!("{id_prefix}textbox")
            } else {
                format!("{id_prefix}textbox  \"{name}\"")
            };
            if let Some(action) = &node.action {
                s.push_str(&format!("  [action: {action}]"));
            }
            s
        }
        SemanticRole::Combobox => {
            let name = node.name.as_deref().unwrap_or("");
            let mut s = if name.is_empty() {
                format!("{id_prefix}combobox")
            } else {
                format!("{id_prefix}combobox  \"{name}\"")
            };
            if let Some(action) = &node.action {
                s.push_str(&format!("  [action: {action}]"));
            }
            s
        }
        SemanticRole::Checkbox => {
            let name = node.name.as_deref().unwrap_or("");
            format!("{id_prefix}checkbox  \"{name}\"  [action: toggle]")
        }
        SemanticRole::Radio => {
            let name = node.name.as_deref().unwrap_or("");
            format!("{id_prefix}radio  \"{name}\"  [action: toggle]")
        }
        SemanticRole::List => {
            let name = node
                .name
                .as_deref()
                .map(|n| format!("  \"{n}\""))
                .unwrap_or_default();
            format!("list{name}")
        }
        SemanticRole::ListItem => {
            let name = node.name.as_deref().unwrap_or("");
            format!("item  \"{name}\"")
        }
        SemanticRole::Table => "table".to_string(),
        SemanticRole::Dialog => {
            let name = node.name.as_deref().unwrap_or("");
            format!("dialog  \"{name}\"")
        }
        SemanticRole::IFrame => {
            let name = node.name.as_deref().unwrap_or("iframe");
            let mut s = format!("iframe  \"{name}\"");
            if let Some(href) = &node.href {
                s.push_str(&format!("  → {href}"));
            }
            s
        }
        SemanticRole::Article => {
            let name = node.name.as_deref().unwrap_or("");
            let summary = compact_children(node);
            if summary.is_empty() {
                format!("article  \"{name}\"")
            } else {
                format!("article  \"{name}\"  → {summary}")
            }
        }
        SemanticRole::Form => {
            let name = node.name.as_deref().unwrap_or("");
            if name.is_empty() {
                "form".to_string()
            } else {
                format!("form  \"{name}\"")
            }
        }
        // Landmarks
        role if role.is_landmark() => {
            let role_str = role.role_str();
            let name = node
                .name
                .as_deref()
                .map(|n| format!("  \"{n}\""))
                .unwrap_or_default();
            format!("{role_str}{name}  [role: {role_str}]")
        }
        // Generic — compact if only visible children
        SemanticRole::Generic => {
            let name = node.name.as_deref().unwrap_or("");
            if name.is_empty() {
                format!("region  [role: region]")
            } else {
                format!("region  \"{name}\"  [role: region]")
            }
        }
        _ => {
            let role_str = node.role.to_string();
            let name = node.name.as_deref().unwrap_or("");
            if name.is_empty() {
                role_str
            } else {
                format!("{role_str}  \"{name}\"")
            }
        }
    }
}

/// Compact one-line summary of children for article nodes.
fn compact_children(node: &SemanticNode) -> String {
    let parts: Vec<String> = node
        .children
        .iter()
        .filter_map(|c| match &c.role {
            SemanticRole::StaticText => c.name.as_deref().map(|t| {
                if t.len() > 40 {
                    format!("{}…", &t[..39])
                } else {
                    t.to_string()
                }
            }),
            SemanticRole::Image => c.name.as_deref().map(|n| format!("img:{n}")),
            SemanticRole::Button => c.name.as_deref().map(|n| format!("button \"{n}\"")),
            SemanticRole::Link => {
                let name = c.name.as_deref().unwrap_or("");
                Some(format!("link \"{name}\""))
            }
            _ => None,
        })
        .collect();

    parts.join(" · ")
}
