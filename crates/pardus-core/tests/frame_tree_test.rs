use pardus_core::frame::{FrameData, FrameId, FrameTree};
use pardus_core::Page;
use scraper::Html;

fn sample_html_with_iframe() -> &'static str {
    r#"<!DOCTYPE html>
<html><head><title>Main Page</title></head>
<body>
    <h1>Hello World</h1>
    <iframe src="https://example.com/widget" id="widget-frame"></iframe>
    <div id="main-content">Main content here</div>
</body></html>"#
}

#[test]
fn test_frame_id_serialization_transparent() {
    let id = FrameId("0.1.3".to_string());
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "\"0.1.3\"");

    let parsed: FrameId = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.as_str(), "0.1.3");
}

#[test]
fn test_frame_tree_serialization_roundtrip() {
    let html = Html::parse_document(sample_html_with_iframe());
    let tree = FrameTree::empty(html, "https://example.com");

    let json = serde_json::to_string(&tree).unwrap();
    let deserialized: FrameTree = serde_json::from_str(&json).unwrap();

    assert_eq!(tree.root.url, deserialized.root.url);
    assert_eq!(tree.root.id, deserialized.root.id);
}

#[test]
fn test_frame_tree_with_children_serialization() {
    let tree = FrameTree {
        root: FrameData {
            id: FrameId::root(),
            url: "https://example.com".to_string(),
            html: Some("<html><body>Main</body></html>".to_string()),
            srcdoc: None,
            sandbox: None,
            sandbox_tokens: Vec::new(),
            parent_id: None,
            child_frames: vec![
                FrameData {
                    id: FrameId("0.0".to_string()),
                    url: "https://example.com/child1".to_string(),
                    html: Some("<html><body>Child 1</body></html>".to_string()),
                    srcdoc: None,
                    sandbox: Some("allow-scripts".to_string()),
                    sandbox_tokens: vec!["allow-scripts".to_string()],
                    parent_id: Some(FrameId::root()),
                    child_frames: Vec::new(),
                    load_error: None,
                },
                FrameData {
                    id: FrameId("0.1".to_string()),
                    url: "about:srcdoc".to_string(),
                    html: Some("<p>Inline</p>".to_string()),
                    srcdoc: Some("<p>Inline</p>".to_string()),
                    sandbox: None,
                    sandbox_tokens: Vec::new(),
                    parent_id: Some(FrameId::root()),
                    child_frames: Vec::new(),
                    load_error: None,
                },
            ],
            load_error: None,
        },
    };

    let json = serde_json::to_string_pretty(&tree).unwrap();
    assert!(json.contains("\"0.0\""));
    assert!(json.contains("\"0.1\""));
    assert!(json.contains("https://example.com/child1"));
    assert!(json.contains("about:srcdoc"));
    assert!(json.contains("allow-scripts"));

    let deserialized: FrameTree = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.frame_count(), 3);
    assert_eq!(deserialized.max_depth(), 1);

    let child0 = deserialized
        .find_frame(&FrameId("0.0".to_string()))
        .unwrap();
    assert_eq!(child0.url, "https://example.com/child1");
    assert!(!child0.scripts_blocked());

    let child1 = deserialized
        .find_frame(&FrameId("0.1".to_string()))
        .unwrap();
    assert!(child1.srcdoc.is_some());
}

#[test]
fn test_page_from_html_with_frame_tree() {
    let html_str = "<html><body>Test</body></html>";
    let tree = FrameTree {
        root: FrameData {
            id: FrameId::root(),
            url: "https://example.com".to_string(),
            html: Some("<html><body>Test</body></html>".to_string()),
            srcdoc: None,
            sandbox: None,
            sandbox_tokens: Vec::new(),
            parent_id: None,
            child_frames: Vec::new(),
            load_error: None,
        },
    };

    let page = Page::from_html_with_frame_tree(html_str, "https://example.com", tree);
    assert_eq!(page.url, "https://example.com");
    assert!(page.frame_tree().is_some());
    assert_eq!(page.frame_tree().unwrap().frame_count(), 1);
}

