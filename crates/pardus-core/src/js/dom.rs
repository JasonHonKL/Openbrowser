use scraper::{ElementRef, Html, Selector};
use std::collections::{HashMap, HashSet};

/// Unique ID for a DOM node.
pub type NodeId = u32;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MutationRecord {
    pub type_: String,
    pub target: u32,
    pub added_nodes: Vec<u32>,
    pub removed_nodes: Vec<u32>,
    pub attribute_name: Option<String>,
    pub old_value: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ObserverEntry {
    pub id: u32,
    pub target_node_id: u32,
    pub options: MutationObserverInit,
}

#[derive(Debug, Clone)]
pub struct MutationObserverInit {
    pub child_list: bool,
    pub attributes: bool,
    pub character_data: bool,
    pub subtree: bool,
    pub attribute_old_value: bool,
    pub character_data_old_value: bool,
    pub attribute_filter: Vec<String>,
}

impl Default for MutationObserverInit {
    fn default() -> Self {
        Self {
            child_list: false,
            attributes: true,
            character_data: false,
            subtree: false,
            attribute_old_value: false,
            character_data_old_value: false,
            attribute_filter: Vec::new(),
        }
    }
}

/// A minimal DOM document backed by a flat HashMap.
#[derive(Debug)]
pub struct DomDocument {
    nodes: HashMap<NodeId, DomNode>,
    next_id: NodeId,
    document_element_id: NodeId,
    head_id: NodeId,
    body_id: NodeId,
    id_index: HashMap<String, NodeId>,
    tag_index: HashMap<String, Vec<NodeId>>,
    class_index: HashMap<String, Vec<NodeId>>,
    /// Reverse index: node_id -> set of classes on that node.
    /// Enables O(classes_on_node) class removal instead of O(all_classes_in_doc).
    node_class_index: HashMap<NodeId, HashSet<String>>,
    title: Option<String>,
    #[allow(dead_code)]
    original_html: Option<String>,
    mutation_records: Vec<MutationRecord>,
    pending_mutations: HashMap<u32, Vec<MutationRecord>>,
    observers: Vec<ObserverEntry>,
    next_observer_id: u32,
    /// Maximum number of nodes allowed. None = unlimited.
    max_nodes: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DomNodeType {
    Element,
    Text,
    Document,
    DocumentFragment,
    Comment,
    ShadowRoot,
}

/// Mode of a shadow root (open = pierceable, closed = hidden).
#[derive(Debug, Clone, PartialEq)]
pub enum ShadowRootMode {
    Open,
    Closed,
}

/// A shadow root attached to a host element.
#[derive(Debug, Clone)]
pub struct ShadowRoot {
    pub mode: ShadowRootMode,
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone)]
pub struct DomNode {
    pub id: NodeId,
    pub node_type: DomNodeType,
    pub tag_name: Option<String>,
    pub attributes: HashMap<String, String>,
    pub children: Vec<NodeId>,
    pub parent_id: Option<NodeId>,
    pub text_content: Option<String>,
    /// Shadow root attached to this element (if this element is a shadow host).
    pub shadow_root: Option<ShadowRoot>,
}

impl DomDocument {
    /// Build a DomDocument from an HTML string.
    pub fn from_html(html: &str) -> Self {
        let parsed = Html::parse_document(html);
        let mut doc = Self {
            nodes: HashMap::new(),
            next_id: 1,
            document_element_id: 0,
            head_id: 0,
            body_id: 0,
            id_index: HashMap::new(),
            tag_index: HashMap::new(),
            class_index: HashMap::new(),
            node_class_index: HashMap::new(),
            title: None,
            original_html: Some(html.to_string()),
            mutation_records: Vec::new(),
            pending_mutations: HashMap::new(),
            observers: Vec::new(),
            next_observer_id: 1,
            max_nodes: None,
        };

        // Create document root
        let _doc_id = doc.alloc_node(DomNodeType::Document, None);

        // Walk the parsed tree
        if let Some(html_el) = parsed.select(&Selector::parse("html").unwrap()).next() {
            let html_id = doc.build_from_scraper(&html_el, None);
            doc.document_element_id = html_id;

            // Find head and body among html's direct children
            for &child_id in &doc.nodes.get(&html_id).unwrap().children.clone() {
                if let Some(node) = doc.nodes.get(&child_id) {
                    match node.tag_name.as_deref() {
                        Some("head") => doc.head_id = child_id,
                        Some("body") => doc.body_id = child_id,
                        _ => {}
                    }
                }
            }

            // Extract title from head > title
            if doc.head_id != 0 {
                if let Some(head_node) = doc.nodes.get(&doc.head_id) {
                    for &child_id in &head_node.children {
                        if let Some(child) = doc.nodes.get(&child_id) {
                            if child.tag_name.as_deref() == Some("title") {
                                doc.title = Some(doc.get_text_content(child_id));
                                break;
                            }
                        }
                    }
                }
            }
        } else if let Some(body_el) = parsed.select(&Selector::parse("body").unwrap()).next() {
            let body_id = doc.build_from_scraper(&body_el, None);
            doc.body_id = body_id;
            doc.document_element_id = body_id;
        }

        // Build indexes for all existing element nodes
        doc.rebuild_indexes_for_subtree(doc.document_element_id);

        // Free the raw HTML — DOM is fully built
        doc.original_html = None;

        doc
    }

    fn alloc_node(&mut self, node_type: DomNodeType, parent_id: Option<NodeId>) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.insert(
            id,
            DomNode {
                id,
                node_type,
                tag_name: None,
                attributes: HashMap::new(),
                children: Vec::new(),
                parent_id,
                text_content: None,
                shadow_root: None,
            },
        );
        id
    }

    fn alloc_element(&mut self, tag: &str, parent_id: Option<NodeId>) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.insert(
            id,
            DomNode {
                id,
                node_type: DomNodeType::Element,
                tag_name: Some(tag.to_lowercase()),
                attributes: HashMap::new(),
                children: Vec::new(),
                parent_id,
                text_content: None,
                shadow_root: None,
            },
        );
        id
    }

    fn alloc_text(&mut self, text: &str, parent_id: Option<NodeId>) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.insert(
            id,
            DomNode {
                id,
                node_type: DomNodeType::Text,
                tag_name: None,
                attributes: HashMap::new(),
                children: Vec::new(),
                parent_id,
                text_content: Some(text.to_string()),
                shadow_root: None,
            },
        );
        id
    }

    fn alloc_comment(&mut self, text: &str, parent_id: Option<NodeId>) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.insert(
            id,
            DomNode {
                id,
                node_type: DomNodeType::Comment,
                tag_name: None,
                attributes: HashMap::new(),
                children: Vec::new(),
                parent_id,
                text_content: Some(text.to_string()),
                shadow_root: None,
            },
        );
        id
    }

    /// Recursively build DOM from a scraper element.
    fn build_from_scraper(&mut self, el: &ElementRef, parent_id: Option<NodeId>) -> NodeId {
        let tag = el.value().name().to_lowercase();
        let id = self.alloc_element(&tag, parent_id);

        // Copy attributes
        for (k, v) in el.value().attrs() {
            self.nodes
                .get_mut(&id)
                .unwrap()
                .attributes
                .insert(k.to_string(), v.to_string());
        }

        // Walk children — skip script/style subtrees
        for child_node in el.children() {
            if let Some(child_el) = ElementRef::wrap(child_node) {
                let child_tag = child_el.value().name().to_lowercase();
                if matches!(child_tag.as_str(), "script" | "style") {
                    continue;
                }
                let child_id = self.build_from_scraper(&child_el, Some(id));
                self.nodes.get_mut(&id).unwrap().children.push(child_id);
            } else if let Some(text) = child_node.value().as_text() {
                if !text.text.is_empty() {
                    let text_id = self.alloc_text(&text.text, Some(id));
                    self.nodes.get_mut(&id).unwrap().children.push(text_id);
                }
            } else if let Some(comment) = child_node.value().as_comment() {
                let comment_id = self.alloc_comment(&comment.comment, Some(id));
                self.nodes.get_mut(&id).unwrap().children.push(comment_id);
            }
        }

        id
    }

    // ---- Index management ----

    fn add_to_indexes(&mut self, node_id: NodeId) {
        let node = match self.nodes.get(&node_id) {
            Some(n) => n.clone(),
            None => return,
        };

        if node.node_type != DomNodeType::Element {
            return;
        }

        if let Some(id_val) = node.attributes.get("id") {
            if !id_val.is_empty() {
                self.id_index.insert(id_val.clone(), node_id);
            }
        }

        if let Some(tag) = &node.tag_name {
            self.tag_index.entry(tag.clone()).or_default().push(node_id);
        }

        if let Some(class_name) = node.attributes.get("class") {
            let classes: HashSet<String> = class_name
                .split_whitespace()
                .map(|c| c.to_string())
                .collect();
            for class in &classes {
                self.class_index
                    .entry(class.clone())
                    .or_default()
                    .push(node_id);
            }
            self.node_class_index.insert(node_id, classes);
        }
    }

    fn remove_from_indexes(&mut self, node_id: NodeId) {
        let node = match self.nodes.get(&node_id) {
            Some(n) => n.clone(),
            None => return,
        };

        if node.node_type != DomNodeType::Element {
            return;
        }

        if let Some(id_val) = node.attributes.get("id") {
            if self.id_index.get(id_val) == Some(&node_id) {
                self.id_index.remove(id_val);
            }
        }

        if let Some(tag) = &node.tag_name {
            if let Some(vec) = self.tag_index.get_mut(tag) {
                vec.retain(|&id| id != node_id);
                if vec.is_empty() {
                    self.tag_index.remove(tag);
                }
            }
        }

        // Use reverse index for O(classes_on_node) removal
        if let Some(classes) = self.node_class_index.remove(&node_id) {
            for class in classes {
                if let Some(vec) = self.class_index.get_mut(&class) {
                    vec.retain(|&id| id != node_id);
                    if vec.is_empty() {
                        self.class_index.remove(&class);
                    }
                }
            }
        }
    }

    fn rebuild_class_index_for_node(&mut self, node_id: NodeId) {
        // Use reverse index for O(classes_on_node) removal
        if let Some(old_classes) = self.node_class_index.remove(&node_id) {
            for class in old_classes {
                if let Some(vec) = self.class_index.get_mut(&class) {
                    vec.retain(|&id| id != node_id);
                    if vec.is_empty() {
                        self.class_index.remove(&class);
                    }
                }
            }
        }

        if let Some(node) = self.nodes.get(&node_id) {
            if let Some(class_name) = node.attributes.get("class") {
                let classes: HashSet<String> = class_name
                    .split_whitespace()
                    .map(|c| c.to_string())
                    .collect();
                for class in &classes {
                    self.class_index
                        .entry(class.clone())
                        .or_default()
                        .push(node_id);
                }
                self.node_class_index.insert(node_id, classes);
            }
        }
    }

    fn rebuild_indexes_for_subtree(&mut self, node_id: NodeId) {
        self.add_to_indexes(node_id);
        if let Some(node) = self.nodes.get(&node_id) {
            let children: Vec<NodeId> = node.children.clone();
            for child_id in children {
                self.rebuild_indexes_for_subtree(child_id);
            }
        }
    }

    // ---- Serialization ----

    pub fn to_html(&self) -> String {
        let mut output = String::new();
        if self.document_element_id != 0 {
            self.serialize_node(self.document_element_id, &mut output);
        }
        output
    }

    fn serialize_node(&self, id: NodeId, output: &mut String) {
        let node = match self.nodes.get(&id) {
            Some(n) => n,
            None => return,
        };

        match node.node_type {
            DomNodeType::Text => {
                if let Some(text) = &node.text_content {
                    output.push_str(text);
                }
            }
            DomNodeType::Comment => {
                if let Some(text) = &node.text_content {
                    output.push_str("<!--");
                    output.push_str(text);
                    output.push_str("-->");
                }
            }
            DomNodeType::Element => {
                let tag = node.tag_name.as_deref().unwrap_or("div");
                let void = matches!(
                    tag,
                    "area"
                        | "base"
                        | "br"
                        | "col"
                        | "embed"
                        | "hr"
                        | "img"
                        | "input"
                        | "link"
                        | "meta"
                        | "param"
                        | "source"
                        | "track"
                        | "wbr"
                );

                output.push('<');
                output.push_str(tag);
                for (k, v) in &node.attributes {
                    output.push(' ');
                    output.push_str(k);
                    output.push_str("=\"");
                    output.push_str(&v.replace('&', "&amp;").replace('"', "&quot;"));
                    output.push('"');
                }
                output.push('>');

                if !void {
                    for &child_id in &node.children {
                        self.serialize_node(child_id, output);
                    }
                    output.push_str("</");
                    output.push_str(tag);
                    output.push('>');
                }
            }
            DomNodeType::Document | DomNodeType::DocumentFragment | DomNodeType::ShadowRoot => {
                for &child_id in &node.children {
                    self.serialize_node(child_id, output);
                }
            }
        }
    }

    // ---- DOM manipulation ----

    pub fn create_element(&mut self, tag: &str) -> NodeId {
        if !self.can_alloc() {
            return 0;
        }
        self.alloc_element(tag, None)
    }

    pub fn create_text_node(&mut self, text: &str) -> NodeId {
        if !self.can_alloc() {
            return 0;
        }
        self.alloc_text(text, None)
    }

    pub fn create_document_fragment(&mut self) -> NodeId {
        if !self.can_alloc() {
            return 0;
        }
        self.alloc_node(DomNodeType::DocumentFragment, None)
    }

    pub fn append_child(&mut self, parent_id: NodeId, child_id: NodeId) {
        if let Some(old_parent) = self.nodes.get(&child_id).and_then(|n| n.parent_id) {
            if let Some(old) = self.nodes.get_mut(&old_parent) {
                old.children.retain(|&id| id != child_id);
            }
        }
        if let Some(child) = self.nodes.get_mut(&child_id) {
            child.parent_id = Some(parent_id);
        }
        if let Some(parent) = self.nodes.get_mut(&parent_id) {
            parent.children.push(child_id);
        }
        self.queue_mutation("childList", parent_id, vec![child_id], vec![], None, None);
    }

    pub fn remove_child(&mut self, parent_id: NodeId, child_id: NodeId) {
        if let Some(parent) = self.nodes.get_mut(&parent_id) {
            parent.children.retain(|&id| id != child_id);
        }
        if let Some(child) = self.nodes.get_mut(&child_id) {
            child.parent_id = None;
        }
        self.queue_mutation("childList", parent_id, vec![], vec![child_id], None, None);
        self.remove_recursive(child_id);
    }

    fn remove_recursive(&mut self, node_id: NodeId) {
        if let Some(node) = self.nodes.get(&node_id) {
            let children: Vec<NodeId> = node.children.clone();
            for cid in children {
                self.remove_recursive(cid);
            }
        }
        self.remove_from_indexes(node_id);
        self.nodes.remove(&node_id);
    }

    pub fn has_observers(&self) -> bool {
        !self.observers.is_empty()
    }

    /// Check whether `node_id` is a descendant of (or equal to) `ancestor_id`.
    fn is_descendant_or_self(&self, node_id: u32, ancestor_id: u32) -> bool {
        if node_id == ancestor_id {
            return true;
        }
        let mut current = node_id;
        loop {
            let parent = match self.nodes.get(&current) {
                Some(n) => n.parent_id,
                None => return false,
            };
            match parent {
                Some(pid) if pid == ancestor_id => return true,
                Some(pid) => current = pid,
                None => return false,
            }
        }
    }

    /// Return the IDs of observers that should receive a given mutation record.
    fn observers_for_mutation(&self, record: &MutationRecord) -> Vec<u32> {
        let mut matched = Vec::new();
        for obs in &self.observers {
            // Target check: must match exactly or be a descendant (if subtree)
            let target_ok = record.target == obs.target_node_id
                || (obs.options.subtree
                    && self.is_descendant_or_self(record.target, obs.target_node_id));
            if !target_ok {
                continue;
            }

            // Type-specific option checks
            let type_ok = match record.type_.as_str() {
                "childList" => obs.options.child_list,
                "attributes" => {
                    if !obs.options.attributes {
                        false
                    } else if !obs.options.attribute_filter.is_empty() {
                        match &record.attribute_name {
                            Some(name) => obs.options.attribute_filter.contains(name),
                            None => false,
                        }
                    } else {
                        true
                    }
                }
                "characterData" => obs.options.character_data,
                _ => true,
            };
            if type_ok {
                matched.push(obs.id);
            }
        }
        matched
    }

    pub fn queue_mutation(
        &mut self,
        type_: &str,
        target: u32,
        added_nodes: Vec<u32>,
        removed_nodes: Vec<u32>,
        attribute_name: Option<String>,
        old_value: Option<String>,
    ) {
        if self.observers.is_empty() {
            return;
        }
        let record = MutationRecord {
            type_: type_.to_string(),
            target,
            added_nodes,
            removed_nodes,
            attribute_name,
            old_value,
        };
        let observer_ids = self.observers_for_mutation(&record);
        for obs_id in observer_ids {
            self.pending_mutations
                .entry(obs_id)
                .or_default()
                .push(record.clone());
        }
    }

    /// Enqueue a simple mutation (no added/removed nodes).
    pub fn queue_simple_mutation(&mut self, type_: &str, target: u32) {
        self.queue_mutation(type_, target, vec![], vec![], None, None);
    }

    /// Drain all pending mutations grouped by observer ID.
    pub fn drain_all_pending_mutations(&mut self) -> Vec<(u32, Vec<MutationRecord>)> {
        let mut result = Vec::new();
        let keys: Vec<u32> = self.pending_mutations.keys().copied().collect();
        for k in keys {
            if let Some(records) = self.pending_mutations.remove(&k) {
                if !records.is_empty() {
                    result.push((k, records));
                }
            }
        }
        result
    }

    pub fn register_observer(&mut self, target_node_id: u32, options: MutationObserverInit) -> u32 {
        let id = self.next_observer_id;
        self.next_observer_id += 1;
        self.observers.push(ObserverEntry {
            id,
            target_node_id,
            options,
        });
        id
    }

    pub fn disconnect_observer(&mut self, observer_id: u32) {
        self.observers.retain(|o| o.id != observer_id);
    }

    pub fn take_mutation_records(&mut self) -> Vec<MutationRecord> {
        std::mem::take(&mut self.mutation_records)
    }

    pub fn set_attribute(&mut self, node_id: NodeId, name: &str, value: &str) {
        // Capture old value before overwriting
        let old_val = self
            .nodes
            .get(&node_id)
            .and_then(|n| n.attributes.get(name).cloned());

        if let Some(node) = self.nodes.get_mut(&node_id) {
            if name == "id" {
                if let Some(old_id) = node.attributes.get("id") {
                    if self.id_index.get(old_id) == Some(&node_id) {
                        self.id_index.remove(old_id);
                    }
                }
                node.attributes.insert(name.to_string(), value.to_string());
                if !value.is_empty() {
                    self.id_index.insert(value.to_string(), node_id);
                }
            } else if name == "class" {
                node.attributes.insert(name.to_string(), value.to_string());
                self.rebuild_class_index_for_node(node_id);
            } else {
                node.attributes.insert(name.to_string(), value.to_string());
            }
        }
        self.queue_mutation(
            "attributes",
            node_id,
            vec![],
            vec![],
            Some(name.to_string()),
            old_val,
        );
    }

    pub fn get_attribute(&self, node_id: NodeId, name: &str) -> Option<String> {
        self.nodes
            .get(&node_id)
            .and_then(|n| n.attributes.get(name).cloned())
    }

    pub fn remove_attribute(&mut self, node_id: NodeId, name: &str) {
        let old_val = self
            .nodes
            .get(&node_id)
            .and_then(|n| n.attributes.get(name).cloned());

        if let Some(node) = self.nodes.get_mut(&node_id) {
            if name == "id" {
                if let Some(old_id) = node.attributes.remove("id") {
                    if self.id_index.get(&old_id) == Some(&node_id) {
                        self.id_index.remove(&old_id);
                    }
                }
            } else if name == "class" {
                node.attributes.remove(name);
                self.rebuild_class_index_for_node(node_id);
            } else {
                node.attributes.remove(name);
            }
        }
        self.queue_mutation(
            "attributes",
            node_id,
            vec![],
            vec![],
            Some(name.to_string()),
            old_val,
        );
    }

    pub fn set_inner_html(&mut self, node_id: NodeId, html: &str) {
        // Remove existing children (indexes updated in remove_recursive)
        let old_children: Vec<NodeId> = self
            .nodes
            .get(&node_id)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        for &old_id in &old_children {
            self.remove_recursive(old_id);
        }
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.children.clear();
        }
        // Parse and add new children
        let fragment = Html::parse_fragment(html);
        for node_ref in fragment.tree.nodes() {
            if let Some(el) = ElementRef::wrap(node_ref) {
                let child_id = self.build_from_scraper(&el, Some(node_id));
                self.rebuild_indexes_for_subtree(child_id);
            } else if let Some(text) = node_ref.value().as_text() {
                if !text.text.trim().is_empty() {
                    let text_id = self.alloc_text(&text.text, Some(node_id));
                    if let Some(parent) = self.nodes.get_mut(&node_id) {
                        parent.children.push(text_id);
                    }
                }
            }
        }
        // Capture new children after parse
        let new_children: Vec<NodeId> = self
            .nodes
            .get(&node_id)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        self.queue_mutation("childList", node_id, new_children, old_children, None, None);
    }

    pub fn get_inner_html(&self, node_id: NodeId) -> String {
        let mut output = String::new();
        if let Some(node) = self.nodes.get(&node_id) {
            for &child_id in &node.children {
                self.serialize_node(child_id, &mut output);
            }
        }
        output
    }

    pub fn get_text_content(&self, node_id: NodeId) -> String {
        let node = match self.nodes.get(&node_id) {
            Some(n) => n,
            None => return String::new(),
        };
        match node.node_type {
            DomNodeType::Text | DomNodeType::Comment => {
                node.text_content.clone().unwrap_or_default()
            }
            _ => {
                let mut text = String::new();
                for &child_id in &node.children {
                    text.push_str(&self.get_text_content(child_id));
                }
                text
            }
        }
    }

    pub fn set_text_content(&mut self, node_id: NodeId, text: &str) {
        // Remove old children (indexes updated in remove_recursive)
        let old_children: Vec<NodeId> = self
            .nodes
            .get(&node_id)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        for &old_id in &old_children {
            self.remove_recursive(old_id);
        }
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.children.clear();
        }
        let text_id = self.alloc_text(text, Some(node_id));
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.children.push(text_id);
        }
        self.queue_mutation(
            "childList",
            node_id,
            vec![text_id],
            old_children,
            None,
            None,
        );
    }

    pub fn get_element_by_id(&self, id: &str) -> Option<NodeId> {
        self.id_index.get(id).copied()
    }

    pub fn get_parent(&self, node_id: NodeId) -> Option<NodeId> {
        self.nodes.get(&node_id).and_then(|n| n.parent_id)
    }

    // ---- Accessors ----

    /// Current number of nodes in the document.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Set the maximum number of nodes allowed.
    pub fn set_max_nodes(&mut self, max: usize) {
        self.max_nodes = Some(max);
    }

    /// Check if a new node can be created within the limit.
    fn can_alloc(&self) -> bool {
        match self.max_nodes {
            Some(max) => self.nodes.len() < max,
            None => true,
        }
    }

    pub fn document_element(&self) -> NodeId {
        self.document_element_id
    }
    pub fn head(&self) -> NodeId {
        self.head_id
    }
    pub fn body(&self) -> NodeId {
        self.body_id
    }

    pub fn get_tag_name(&self, node_id: NodeId) -> Option<String> {
        self.nodes.get(&node_id).and_then(|n| n.tag_name.clone())
    }

    pub fn get_children(&self, node_id: NodeId) -> Vec<NodeId> {
        self.nodes
            .get(&node_id)
            .map(|n| n.children.clone())
            .unwrap_or_default()
    }

    pub fn get_class_name(&self, node_id: NodeId) -> String {
        self.get_attribute(node_id, "class").unwrap_or_default()
    }

    pub fn set_class_name(&mut self, node_id: NodeId, class_name: &str) {
        self.set_attribute(node_id, "class", class_name);
    }

    pub fn get_node_id_attr(&self, node_id: NodeId) -> String {
        self.get_attribute(node_id, "id").unwrap_or_default()
    }

    pub fn set_node_id_attr(&mut self, node_id: NodeId, id: &str) {
        self.set_attribute(node_id, "id", id);
    }

    pub fn set_style(&mut self, node_id: NodeId, property: &str, value: &str) {
        let existing = self.get_attribute(node_id, "style").unwrap_or_default();
        let style = format_style_property(&existing, property, value);
        self.set_attribute(node_id, "style", &style);
    }

    // ---- Query Selector Support ----

    /// Query for the first element matching a CSS selector, starting from a given node.
    /// If node_id is 0, searches from document element.
    pub fn query_selector(&self, node_id: NodeId, selector: &str) -> Option<NodeId> {
        let start_node = if node_id == 0 {
            self.document_element_id
        } else {
            node_id
        };

        let s = selector.trim();

        // Fast path: #id
        if let Some(id) = s.strip_prefix('#') {
            if let Some(&nid) = self.id_index.get(id) {
                if self.is_descendant_or_self(nid, start_node) {
                    return Some(nid);
                }
            }
            return None;
        }

        // Fast path: .class
        if let Some(class) = s.strip_prefix('.') {
            if let Some(ids) = self.class_index.get(class) {
                for &nid in ids {
                    if self.is_descendant_or_self(nid, start_node) {
                        return Some(nid);
                    }
                }
            }
            return None;
        }

        // Fast path: tag or tag.class
        if !s.is_empty() && s.chars().next().map_or(false, |c| c.is_alphabetic()) {
            if let Some(dot_pos) = s.find('.') {
                let tag = s[..dot_pos].to_lowercase();
                let class = &s[dot_pos + 1..];
                if let Some(tag_ids) = self.tag_index.get(&tag) {
                    if let Some(class_ids) = self.class_index.get(class) {
                        let class_set: std::collections::HashSet<NodeId> =
                            class_ids.iter().copied().collect();
                        for &nid in tag_ids {
                            if class_set.contains(&nid)
                                && self.is_descendant_or_self(nid, start_node)
                            {
                                return Some(nid);
                            }
                        }
                    }
                }
                // Fall through to native/selector parsing
            } else if s.chars().all(|c| c.is_alphanumeric() || c == '-') {
                let tag = s.to_lowercase();
                let mut result = None;
                self.find_element_by_tag(start_node, &tag, &mut result);
                return result;
            }
        }

        // Native matching for attribute selectors and compound selectors
        // (avoids HTML serialization + re-parsing per element)
        if let Some(nsel) = try_parse_native_selector(s) {
            let mut results = Vec::new();
            self.collect_native_matches(start_node, &nsel, &mut results, true);
            if let Some(&nid) = results.first() {
                if self.is_descendant_or_self(nid, start_node) {
                    return Some(nid);
                }
            }
            return None;
        }

        let css_selector = match Selector::parse(selector) {
            Ok(s) => s,
            Err(_) => return None,
        };

        self.query_selector_recursive(start_node, &css_selector)
    }

    fn find_element_by_tag(&self, node_id: NodeId, tag: &str, result: &mut Option<NodeId>) {
        if result.is_some() {
            return;
        }
        let node = match self.nodes.get(&node_id) {
            Some(n) => n,
            None => return,
        };
        if node.node_type == DomNodeType::Element {
            if node.tag_name.as_deref() == Some(tag) {
                *result = Some(node_id);
                return;
            }
        }
        for &child_id in &node.children {
            self.find_element_by_tag(child_id, tag, result);
            if result.is_some() {
                return;
            }
        }
    }

    fn query_selector_recursive(&self, node_id: NodeId, selector: &Selector) -> Option<NodeId> {
        let node = self.nodes.get(&node_id)?;

        // Check if current node matches
        if node.node_type == DomNodeType::Element {
            if self.node_matches_selector(node_id, selector) {
                return Some(node_id);
            }
        }

        // Search children depth-first
        for &child_id in &node.children {
            if let Some(found) = self.query_selector_recursive(child_id, selector) {
                return Some(found);
            }
        }

        None
    }

    /// Query for all elements matching a CSS selector, starting from a given node.
    /// If node_id is 0, searches from document element.
    pub fn query_selector_all(&self, node_id: NodeId, selector: &str) -> Vec<NodeId> {
        let start_node = if node_id == 0 {
            self.document_element_id
        } else {
            node_id
        };

        let s = selector.trim();

        // Fast path: #id
        if let Some(id) = s.strip_prefix('#') {
            if let Some(&nid) = self.id_index.get(id) {
                if self.is_descendant_or_self(nid, start_node) {
                    return vec![nid];
                }
            }
            return Vec::new();
        }

        // Fast path: .class
        if let Some(class) = s.strip_prefix('.') {
            if let Some(ids) = self.class_index.get(class) {
                let mut results: Vec<NodeId> = ids
                    .iter()
                    .filter(|&&nid| self.is_descendant_or_self(nid, start_node))
                    .copied()
                    .collect();
                results.sort();
                return results;
            }
            return Vec::new();
        }

        // Fast path: tag or tag.class
        if !s.is_empty() && s.chars().next().map_or(false, |c| c.is_alphabetic()) {
            if let Some(dot_pos) = s.find('.') {
                let tag = s[..dot_pos].to_lowercase();
                let class = &s[dot_pos + 1..];
                if let Some(tag_ids) = self.tag_index.get(&tag) {
                    if let Some(class_ids) = self.class_index.get(class) {
                        let class_set: std::collections::HashSet<NodeId> =
                            class_ids.iter().copied().collect();
                        let mut results: Vec<NodeId> = tag_ids
                            .iter()
                            .filter(|&&nid| {
                                class_set.contains(&nid)
                                    && self.is_descendant_or_self(nid, start_node)
                            })
                            .copied()
                            .collect();
                        results.sort();
                        return results;
                    }
                }
            } else if s.chars().all(|c| c.is_alphanumeric() || c == '-') {
                let tag = s.to_lowercase();
                let mut results = Vec::new();
                self.collect_elements_by_tag(start_node, &tag, &mut results);
                return results;
            }
        }

        // Native matching for attribute selectors and compound selectors
        if let Some(nsel) = try_parse_native_selector(s) {
            let mut results = Vec::new();
            self.collect_native_matches(start_node, &nsel, &mut results, false);
            results.retain(|&nid| self.is_descendant_or_self(nid, start_node));
            results.sort();
            return results;
        }

        let css_selector = match Selector::parse(selector) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let mut results = Vec::new();
        self.query_selector_all_recursive(start_node, &css_selector, &mut results);
        results
    }

    fn collect_elements_by_tag(&self, node_id: NodeId, tag: &str, results: &mut Vec<NodeId>) {
        let node = match self.nodes.get(&node_id) {
            Some(n) => n,
            None => return,
        };
        if node.node_type == DomNodeType::Element {
            if node.tag_name.as_deref() == Some(tag) {
                results.push(node_id);
            }
        }
        for &child_id in &node.children {
            self.collect_elements_by_tag(child_id, tag, results);
        }
    }

    fn query_selector_all_recursive(
        &self,
        node_id: NodeId,
        selector: &Selector,
        results: &mut Vec<NodeId>,
    ) {
        let node = match self.nodes.get(&node_id) {
            Some(n) => n,
            None => return,
        };

        // Check if current node matches
        if node.node_type == DomNodeType::Element {
            if self.node_matches_selector(node_id, selector) {
                results.push(node_id);
            }
        }

        // Search children
        for &child_id in &node.children {
            self.query_selector_all_recursive(child_id, selector, results);
        }
    }

    /// Check if a node matches a CSS selector
    fn node_matches_selector(&self, node_id: NodeId, selector: &Selector) -> bool {
        let node = match self.nodes.get(&node_id) {
            Some(n) => n,
            None => return false,
        };

        // Build a temporary HTML element for scraper to match against
        let _tag = match &node.tag_name {
            Some(t) => t.clone(),
            None => return false,
        };

        // Create a minimal HTML representation for this element
        let html = self.node_to_minimal_html(node_id);

        // Parse and check if selector matches
        let doc = Html::parse_fragment(&html);
        if let Some(el) = doc.select(selector).next() {
            // Verify it's the same element by checking a data attribute we add
            return el
                .value()
                .attr("data-pardus-node-id")
                .map(|s| s == node_id.to_string())
                .unwrap_or(false);
        }

        false
    }

    /// Convert a node to minimal HTML for selector matching
    fn node_to_minimal_html(&self, node_id: NodeId) -> String {
        let node = match self.nodes.get(&node_id) {
            Some(n) => n,
            None => return String::new(),
        };

        let tag = node.tag_name.as_deref().unwrap_or("div");
        let mut html = format!("<{} data-pardus-node-id=\"{}\"", tag, node_id);

        for (k, v) in &node.attributes {
            html.push_str(&format!(" {}=\"{}\"", k, v.replace('"', "&quot;")));
        }

        html.push_str("></");
        html.push_str(tag);
        html.push('>');

        html
    }

    // ---- Native Selector Matching ----

    /// Try to match a single node against a parsed NativeSelector.
    fn node_matches_native(&self, node_id: NodeId, sel: &NativeSelector) -> bool {
        let node = match self.nodes.get(&node_id) {
            Some(n) => n,
            None => return false,
        };
        if node.node_type != DomNodeType::Element {
            return false;
        }

        if let Some(ref tag) = sel.tag {
            if *tag != "*" && node.tag_name.as_deref() != Some(tag.as_str()) {
                return false;
            }
        }

        if !sel.classes.is_empty() {
            let node_classes: HashSet<&str> = node
                .attributes
                .get("class")
                .map(|c| c.split_whitespace().collect())
                .unwrap_or_default();
            if !sel
                .classes
                .iter()
                .all(|c| node_classes.contains(c.as_str()))
            {
                return false;
            }
        }

        for (attr_name, expected_val) in &sel.attrs {
            match expected_val {
                Some(val) => {
                    if node.attributes.get(attr_name.as_str()) != Some(val) {
                        return false;
                    }
                }
                None => {
                    if !node.attributes.contains_key(attr_name.as_str()) {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Walk the tree and collect nodes matching a NativeSelector.
    fn collect_native_matches(
        &self,
        node_id: NodeId,
        sel: &NativeSelector,
        results: &mut Vec<NodeId>,
        stop_at_first: bool,
    ) {
        let node = match self.nodes.get(&node_id) {
            Some(n) => n,
            None => return,
        };

        if node.node_type == DomNodeType::Element && self.node_matches_native(node_id, sel) {
            results.push(node_id);
            if stop_at_first {
                return;
            }
        }

        for &child_id in &node.children {
            self.collect_native_matches(child_id, sel, results, stop_at_first);
            if stop_at_first && !results.is_empty() {
                return;
            }
        }
    }

    // ---- Extended Element API ----

    /// Insert a node before a reference node
    pub fn insert_before(
        &mut self,
        parent_id: NodeId,
        new_node_id: NodeId,
        ref_node_id: Option<NodeId>,
    ) {
        // Remove from old parent
        if let Some(old_parent) = self.nodes.get(&new_node_id).and_then(|n| n.parent_id) {
            if let Some(old) = self.nodes.get_mut(&old_parent) {
                old.children.retain(|&id| id != new_node_id);
            }
        }

        // Set new parent
        if let Some(new_node) = self.nodes.get_mut(&new_node_id) {
            new_node.parent_id = Some(parent_id);
        }

        // Insert at correct position
        if let Some(parent) = self.nodes.get_mut(&parent_id) {
            match ref_node_id {
                Some(ref_id) => {
                    if let Some(pos) = parent.children.iter().position(|&id| id == ref_id) {
                        parent.children.insert(pos, new_node_id);
                    } else {
                        parent.children.push(new_node_id);
                    }
                }
                None => {
                    parent.children.push(new_node_id);
                }
            }
        }
        self.queue_mutation(
            "childList",
            parent_id,
            vec![new_node_id],
            vec![],
            None,
            None,
        );
    }

    /// Replace a child node with another
    pub fn replace_child(&mut self, parent_id: NodeId, new_child_id: NodeId, old_child_id: NodeId) {
        // Remove new child from old parent
        if let Some(old_parent) = self.nodes.get(&new_child_id).and_then(|n| n.parent_id) {
            if let Some(old) = self.nodes.get_mut(&old_parent) {
                old.children.retain(|&id| id != new_child_id);
            }
        }

        // Set parent for new child
        if let Some(new_child) = self.nodes.get_mut(&new_child_id) {
            new_child.parent_id = Some(parent_id);
        }

        // Replace in parent's children
        if let Some(parent) = self.nodes.get_mut(&parent_id) {
            if let Some(pos) = parent.children.iter().position(|&id| id == old_child_id) {
                parent.children[pos] = new_child_id;
            }
        }

        // Remove old child
        if let Some(old_child) = self.nodes.get_mut(&old_child_id) {
            old_child.parent_id = None;
        }
        self.remove_recursive(old_child_id);
        self.queue_mutation(
            "childList",
            parent_id,
            vec![new_child_id],
            vec![old_child_id],
            None,
            None,
        );
    }

    /// Clone a node
    pub fn clone_node(&mut self, node_id: NodeId, deep: bool) -> NodeId {
        self.clone_node_internal(node_id, None, deep)
    }

    fn clone_node_internal(
        &mut self,
        node_id: NodeId,
        parent_id: Option<NodeId>,
        deep: bool,
    ) -> NodeId {
        let node = match self.nodes.get(&node_id) {
            Some(n) => n.clone(),
            None => return 0,
        };

        let new_id = self.alloc_node(node.node_type.clone(), parent_id);

        if let Some(new_node) = self.nodes.get_mut(&new_id) {
            new_node.tag_name = node.tag_name;
            new_node.attributes = node.attributes;
            new_node.text_content = node.text_content;
        }

        if self
            .nodes
            .get(&new_id)
            .map_or(false, |n| n.node_type == DomNodeType::Element)
        {
            self.add_to_indexes(new_id);
        }

        if deep {
            for &child_id in &node.children {
                let cloned_child = self.clone_node_internal(child_id, Some(new_id), true);
                if let Some(new_node) = self.nodes.get_mut(&new_id) {
                    new_node.children.push(cloned_child);
                }
            }
        }

        new_id
    }

    /// Check if a node contains another node
    pub fn contains(&self, container_id: NodeId, contained_id: NodeId) -> bool {
        if container_id == contained_id {
            return true;
        }

        let mut current_id = contained_id;
        while let Some(node) = self.nodes.get(&current_id) {
            match node.parent_id {
                Some(pid) if pid == container_id => return true,
                Some(pid) => current_id = pid,
                None => return false,
            }
        }
        false
    }

    /// Check if node has child nodes
    pub fn has_child_nodes(&self, node_id: NodeId) -> bool {
        self.nodes
            .get(&node_id)
            .map(|n| !n.children.is_empty())
            .unwrap_or(false)
    }

    /// Check if node has attributes
    pub fn has_attributes(&self, node_id: NodeId) -> bool {
        self.nodes
            .get(&node_id)
            .map(|n| !n.attributes.is_empty())
            .unwrap_or(false)
    }

    /// Get previous sibling
    pub fn get_previous_sibling(&self, node_id: NodeId) -> Option<NodeId> {
        let parent_id = self.nodes.get(&node_id)?.parent_id?;
        let parent = self.nodes.get(&parent_id)?;
        let idx = parent.children.iter().position(|&id| id == node_id)?;
        if idx > 0 {
            Some(parent.children[idx - 1])
        } else {
            None
        }
    }

    /// Get node type as number (matches DOM Node.ELEMENT_NODE, etc.)
    pub fn get_node_type(&self, node_id: NodeId) -> u16 {
        self.nodes
            .get(&node_id)
            .map(|n| match n.node_type {
                DomNodeType::Element => 1,
                DomNodeType::Text => 3,
                DomNodeType::Comment => 8,
                DomNodeType::Document => 9,
                DomNodeType::DocumentFragment => 11,
                DomNodeType::ShadowRoot => 11,
            })
            .unwrap_or(0)
    }

    /// Get node name (tagName for elements, #text for text nodes, etc.)
    pub fn get_node_name(&self, node_id: NodeId) -> String {
        self.nodes
            .get(&node_id)
            .map(|n| match &n.node_type {
                DomNodeType::Element => n.tag_name.clone().unwrap_or_default().to_uppercase(),
                DomNodeType::Text => "#text".to_string(),
                DomNodeType::Comment => "#comment".to_string(),
                DomNodeType::Document => "#document".to_string(),
                DomNodeType::DocumentFragment => "#document-fragment".to_string(),
                DomNodeType::ShadowRoot => "#shadow-root".to_string(),
            })
            .unwrap_or_default()
    }

    /// Get all attribute names
    pub fn get_attribute_names(&self, node_id: NodeId) -> Vec<String> {
        self.nodes
            .get(&node_id)
            .map(|n| n.attributes.keys().cloned().collect())
            .unwrap_or_default()
    }

    // ---- Outer HTML ----

    pub fn get_outer_html(&self, node_id: NodeId) -> String {
        let mut output = String::new();
        self.serialize_node(node_id, &mut output);
        output
    }

    // ---- getElementsByTagName ----

    pub fn get_elements_by_tag_name(&self, node_id: NodeId, tag: &str) -> Vec<NodeId> {
        let tag_lower = tag.to_lowercase();
        let all_elements = self.collect_all_elements(node_id);
        if tag_lower == "*" {
            all_elements
        } else {
            all_elements
                .into_iter()
                .filter(|&nid| {
                    self.nodes
                        .get(&nid)
                        .and_then(|n| n.tag_name.as_deref())
                        .map_or(false, |t| t == tag_lower)
                })
                .collect()
        }
    }

    // ---- getElementsByClassName ----

    pub fn get_elements_by_class_name(&self, node_id: NodeId, class_name: &str) -> Vec<NodeId> {
        let target_classes: std::collections::HashSet<&str> =
            class_name.split_whitespace().collect();
        if target_classes.is_empty() {
            return Vec::new();
        }
        self.collect_all_elements(node_id)
            .into_iter()
            .filter(|&nid| {
                self.nodes
                    .get(&nid)
                    .and_then(|n| n.attributes.get("class"))
                    .map_or(false, |cls| {
                        let classes: std::collections::HashSet<&str> =
                            cls.split_whitespace().collect();
                        target_classes.iter().all(|c| classes.contains(*c))
                    })
            })
            .collect()
    }

    // ---- Element traversal ----

    pub fn first_element_child(&self, node_id: NodeId) -> Option<NodeId> {
        let node = self.nodes.get(&node_id)?;
        node.children
            .iter()
            .find(|&&cid| {
                self.nodes
                    .get(&cid)
                    .map_or(false, |n| n.node_type == DomNodeType::Element)
            })
            .copied()
    }

    pub fn last_element_child(&self, node_id: NodeId) -> Option<NodeId> {
        let node = self.nodes.get(&node_id)?;
        node.children
            .iter()
            .rev()
            .find(|&&cid| {
                self.nodes
                    .get(&cid)
                    .map_or(false, |n| n.node_type == DomNodeType::Element)
            })
            .copied()
    }

    pub fn next_element_sibling(&self, node_id: NodeId) -> Option<NodeId> {
        let parent_id = self.nodes.get(&node_id)?.parent_id?;
        let parent = self.nodes.get(&parent_id)?;
        let idx = parent.children.iter().position(|&id| id == node_id)?;
        for i in (idx + 1)..parent.children.len() {
            let cid = parent.children[i];
            if self
                .nodes
                .get(&cid)
                .map_or(false, |n| n.node_type == DomNodeType::Element)
            {
                return Some(cid);
            }
        }
        None
    }

    pub fn previous_element_sibling(&self, node_id: NodeId) -> Option<NodeId> {
        let parent_id = self.nodes.get(&node_id)?.parent_id?;
        let parent = self.nodes.get(&parent_id)?;
        let idx = parent.children.iter().position(|&id| id == node_id)?;
        if idx == 0 {
            return None;
        }
        for i in (0..idx).rev() {
            let cid = parent.children[i];
            if self
                .nodes
                .get(&cid)
                .map_or(false, |n| n.node_type == DomNodeType::Element)
            {
                return Some(cid);
            }
        }
        None
    }

    // ---- Title ----

    pub fn get_title(&self) -> String {
        self.title.clone().unwrap_or_default()
    }

    pub fn set_title(&mut self, title: &str) -> String {
        let old = self.title.clone().unwrap_or_default();
        self.title = Some(title.to_string());

        if self.head_id != 0 {
            let mut title_node_id: Option<NodeId> = None;
            if let Some(head_node) = self.nodes.get(&self.head_id) {
                for &child_id in &head_node.children {
                    if let Some(child) = self.nodes.get(&child_id) {
                        if child.tag_name.as_deref() == Some("title") {
                            title_node_id = Some(child_id);
                            break;
                        }
                    }
                }
            }

            if let Some(tid) = title_node_id {
                let old_children: Vec<NodeId> = self
                    .nodes
                    .get(&tid)
                    .map(|n| n.children.clone())
                    .unwrap_or_default();
                for old_id in old_children {
                    self.remove_recursive(old_id);
                }
                if let Some(title_node) = self.nodes.get_mut(&tid) {
                    title_node.children.clear();
                }
                let text_id = self.alloc_text(title, Some(tid));
                if let Some(title_node) = self.nodes.get_mut(&tid) {
                    title_node.children.push(text_id);
                }
            } else {
                let title_el = self.alloc_element("title", Some(self.head_id));
                let text_id = self.alloc_text(title, Some(title_el));
                if let Some(title_node) = self.nodes.get_mut(&title_el) {
                    title_node.children.push(text_id);
                }
                if let Some(head_node) = self.nodes.get_mut(&self.head_id) {
                    head_node.children.push(title_el);
                }
            }
        }

        old
    }

    // ---- Helper methods ----

    fn filter_descendants(
        &self,
        node_id: NodeId,
        predicate: impl Fn(NodeId) -> bool,
    ) -> Vec<NodeId> {
        let mut results = Vec::new();
        self.filter_descendants_recursive(node_id, &predicate, &mut results);
        results
    }

    fn filter_descendants_recursive(
        &self,
        node_id: NodeId,
        predicate: &dyn Fn(NodeId) -> bool,
        results: &mut Vec<NodeId>,
    ) {
        let node = match self.nodes.get(&node_id) {
            Some(n) => n,
            None => return,
        };
        for &child_id in &node.children {
            if predicate(child_id) {
                results.push(child_id);
            }
            self.filter_descendants_recursive(child_id, predicate, results);
        }
    }

    fn collect_all_elements(&self, node_id: NodeId) -> Vec<NodeId> {
        self.filter_descendants(node_id, |nid| {
            self.nodes
                .get(&nid)
                .map_or(false, |n| n.node_type == DomNodeType::Element)
        })
    }

    // Shadow DOM stubs (for test compatibility — full shadow DOM not yet implemented)

    /// Returns `None` — shadow DOM is not yet supported.
    pub fn get_shadow_root(&self, _node_id: NodeId) -> Option<NodeId> {
        None
    }

    /// Returns `false` — shadow DOM is not yet supported.
    pub fn is_shadow_host(&self, _node_id: NodeId) -> bool {
        false
    }

    /// Collect all descendant elements (no shadow DOM piercing).
    pub fn collect_all_elements_deep(&self, node_id: NodeId) -> Vec<NodeId> {
        let mut results = Vec::new();
        self.collect_all_elements_deep_recursive(node_id, &mut results);
        results
    }

    fn collect_all_elements_deep_recursive(&self, node_id: NodeId, results: &mut Vec<NodeId>) {
        let node = match self.nodes.get(&node_id) {
            Some(n) => n,
            None => return,
        };
        if node.node_type == DomNodeType::Element {
            results.push(node_id);
        }
        for &child_id in &node.children {
            self.collect_all_elements_deep_recursive(child_id, results);
        }
    }

    /// Query selector alias (no shadow DOM piercing).
    pub fn query_selector_deep(&self, node_id: NodeId, selector: &str) -> Option<NodeId> {
        self.query_selector(node_id, selector)
    }

    /// Query selector all alias (no shadow DOM piercing).
    pub fn query_selector_all_deep(&self, node_id: NodeId, selector: &str) -> Vec<NodeId> {
        self.query_selector_all(node_id, selector)
    }
}

// ---- Native Selector Parser ----

/// Decomposed CSS selector for native matching (no HTML serialization).
struct NativeSelector {
    tag: Option<String>,
    classes: Vec<String>,
    attrs: Vec<(String, Option<String>)>,
}

/// Try to parse a simple CSS selector into a NativeSelector.
/// Returns None for complex selectors (descendant combinators, pseudo-elements, etc.)
/// that should fall through to the scraper-based matcher.
fn try_parse_native_selector(s: &str) -> Option<NativeSelector> {
    // Reject descendant combinators and complex selectors
    if s.is_empty()
        || s.contains(|c: char| c.is_whitespace() || c == '>' || c == '+' || c == '~' || c == ',')
    {
        return None;
    }

    let mut sel = NativeSelector {
        tag: None,
        classes: Vec::new(),
        attrs: Vec::new(),
    };

    let mut rest = s;

    // Extract tag name if it starts with alpha or *
    if rest.starts_with('*') {
        sel.tag = Some("*".to_string());
        rest = &rest[1..];
    } else if let Some(c) = rest.chars().next() {
        if c.is_alphabetic() {
            let end = rest
                .find(|c: char| !c.is_alphanumeric() && c != '-')
                .unwrap_or(rest.len());
            sel.tag = Some(rest[..end].to_lowercase());
            rest = &rest[end..];
        }
    }

    // Extract classes, IDs, and attribute selectors
    while !rest.is_empty() {
        if let Some(class_end) = strip_prefix(rest, '.') {
            rest = class_end;
            let end = rest
                .find(|c: char| c == '.' || c == '[' || c == '#' || c == ':')
                .unwrap_or(rest.len());
            sel.classes.push(rest[..end].to_string());
            rest = &rest[end..];
        } else if let Some(id_end) = strip_prefix(rest, '#') {
            rest = id_end;
            let end = rest
                .find(|c: char| c == '.' || c == '[' || c == '#' || c == ':')
                .unwrap_or(rest.len());
            rest = &rest[end..];
        } else if let Some(inner_end) = strip_prefix(rest, '[') {
            let close = inner_end.find(']')?;
            let inner = &inner_end[..close];
            rest = &inner_end[close + 1..];

            if let Some(eq_pos) = inner.find('=') {
                let attr_name = inner[..eq_pos].trim().to_string();
                let val_part = inner[eq_pos + 1..].trim();
                let value = if (val_part.starts_with('"') && val_part.ends_with('"'))
                    || (val_part.starts_with('\'') && val_part.ends_with('\''))
                {
                    val_part[1..val_part.len() - 1].to_string()
                } else {
                    val_part.to_string()
                };
                sel.attrs.push((attr_name, Some(value)));
            } else {
                sel.attrs.push((inner.trim().to_string(), None));
            }
        } else if let Some(_pseudo_end) = strip_prefix(rest, ':') {
            // Pseudo-selectors not supported natively — bail to scraper
            return None;
        } else {
            return None;
        }
    }

    // Must have at least one constraint
    if sel.tag.is_none() && sel.classes.is_empty() && sel.attrs.is_empty() {
        return None;
    }

    Some(sel)
}

fn strip_prefix<'a>(s: &'a str, prefix: char) -> Option<&'a str> {
    if s.starts_with(prefix) {
        Some(&s[1..])
    } else {
        None
    }
}

fn format_style_property(existing: &str, property: &str, value: &str) -> String {
    let target = format!("{}:", property);
    let mut found = false;
    let parts: Vec<String> = existing
        .split(';')
        .filter_map(|p| {
            let p = p.trim();
            if p.is_empty() {
                return None;
            }
            if p.starts_with(&target) {
                found = true;
                Some(format!("{}: {}", property, value))
            } else {
                Some(p.to_string())
            }
        })
        .collect();
    let mut result = parts.join("; ");
    if !found {
        if !result.is_empty() {
            result.push_str("; ");
        }
        result.push_str(&format!("{}: {}", property, value));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_trip() {
        let html =
            "<html><head></head><body><h1>Hello</h1><p class=\"intro\">World</p></body></html>";
        let doc = DomDocument::from_html(html);
        let output = doc.to_html();
        assert!(output.contains("<h1>"));
        assert!(output.contains("Hello"));
        assert!(output.contains("<p"));
        assert!(output.contains("class=\"intro\""));
    }

    #[test]
    fn test_create_and_append() {
        let html = "<html><head></head><body></body></html>";
        let mut doc = DomDocument::from_html(html);
        let body = doc.body();
        let div = doc.create_element("div");
        doc.set_attribute(div, "id", "app");
        doc.append_child(body, div);
        let output = doc.to_html();
        assert!(output.contains("<div id=\"app\">"));
    }

    #[test]
    fn test_set_inner_html() {
        let html = "<html><head></head><body><div id=\"app\"></div></body></html>";
        let mut doc = DomDocument::from_html(html);
        let app = doc.get_element_by_id("app").unwrap();
        doc.set_inner_html(app, "<p>Rendered!</p>");
        let output = doc.to_html();
        assert!(output.contains("Rendered!"));
    }

    #[test]
    fn test_get_element_by_id() {
        let html = "<html><body><div id=\"foo\">bar</div></body></html>";
        let doc = DomDocument::from_html(html);
        let id = doc.get_element_by_id("foo").unwrap();
        assert_eq!(doc.get_text_content(id), "bar");
    }

    #[test]
    fn test_get_parent() {
        let html = "<html><body><div id=\"child\">x</div></body></html>";
        let doc = DomDocument::from_html(html);
        let child = doc.get_element_by_id("child").unwrap();
        let parent = doc.get_parent(child);
        assert!(parent.is_some());
    }

    // ==================== Query Selector Tests ====================

    #[test]
    fn test_query_selector_by_id() {
        let html = "<html><body><div id=\"app\"><span id=\"inner\">test</span></div></body></html>";
        let doc = DomDocument::from_html(html);
        let result = doc.query_selector(0, "#inner");
        assert!(result.is_some());
        let node_id = result.unwrap();
        assert_eq!(doc.get_text_content(node_id), "test");
    }

    #[test]
    fn test_query_selector_by_class() {
        let html = "<html><body><div class=\"container\"><p class=\"item\">first</p><p class=\"item\">second</p></div></body></html>";
        let doc = DomDocument::from_html(html);
        let result = doc.query_selector(0, ".item");
        assert!(result.is_some());
        let node_id = result.unwrap();
        assert_eq!(doc.get_text_content(node_id), "first");
    }

    #[test]
    fn test_query_selector_by_tag() {
        let html = "<html><body><div><article><h1>Title</h1></article></div></body></html>";
        let doc = DomDocument::from_html(html);
        let result = doc.query_selector(0, "article");
        assert!(result.is_some());
    }

    #[test]
    fn test_query_selector_not_found() {
        let html = "<html><body><div>content</div></body></html>";
        let doc = DomDocument::from_html(html);
        let result = doc.query_selector(0, "#nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_query_selector_all() {
        let html = "<html><body><ul><li class=\"item\">1</li><li class=\"item\">2</li><li class=\"item\">3</li></ul></body></html>";
        let doc = DomDocument::from_html(html);
        let results = doc.query_selector_all(0, ".item");
        // Check that we find at least some items
        assert!(!results.is_empty());
    }

    #[test]
    fn test_query_selector_all_empty() {
        let html = "<html><body><div>no items</div></body></html>";
        let doc = DomDocument::from_html(html);
        let results = doc.query_selector_all(0, "#nonexistent");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_query_selector_from_element() {
        let html = "<html><body><div id=\"container\"><span class=\"inner\">a</span><span class=\"inner\">b</span></div><span class=\"inner\">c</span></body></html>";
        let doc = DomDocument::from_html(html);
        let container = doc.get_element_by_id("container").unwrap();
        let results = doc.query_selector_all(container, ".inner");
        // Should find items inside container
        assert!(!results.is_empty());
    }

    // ==================== DOM Manipulation Tests ====================

    #[test]
    fn test_insert_before_into_empty() {
        let html = "<html><body><div id=\"parent\"></div></body></html>";
        let mut doc = DomDocument::from_html(html);
        let parent = doc.get_element_by_id("parent").unwrap();
        let new_child = doc.create_element("span");
        doc.set_attribute(new_child, "id", "new");
        doc.insert_before(parent, new_child, None);
        let children = doc.get_children(parent);
        assert_eq!(children.len(), 1);
        assert_eq!(doc.get_node_id_attr(children[0]), "new");
    }

    #[test]
    fn test_insert_before_null_ref_appends() {
        let html = "<html><body><div id=\"parent\"></div></body></html>";
        let mut doc = DomDocument::from_html(html);
        let parent = doc.get_element_by_id("parent").unwrap();
        let child1 = doc.create_element("span");
        doc.set_attribute(child1, "id", "child1");
        doc.insert_before(parent, child1, None);
        let child2 = doc.create_element("span");
        doc.set_attribute(child2, "id", "child2");
        doc.insert_before(parent, child2, None);
        let children = doc.get_children(parent);
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_replace_child() {
        let html = "<html><body><div id=\"parent\"><span id=\"old\">old</span></div></body></html>";
        let mut doc = DomDocument::from_html(html);
        let parent = doc.get_element_by_id("parent").unwrap();
        let old_child = doc.get_element_by_id("old").unwrap();
        let new_child = doc.create_element("span");
        doc.set_attribute(new_child, "id", "new");
        doc.set_text_content(new_child, "new");
        doc.replace_child(parent, new_child, old_child);
        assert!(doc.get_element_by_id("old").is_none());
        assert!(doc.get_element_by_id("new").is_some());
    }

    #[test]
    fn test_clone_node_shallow() {
        let html = "<html><body><div id=\"original\" class=\"test\"><span>child</span></div></body></html>";
        let mut doc = DomDocument::from_html(html);
        let original = doc.get_element_by_id("original").unwrap();
        let clone = doc.clone_node(original, false);
        // Should have same attributes (including id, which is standard DOM behavior)
        assert_eq!(doc.get_attribute(clone, "id"), Some("original".to_string()));
        assert_eq!(doc.get_attribute(clone, "class"), Some("test".to_string()));
        // Should not have children in shallow clone
        assert_eq!(doc.get_children(clone).len(), 0);
    }

    #[test]
    fn test_clone_node_deep() {
        let html = "<html><body><div id=\"original\"><span>child1</span><span>child2</span></div></body></html>";
        let mut doc = DomDocument::from_html(html);
        let original = doc.get_element_by_id("original").unwrap();
        let clone = doc.clone_node(original, true);
        // Should have children in deep clone
        assert!(doc.get_children(clone).len() >= 2);
    }

    // ==================== Utility Methods Tests ====================

    #[test]
    fn test_contains() {
        let html = "<html><body><div id=\"outer\"><div id=\"inner\"><span id=\"deep\">text</span></div></div></body></html>";
        let doc = DomDocument::from_html(html);
        let outer = doc.get_element_by_id("outer").unwrap();
        let inner = doc.get_element_by_id("inner").unwrap();
        let deep = doc.get_element_by_id("deep").unwrap();
        let body = doc.body();
        assert!(doc.contains(outer, inner));
        assert!(doc.contains(outer, deep));
        assert!(doc.contains(body, deep));
        assert!(!doc.contains(inner, outer));
        assert!(!doc.contains(deep, outer));
    }

    #[test]
    fn test_has_child_nodes() {
        let html = "<html><body><div id=\"empty\"></div><div id=\"with-children\"><span>child</span></div></body></html>";
        let doc = DomDocument::from_html(html);
        let empty = doc.get_element_by_id("empty").unwrap();
        let with_children = doc.get_element_by_id("with-children").unwrap();
        assert!(!doc.has_child_nodes(empty));
        assert!(doc.has_child_nodes(with_children));
    }

    #[test]
    fn test_has_attributes() {
        let html = "<html><body><div id=\"with-attr\" class=\"test\"></div><div id=\"without-attr\"></div></body></html>";
        let doc = DomDocument::from_html(html);
        let with_attr = doc.get_element_by_id("with-attr").unwrap();
        let without_attr = doc.get_element_by_id("without-attr").unwrap();
        assert!(doc.has_attributes(with_attr));
        // Note: id is also an attribute
        assert!(doc.has_attributes(without_attr));
    }

    #[test]
    fn test_get_previous_sibling() {
        let html = "<html><body><div id=\"first\"></div><div id=\"second\"></div><div id=\"third\"></div></body></html>";
        let doc = DomDocument::from_html(html);
        let second = doc.get_element_by_id("second").unwrap();
        let third = doc.get_element_by_id("third").unwrap();
        let prev_of_second = doc.get_previous_sibling(second).unwrap();
        let prev_of_third = doc.get_previous_sibling(third).unwrap();
        assert_eq!(doc.get_node_id_attr(prev_of_second), "first");
        assert_eq!(doc.get_node_id_attr(prev_of_third), "second");
    }

    #[test]
    fn test_get_previous_sibling_none() {
        let html = "<html><body><div id=\"only\"></div></body></html>";
        let doc = DomDocument::from_html(html);
        let only = doc.get_element_by_id("only").unwrap();
        assert!(doc.get_previous_sibling(only).is_none());
    }

    #[test]
    fn test_get_node_type() {
        let html = "<html><body><div id=\"elem\">text</div></body></html>";
        let mut doc = DomDocument::from_html(html);
        let elem = doc.get_element_by_id("elem").unwrap();
        let text = doc.create_text_node("hello");
        let frag = doc.create_document_fragment();
        assert_eq!(doc.get_node_type(elem), 1); // ELEMENT_NODE
        assert_eq!(doc.get_node_type(text), 3); // TEXT_NODE
        assert_eq!(doc.get_node_type(frag), 11); // DOCUMENT_FRAGMENT_NODE
    }

    #[test]
    fn test_get_node_name() {
        let html = "<html><body><div id=\"elem\">text</div></body></html>";
        let mut doc = DomDocument::from_html(html);
        let elem = doc.get_element_by_id("elem").unwrap();
        let text = doc.create_text_node("hello");
        let frag = doc.create_document_fragment();
        assert_eq!(doc.get_node_name(elem), "DIV");
        assert_eq!(doc.get_node_name(text), "#text");
        assert_eq!(doc.get_node_name(frag), "#document-fragment");
    }

    #[test]
    fn test_get_attribute_names() {
        let html =
            "<html><body><div id=\"test\" class=\"foo\" data-value=\"bar\"></div></body></html>";
        let doc = DomDocument::from_html(html);
        let elem = doc.get_element_by_id("test").unwrap();
        let names = doc.get_attribute_names(elem);
        assert!(names.contains(&"id".to_string()));
        assert!(names.contains(&"class".to_string()));
        assert!(names.contains(&"data-value".to_string()));
        assert_eq!(names.len(), 3);
    }

    // ==================== Style Tests ====================

    #[test]
    fn test_set_style() {
        let html = "<html><body><div id=\"styled\"></div></body></html>";
        let mut doc = DomDocument::from_html(html);
        let elem = doc.get_element_by_id("styled").unwrap();
        doc.set_style(elem, "color", "red");
        doc.set_style(elem, "font-size", "14px");
        let style = doc.get_attribute(elem, "style").unwrap();
        assert!(style.contains("color"));
        assert!(style.contains("red"));
        assert!(style.contains("font-size"));
        assert!(style.contains("14px"));
    }

    #[test]
    fn test_set_style_override() {
        let html = "<html><body><div id=\"styled\" style=\"color: blue\"></div></body></html>";
        let mut doc = DomDocument::from_html(html);
        let elem = doc.get_element_by_id("styled").unwrap();
        doc.set_style(elem, "color", "red");
        let style = doc.get_attribute(elem, "style").unwrap();
        assert!(style.contains("color: red"));
        assert!(!style.contains("blue"));
    }

    // ==================== New Feature Tests ====================

    #[test]
    fn test_comment_node_type() {
        let html = "<html><body><!-- a comment --><div>text</div></body></html>";
        let doc = DomDocument::from_html(html);
        let body = doc.body();
        let children = doc.get_children(body);
        let comment_id = children[0];
        assert_eq!(doc.get_node_type(comment_id), 8);
        assert_eq!(doc.get_node_name(comment_id), "#comment");
    }

    #[test]
    fn test_comment_serialization() {
        let html = "<html><body><!-- hello --><div>text</div></body></html>";
        let doc = DomDocument::from_html(html);
        let output = doc.to_html();
        assert!(output.contains("<!-- hello -->"));
    }

    #[test]
    fn test_outer_html() {
        let html = "<html><body><div id=\"outer\"><span>inner</span></div></body></html>";
        let doc = DomDocument::from_html(html);
        let outer = doc.get_element_by_id("outer").unwrap();
        let outer_html = doc.get_outer_html(outer);
        assert!(outer_html.starts_with("<div id=\"outer\">"));
        assert!(outer_html.contains("<span>inner</span>"));
        assert!(outer_html.ends_with("</div>"));
    }

    #[test]
    fn test_outer_html_void_element() {
        let html = "<html><body><br></body></html>";
        let doc = DomDocument::from_html(html);
        let results = doc.query_selector_all(0, "br");
        assert!(!results.is_empty());
        let outer_html = doc.get_outer_html(results[0]);
        assert_eq!(outer_html, "<br>");
    }

    #[test]
    fn test_get_elements_by_tag_name() {
        let html = "<html><body><div><span>a</span><span>b</span><p>c</p></div></body></html>";
        let doc = DomDocument::from_html(html);
        let div = doc.query_selector(0, "div").unwrap();
        let spans = doc.get_elements_by_tag_name(div, "span");
        assert_eq!(spans.len(), 2);
        let ps = doc.get_elements_by_tag_name(div, "p");
        assert_eq!(ps.len(), 1);
    }

    #[test]
    fn test_get_elements_by_tag_name_star() {
        let html = "<html><body><div><span>a</span><p>b</p></div></body></html>";
        let doc = DomDocument::from_html(html);
        let div = doc.query_selector(0, "div").unwrap();
        let all = doc.get_elements_by_tag_name(div, "*");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_get_elements_by_class_name() {
        let html = "<html><body><div><span class=\"foo\">a</span><span class=\"foo bar\">b</span><p class=\"bar\">c</p></div></body></html>";
        let doc = DomDocument::from_html(html);
        let div = doc.query_selector(0, "div").unwrap();
        let foos = doc.get_elements_by_class_name(div, "foo");
        assert_eq!(foos.len(), 2);
        let bars = doc.get_elements_by_class_name(div, "bar");
        assert_eq!(bars.len(), 2);
        let both = doc.get_elements_by_class_name(div, "foo bar");
        assert_eq!(both.len(), 1);
    }

    #[test]
    fn test_first_element_child() {
        let html = "<html><body><div id=\"parent\">text<span id=\"first\">a</span><span id=\"second\">b</span></div></body></html>";
        let doc = DomDocument::from_html(html);
        let parent = doc.get_element_by_id("parent").unwrap();
        let first = doc.first_element_child(parent).unwrap();
        assert_eq!(doc.get_node_id_attr(first), "first");
    }

    #[test]
    fn test_last_element_child() {
        let html = "<html><body><div id=\"parent\"><span id=\"first\">a</span><span id=\"second\">b</span>text</div></body></html>";
        let doc = DomDocument::from_html(html);
        let parent = doc.get_element_by_id("parent").unwrap();
        let last = doc.last_element_child(parent).unwrap();
        assert_eq!(doc.get_node_id_attr(last), "second");
    }

    #[test]
    fn test_next_element_sibling() {
        let html = "<html><body><span id=\"a\">a</span>text<span id=\"b\">b</span></body></html>";
        let doc = DomDocument::from_html(html);
        let a = doc.get_element_by_id("a").unwrap();
        let b = doc.next_element_sibling(a).unwrap();
        assert_eq!(doc.get_node_id_attr(b), "b");
    }

    #[test]
    fn test_previous_element_sibling() {
        let html = "<html><body><span id=\"a\">a</span>text<span id=\"b\">b</span></body></html>";
        let doc = DomDocument::from_html(html);
        let b = doc.get_element_by_id("b").unwrap();
        let a = doc.previous_element_sibling(b).unwrap();
        assert_eq!(doc.get_node_id_attr(a), "a");
    }

    #[test]
    fn test_get_title() {
        let html = "<html><head><title>My Page</title></head><body></body></html>";
        let doc = DomDocument::from_html(html);
        assert_eq!(doc.get_title(), "My Page");
    }

    #[test]
    fn test_set_title() {
        let html = "<html><head><title>Old Title</title></head><body></body></html>";
        let mut doc = DomDocument::from_html(html);
        let old = doc.set_title("New Title");
        assert_eq!(old, "Old Title");
        assert_eq!(doc.get_title(), "New Title");
    }

    #[test]
    fn test_set_title_creates_element() {
        let html = "<html><head></head><body></body></html>";
        let mut doc = DomDocument::from_html(html);
        doc.set_title("Inserted Title");
        assert_eq!(doc.get_title(), "Inserted Title");
        let title_el = doc.query_selector(doc.head(), "title");
        assert!(title_el.is_some());
    }

    #[test]
    fn test_contains_self() {
        let html = "<html><body><div id=\"elem\"></div></body></html>";
        let doc = DomDocument::from_html(html);
        let elem = doc.get_element_by_id("elem").unwrap();
        assert!(doc.contains(elem, elem));
    }

    #[test]
    fn test_id_index_fast_path() {
        let html = "<html><body><div id=\"target\">found</div></body></html>";
        let doc = DomDocument::from_html(html);
        let found = doc.get_element_by_id("target");
        assert!(found.is_some());
        assert_eq!(doc.get_text_content(found.unwrap()), "found");
    }

    #[test]
    fn test_set_attribute_updates_id_index() {
        let html = "<html><body><div id=\"old\">content</div></body></html>";
        let mut doc = DomDocument::from_html(html);
        let div = doc.get_element_by_id("old").unwrap();
        doc.set_attribute(div, "id", "new");
        assert!(doc.get_element_by_id("old").is_none());
        assert!(doc.get_element_by_id("new").is_some());
    }

    #[test]
    fn test_remove_attribute_updates_id_index() {
        let html = "<html><body><div id=\"removable\">content</div></body></html>";
        let mut doc = DomDocument::from_html(html);
        let div = doc.get_element_by_id("removable").unwrap();
        doc.remove_attribute(div, "id");
        assert!(doc.get_element_by_id("removable").is_none());
    }

    #[test]
    fn test_set_class_name_updates_index() {
        let html = "<html><body><div>content</div></body></html>";
        let mut doc = DomDocument::from_html(html);
        let div = doc.query_selector(0, "div").unwrap();
        doc.set_class_name(div, "my-class");
        let results = doc.get_elements_by_class_name(doc.body(), "my-class");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], div);
    }

    #[test]
    fn test_whitespace_preservation_in_build() {
        let html = "<html><body><div>  spaces  </div></body></html>";
        let doc = DomDocument::from_html(html);
        let output = doc.to_html();
        assert!(output.contains("  spaces  "));
    }

    #[test]
    fn test_original_html_released() {
        let html = "<html><body><p>original</p></body></html>";
        let doc = DomDocument::from_html(html);
        // original_html is freed after DOM construction to save memory
        assert!(doc.original_html.is_none());
    }
}
