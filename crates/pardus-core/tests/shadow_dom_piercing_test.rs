// Tests for Shadow DOM piercing

#[test]
fn test_basic() {
    assert!(true);
}

#[test]
fn test_get_shadow_root_returns_none() {
    use pardus_core::js::dom::DomDocument;
    let html = "<html><body><div id=\"regular\"></div></body></html>";
    let doc = DomDocument::from_html(html);
    let regular = doc.get_element_by_id("regular").unwrap();
    assert!(doc.get_shadow_root(regular).is_none());
}

#[test]
fn test_is_shadow_host_returns_false() {
    use pardus_core::js::dom::DomDocument;
    let html = "<html><body><div id=\"regular\"></div></body></html>";
    let doc = DomDocument::from_html(html);
    let regular = doc.get_element_by_id("regular").unwrap();
    assert!(!doc.is_shadow_host(regular));
}

#[test]
fn test_collect_all_elements_deep() {
    use pardus_core::js::dom::DomDocument;
    let html = "<html><body><div><span></span></div></body></html>";
    let doc = DomDocument::from_html(html);
    let body = doc.body();
    let elements = doc.collect_all_elements_deep(body);
    assert!(elements.len() >= 1);
}

#[test]
fn test_query_selector_deep() {
    use pardus_core::js::dom::DomDocument;
    let html = "<html><body><div id=\"target\"></div></body></html>";
    let doc = DomDocument::from_html(html);
    let found = doc.query_selector_deep(0, "#target");
    assert!(found.is_some());
}

#[test]
fn test_query_selector_all_deep() {
    use pardus_core::js::dom::DomDocument;
    let html = "<html><body><button></button><button></button></body></html>";
    let doc = DomDocument::from_html(html);
    let found = doc.query_selector_all_deep(0, "button");
    assert_eq!(found.len(), 2);
}

#[test]
fn test_empty_document() {
    use pardus_core::js::dom::DomDocument;
    let html = "<html><body></body></html>";
    let doc = DomDocument::from_html(html);
    let body = doc.body();
    let elements = doc.collect_all_elements_deep(body);
    let _ = elements.len();
}