#[test]
fn test_page_semantic_tree_with_frames() {
    let html_str = sample_html_with_iframe();
    let tree = FrameTree {
        root: FrameData {
            id: FrameId::root(),
            url: "https://example.com".to_string(),
            html: Some(html_str.to_string()),
            srcdoc: None,
            sandbox: None,
            sandbox_tokens: Vec::new(),
            parent_id: None,
            child_frames: vec![FrameData {
                id: FrameId("0.0".to_string()),
                url: "https://example.com/widget".to_string(),
                html: Some("<html><body><button>Click me</button></body></html>".to_string()),
                srcdoc: None,
                sandbox: None,
                sandbox_tokens: Vec::new(),
                parent_id: Some(FrameId::root()),
                child_frames: Vec::new(),
                load_error: None,
            }],
            load_error: None,
        },
    };

    let page = Page::from_html_with_frame_tree(html_str, "https://example.com", tree);
    let semantic = page.semantic_tree();
    assert!(semantic.stats.iframes > 0);
}

#[test]
fn test_frame_data_with_load_error() {
    let frame = FrameData {
        id: FrameId("0.0".to_string()),
        url: "https://example.com/broken".to_string(),
        html: None,
        srcdoc: None,
        sandbox: None,
        sandbox_tokens: Vec::new(),
        parent_id: Some(FrameId::root()),
        child_frames: Vec::new(),
        load_error: Some("HTTP 404".to_string()),
    };

    let json = serde_json::to_string(&frame).unwrap();
    assert!(json.contains("HTTP 404"));

    let deserialized: FrameData = serde_json::from_str(&json).unwrap();
    assert!(deserialized.html.is_none());
    assert_eq!(deserialized.load_error.as_deref(), Some("HTTP 404"));
}

#[test]
fn test_all_frames_traversal() {
    let tree = FrameTree {
        root: FrameData {
            id: FrameId::root(),
            url: "https://example.com".to_string(),
            html: Some("root".to_string()),
            srcdoc: None,
            sandbox: None,
            sandbox_tokens: Vec::new(),
            parent_id: None,
            child_frames: vec![
                FrameData {
                    id: FrameId("0.0".to_string()),
                    url: "https://a.com".to_string(),
                    html: Some("a".to_string()),
                    srcdoc: None,
                    sandbox: None,
                    sandbox_tokens: Vec::new(),
                    parent_id: Some(FrameId::root()),
                    child_frames: vec![FrameData {
                        id: FrameId("0.0.0".to_string()),
                        url: "https://deep.com".to_string(),
                        html: Some("deep".to_string()),
                        srcdoc: None,
                        sandbox: None,
                        sandbox_tokens: Vec::new(),
                        parent_id: Some(FrameId("0.0".to_string())),
                        child_frames: Vec::new(),
                        load_error: None,
                    }],
                    load_error: None,
                },
                FrameData {
                    id: FrameId("0.1".to_string()),
                    url: "https://b.com".to_string(),
                    html: Some("b".to_string()),
                    srcdoc: None,
                    sandbox: None,
                    sandbox_tokens: Vec::new(),
                    parent_id: Some(FrameId::root()),
                    child_frames: Vec::new(),
                    load_error: None,
                },
            ],
            load_error: None,
        },
    };

    assert_eq!(tree.frame_count(), 4);
    assert_eq!(tree.max_depth(), 2);

    let all = tree.all_frames();
    assert_eq!(all.len(), 4);

    let urls: Vec<&str> = all.iter().map(|f| f.url.as_str()).collect();
    assert!(urls.contains(&"https://example.com"));
    assert!(urls.contains(&"https://a.com"));
    assert!(urls.contains(&"https://deep.com"));
    assert!(urls.contains(&"https://b.com"));

    assert!(tree.find_frame(&FrameId("0.0.0".to_string())).is_some());
    assert!(tree.find_frame(&FrameId("0.2".to_string())).is_none());
}
