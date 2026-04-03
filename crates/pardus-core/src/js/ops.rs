//! DOM operations for deno_core.
//!
//! These ops provide the bridge between JavaScript and our Rust DOM implementation.

use super::dom::DomDocument;
use deno_core::*;
use std::cell::RefCell;
use std::rc::Rc;

// ==================== Document Methods ====================

#[op2(fast)]
pub fn op_create_element(state: &mut OpState, #[string] tag: &str) -> u32 {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow_mut().create_element(tag)
}

#[op2(fast)]
pub fn op_create_text_node(state: &mut OpState, #[string] text: &str) -> u32 {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow_mut().create_text_node(text)
}

#[op2(fast)]
pub fn op_create_document_fragment(state: &mut OpState) -> u32 {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow_mut().create_document_fragment()
}

#[op2(fast)]
pub fn op_get_element_by_id(state: &mut OpState, #[string] id: &str) -> u32 {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().get_element_by_id(id).unwrap_or(0)
}

#[op2(fast)]
pub fn op_query_selector(state: &mut OpState, node_id: u32, #[string] selector: &str) -> u32 {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().query_selector(node_id, selector).unwrap_or(0)
}

#[op2]
#[serde]
pub fn op_query_selector_all(
    state: &mut OpState,
    node_id: u32,
    #[string] selector: &str,
) -> Vec<u32> {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().query_selector_all(node_id, selector)
}

#[op2(fast)]
pub fn op_get_document_element(state: &mut OpState) -> u32 {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().document_element()
}

#[op2(fast)]
pub fn op_get_head(state: &mut OpState) -> u32 {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().head()
}

#[op2(fast)]
pub fn op_get_body(state: &mut OpState) -> u32 {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().body()
}

// ==================== Node/Element Methods ====================

#[op2(fast)]
pub fn op_append_child(state: &mut OpState, parent_id: u32, child_id: u32) {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow_mut().append_child(parent_id, child_id);
}

#[op2(fast)]
pub fn op_remove_child(state: &mut OpState, parent_id: u32, child_id: u32) {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow_mut().remove_child(parent_id, child_id);
}

#[op2(fast)]
pub fn op_insert_before(state: &mut OpState, parent_id: u32, new_node_id: u32, ref_node_id: u32) {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    let ref_id = if ref_node_id == 0 {
        None
    } else {
        Some(ref_node_id)
    };
    dom.borrow_mut()
        .insert_before(parent_id, new_node_id, ref_id);
}

#[op2(fast)]
pub fn op_replace_child(state: &mut OpState, parent_id: u32, new_child_id: u32, old_child_id: u32) {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow_mut()
        .replace_child(parent_id, new_child_id, old_child_id);
}

#[op2(fast)]
pub fn op_clone_node(state: &mut OpState, node_id: u32, deep: bool) -> u32 {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow_mut().clone_node(node_id, deep)
}

// ==================== Attribute Methods ====================

#[op2(fast)]
pub fn op_set_attribute(
    state: &mut OpState,
    node_id: u32,
    #[string] name: &str,
    #[string] value: &str,
) {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow_mut().set_attribute(node_id, name, value);
}

#[op2]
#[string]
pub fn op_get_attribute(state: &mut OpState, node_id: u32, #[string] name: &str) -> Option<String> {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().get_attribute(node_id, name)
}

#[op2(fast)]
pub fn op_remove_attribute(state: &mut OpState, node_id: u32, #[string] name: &str) {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow_mut().remove_attribute(node_id, name);
}

// ==================== Property Getters ====================

#[op2]
#[string]
pub fn op_get_tag_name(state: &mut OpState, node_id: u32) -> Option<String> {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().get_tag_name(node_id)
}

#[op2]
#[string]
pub fn op_get_node_id_attr(state: &mut OpState, node_id: u32) -> String {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().get_node_id_attr(node_id)
}

#[op2(fast)]
pub fn op_set_node_id_attr(state: &mut OpState, node_id: u32, #[string] id: &str) {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow_mut().set_node_id_attr(node_id, id);
}

#[op2]
#[string]
pub fn op_get_class_name(state: &mut OpState, node_id: u32) -> String {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().get_class_name(node_id)
}

#[op2(fast)]
pub fn op_set_class_name(state: &mut OpState, node_id: u32, #[string] class_name: &str) {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow_mut().set_class_name(node_id, class_name);
}

#[op2]
#[string]
pub fn op_get_inner_html(state: &mut OpState, node_id: u32) -> String {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().get_inner_html(node_id)
}

#[op2(fast)]
pub fn op_set_inner_html(state: &mut OpState, node_id: u32, #[string] html: &str) {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow_mut().set_inner_html(node_id, html);
}

#[op2]
#[string]
pub fn op_get_text_content(state: &mut OpState, node_id: u32) -> String {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().get_text_content(node_id)
}

#[op2(fast)]
pub fn op_set_text_content(state: &mut OpState, node_id: u32, #[string] text: &str) {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow_mut().set_text_content(node_id, text);
}

#[op2(fast)]
pub fn op_get_parent(state: &mut OpState, node_id: u32) -> u32 {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().get_parent(node_id).unwrap_or(0)
}

#[op2]
#[serde]
pub fn op_get_children(state: &mut OpState, node_id: u32) -> Vec<u32> {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().get_children(node_id)
}

#[op2(fast)]
pub fn op_get_previous_sibling(state: &mut OpState, node_id: u32) -> u32 {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().get_previous_sibling(node_id).unwrap_or(0)
}

// ==================== Style ====================

#[op2(fast)]
pub fn op_set_style(
    state: &mut OpState,
    node_id: u32,
    #[string] property: &str,
    #[string] value: &str,
) {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow_mut().set_style(node_id, property, value);
}

// ==================== Utility Methods ====================

#[op2(fast)]
pub fn op_contains(state: &mut OpState, container_id: u32, contained_id: u32) -> bool {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().contains(container_id, contained_id)
}

#[op2(fast)]
pub fn op_has_child_nodes(state: &mut OpState, node_id: u32) -> bool {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().has_child_nodes(node_id)
}

#[op2(fast)]
pub fn op_has_attributes(state: &mut OpState, node_id: u32) -> bool {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().has_attributes(node_id)
}

#[op2(fast)]
pub fn op_get_node_type(state: &mut OpState, node_id: u32) -> u16 {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().get_node_type(node_id)
}

#[op2]
#[string]
pub fn op_get_node_name(state: &mut OpState, node_id: u32) -> String {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().get_node_name(node_id)
}

#[op2]
#[serde]
pub fn op_get_attribute_names(state: &mut OpState, node_id: u32) -> Vec<String> {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().get_attribute_names(node_id)
}

// ==================== Timer Ops ====================

#[op2]
pub fn op_set_timeout(
    state: &mut OpState,
    #[string] callback_str: Option<String>,
    #[smi] delay_ms: u32,
) -> u32 {
    let queue = state.borrow_mut::<super::timer::TimerQueue>();
    queue.set_timeout(callback_str, delay_ms.into())
}

#[op2]
pub fn op_set_interval(
    state: &mut OpState,
    #[string] callback_str: Option<String>,
    #[smi] delay_ms: u32,
) -> u32 {
    let queue = state.borrow_mut::<super::timer::TimerQueue>();
    queue.set_interval(callback_str, delay_ms.into())
}

#[op2(fast)]
pub fn op_clear_timer(state: &mut OpState, id: u32) {
    let queue = state.borrow_mut::<super::timer::TimerQueue>();
    queue.clear_timer(id);
}

// ==================== MutationObserver Ops ====================

#[op2]
pub fn op_register_observer(
    state: &mut OpState,
    #[smi] target_node_id: u32,
    child_list: bool,
    attributes: bool,
    subtree: bool,
    character_data: bool,
    attribute_old_value: bool,
    character_data_old_value: bool,
    #[serde] attribute_filter: Vec<String>,
) -> u32 {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    let mut init = super::dom::MutationObserverInit::default();
    init.child_list = child_list;
    init.attributes = attributes;
    init.subtree = subtree;
    init.character_data = character_data;
    init.attribute_old_value = attribute_old_value;
    init.character_data_old_value = character_data_old_value;
    init.attribute_filter = attribute_filter;
    dom.borrow_mut().register_observer(target_node_id, init)
}

#[op2(fast)]
pub fn op_disconnect_observer(state: &mut OpState, observer_id: u32) {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow_mut().disconnect_observer(observer_id);
}

#[op2]
#[serde]
pub fn op_take_mutation_records(state: &mut OpState) -> Vec<super::dom::MutationRecord> {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow_mut().take_mutation_records()
}

#[op2(fast)]
pub fn op_has_observers(state: &mut OpState) -> bool {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow().has_observers()
}

#[op2]
#[serde]
pub fn op_drain_pending_mutations(
    state: &mut OpState,
) -> Vec<(u32, Vec<super::dom::MutationRecord>)> {
    let dom = state.borrow::<Rc<RefCell<DomDocument>>>().clone();
    dom.borrow_mut().drain_all_pending_mutations()
}
