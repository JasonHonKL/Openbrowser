//! Integration tests for the Pardus CDP interact handler.
//!
//! Tests the full click/type/submit flow through `PardusDomain::handle()`,
//! verifying that actions correctly update the target store and return
//! appropriate JSON responses.

use std::{collections::HashMap, sync::Arc};

use pardus_cdp::{
    domain::{
        CdpDomainHandler, DomainContext, HandleResult, TargetEntry, pardus_ext::PardusDomain,
    },
    protocol::{event_bus::EventBus, node_map::NodeMap, target::CdpSession},
};
use pardus_core::{App, BrowserConfig, UrlPolicy};
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a DomainContext with a permissive URL policy (allows localhost for mockito tests).
fn setup_ctx_with_html(
    html: &str,
    url: &str,
) -> (
    Arc<Mutex<HashMap<String, TargetEntry>>>,
    Arc<EventBus>,
    DomainContext,
) {
    let mut config = BrowserConfig::default();
    config.url_policy = UrlPolicy::permissive();
    let app = Arc::new(App::new(config).unwrap());
    let targets = Arc::new(Mutex::new(HashMap::new()));
    let event_bus = Arc::new(EventBus::new(1024));
    let node_map = Arc::new(Mutex::new(NodeMap::new()));

    // Pre-populate target synchronously (won't block — no runtime yet)
    let entry = TargetEntry {
        url: url.to_string(),
        html: Some(html.to_string()),
        title: None,
        js_enabled: false,
        frame_tree_json: None,
        form_state: HashMap::new(),
    };
    // We'll insert in the test body since we need async.

    let ctx = DomainContext::new(app, targets.clone(), event_bus.clone(), node_map);
    (targets, event_bus, ctx)
}

/// Create a CdpSession attached to the given target_id.
fn make_session(target_id: &str) -> CdpSession {
    let mut session = CdpSession::new("test-session".to_string());
    session.target_id = Some(target_id.to_string());
    session
}

/// Insert a target entry into the targets map (async).
async fn insert_target(
    targets: &Arc<Mutex<HashMap<String, TargetEntry>>>,
    target_id: &str,
    html: &str,
    url: &str,
) {
    let mut map = targets.lock().await;
    map.insert(
        target_id.to_string(),
        TargetEntry {
            url: url.to_string(),
            html: Some(html.to_string()),
            title: None,
            js_enabled: false,
            frame_tree_json: None,
            form_state: HashMap::new(),
        },
    );
}

/// Send an interact command through the domain handler and return the JSON result.
async fn interact(
    ctx: &DomainContext,
    target_id: &str,
    action: &str,
    selector: &str,
) -> serde_json::Value {
    let domain = PardusDomain;
    let mut session = make_session(target_id);

    let params = serde_json::json!({
        "action": action,
        "selector": selector,
    });

    let result = domain.handle("interact", params, &mut session, ctx).await;
    match result {
        HandleResult::Success(v) => v,
        HandleResult::Error(e) => {
            serde_json::json!({ "error": e.error.message, "cdp_error": true })
        }
        HandleResult::Ack => serde_json::json!({ "ack": true }),
    }
}

/// Send an interact command with a value parameter.
async fn interact_with_value(
    ctx: &DomainContext,
    target_id: &str,
    action: &str,
    selector: &str,
    value: &str,
) -> serde_json::Value {
    let domain = PardusDomain;
    let mut session = make_session(target_id);

    let params = serde_json::json!({
        "action": action,
        "selector": selector,
        "value": value,
    });

    let result = domain.handle("interact", params, &mut session, ctx).await;
    match result {
        HandleResult::Success(v) => v,
        HandleResult::Error(e) => {
            serde_json::json!({ "error": e.error.message, "cdp_error": true })
        }
        HandleResult::Ack => serde_json::json!({ "ack": true }),
    }
}

/// Send an interact command with fields.
async fn interact_with_fields(
    ctx: &DomainContext,
    target_id: &str,
    action: &str,
    selector: &str,
    fields: serde_json::Value,
) -> serde_json::Value {
    let domain = PardusDomain;
    let mut session = make_session(target_id);

    let params = serde_json::json!({
        "action": action,
        "selector": selector,
        "fields": fields,
    });

    let result = domain.handle("interact", params, &mut session, ctx).await;
    match result {
        HandleResult::Success(v) => v,
        HandleResult::Error(e) => {
            serde_json::json!({ "error": e.error.message, "cdp_error": true })
        }
        HandleResult::Ack => serde_json::json!({ "ack": true }),
    }
}

// ---------------------------------------------------------------------------
// Tests: error cases (no HTTP needed)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_interact_no_active_page() {
    let (_targets, _eb, ctx) = setup_ctx_with_html("", "");
    // Don't insert any target

    let result = interact(&ctx, "missing-target", "click", "#1").await;
    assert_eq!(result["success"], false);
    assert_eq!(result["error"], "No active page");
}

#[tokio::test]
async fn test_click_element_not_found() {
    let (targets, _eb, ctx) = setup_ctx_with_html(
        "<html><body><p>No interactive elements</p></body></html>",
        "https://example.com",
    );
    insert_target(
        &targets,
        "t1",
        "<html><body><p>Nothing</p></body></html>",
        "https://example.com",
    )
    .await;

    let result = interact(&ctx, "t1", "click", "#999").await;
    assert_eq!(result["success"], false);
    assert!(result["error"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_type_element_not_found() {
    let (targets, _eb, ctx) = setup_ctx_with_html("", "");
    insert_target(
        &targets,
        "t1",
        "<html><body></body></html>",
        "https://example.com",
    )
    .await;

    let result = interact_with_value(&ctx, "t1", "type", "#999", "hello").await;
    assert_eq!(result["success"], false);
    assert!(result["error"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_interact_unknown_action() {
    let (targets, _eb, ctx) = setup_ctx_with_html("", "");
    insert_target(
        &targets,
        "t1",
        "<html><body></body></html>",
        "https://example.com",
    )
    .await;

    let result = interact(&ctx, "t1", "teleport", "#1").await;
    assert_eq!(result["success"], false);
    assert!(result["error"].as_str().unwrap().contains("Unknown action"));
}

// ---------------------------------------------------------------------------
// Tests: type action — form state accumulation (no HTTP needed)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_type_stores_value_in_form_state() {
    let (targets, _eb, ctx) = setup_ctx_with_html("", "");
    insert_target(
        &targets,
        "t1",
        r#"<html><body>
            <form>
                <input type="text" name="username">
                <input type="password" name="password">
            </form>
        </body></html>"#,
        "https://example.com/login",
    )
    .await;

    let result =
        interact_with_value(&ctx, "t1", "type", r#"input[name="username"]"#, "alice").await;
    assert_eq!(result["success"], true);
    assert_eq!(result["action"], "type");
    assert_eq!(result["value"], "alice");

    // Verify form_state was persisted
    let map = targets.lock().await;
    let entry = map.get("t1").unwrap();
    assert_eq!(entry.form_state.get("username"), Some(&"alice".to_string()));
}

#[tokio::test]
async fn test_type_accumulates_multiple_fields() {
    let (targets, _eb, ctx) = setup_ctx_with_html("", "");
    insert_target(
        &targets,
        "t1",
        r#"<html><body>
            <form>
                <input type="text" name="username">
                <input type="password" name="password">
            </form>
        </body></html>"#,
        "https://example.com/login",
    )
    .await;

    interact_with_value(&ctx, "t1", "type", r#"input[name="username"]"#, "alice").await;
    interact_with_value(&ctx, "t1", "type", r#"input[name="password"]"#, "s3cret").await;

    let map = targets.lock().await;
    let entry = map.get("t1").unwrap();
    assert_eq!(entry.form_state.get("username"), Some(&"alice".to_string()));
    assert_eq!(
        entry.form_state.get("password"),
        Some(&"s3cret".to_string())
    );
}

// ---------------------------------------------------------------------------
// Tests: type + click button flow (with mockito)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_click_link_navigates() {
    let mut server = mockito::Server::new_async().await;

    // Mock the target page that the link points to
    let target_html = r#"<html><head><title>Target Page</title></head>
        <body><h1>Welcome to About</h1></body></html>"#;
    let mock = server
        .mock("GET", "/about")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(target_html)
        .create_async()
        .await;

    let base_url = server.url();

    // Source page with a link to /about
    let source_html = format!(
        r#"<html><body>
            <a href="{base}/about">About Us</a>
        </body></html>"#,
        base = base_url,
    );

    let (targets, _eb, ctx) = setup_ctx_with_html("", "");
    insert_target(&targets, "t1", &source_html, &base_url).await;

    // Click the link (element #1 = the <a> tag)
    let result = interact(&ctx, "t1", "click", "#1").await;
    assert_eq!(result["success"], true);
    assert_eq!(result["action"], "click");
    assert_eq!(result["navigated"], true);

    // Verify the target store was updated with the new page
    let map = targets.lock().await;
    let entry = map.get("t1").unwrap();
    assert!(entry.html.as_ref().unwrap().contains("Welcome to About"));
    assert!(entry.url.contains("/about"));

    // Form state should be cleared after navigation
    assert!(entry.form_state.is_empty());

    mock.assert_async().await;
}

#[tokio::test]
async fn test_click_link_resolves_relative_url() {
    let mut server = mockito::Server::new_async().await;

    let target_html = r#"<html><body><h1>Contact Us</h1></body></html>"#;
    let mock = server
        .mock("GET", "/contact")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(target_html)
        .create_async()
        .await;

    let base_url = server.url();

    // Source page with a relative link
    let source_html = r#"<html><body>
        <a href="/contact">Contact</a>
    </body></html>"#;

    let (targets, _eb, ctx) = setup_ctx_with_html("", "");
    insert_target(&targets, "t1", source_html, &format!("{}/page", base_url)).await;

    let result = interact(&ctx, "t1", "click", "#1").await;
    assert_eq!(result["success"], true);
    assert_eq!(result["navigated"], true);

    let map = targets.lock().await;
    let entry = map.get("t1").unwrap();
    assert!(entry.html.as_ref().unwrap().contains("Contact Us"));

    mock.assert_async().await;
}

#[tokio::test]
async fn test_click_button_submits_form() {
    let mut server = mockito::Server::new_async().await;

    // Mock the form submission endpoint
    let response_html = r#"<html><body><h1>Login Successful</h1></body></html>"#;
    let mock = server
        .mock("POST", "/login")
        .match_header("content-type", "application/x-www-form-urlencoded")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(response_html)
        .create_async()
        .await;

    let base_url = server.url();

    let source_html = format!(
        r#"<html><body>
            <form action="{base}/login" method="POST">
                <input type="text" name="username" value="">
                <input type="hidden" name="csrf" value="token123">
                <button type="submit" name="submit">Login</button>
            </form>
        </body></html>"#,
        base = base_url,
    );

    let (targets, _eb, ctx) = setup_ctx_with_html("", "");
    insert_target(&targets, "t1", &source_html, &base_url).await;

    // Click the submit button (should be element #3 after the two inputs)
    let result = interact(&ctx, "t1", "click", r#"button[name="submit"]"#).await;
    assert_eq!(result["success"], true);
    assert_eq!(result["action"], "click");

    // Verify target was updated with the response page
    let map = targets.lock().await;
    let entry = map.get("t1").unwrap();
    assert!(entry.html.as_ref().unwrap().contains("Login Successful"));

    mock.assert_async().await;
}

#[tokio::test]
async fn test_submit_with_fields() {
    let mut server = mockito::Server::new_async().await;

    let response_html = r#"<html><body><h1>Search Results</h1></body></html>"#;
    let mock = server
        .mock("GET", "/search")
        .match_query(mockito::Matcher::AllOf(vec![mockito::Matcher::UrlEncoded(
            "q".to_string(),
            "rust lang".to_string(),
        )]))
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(response_html)
        .create_async()
        .await;

    let base_url = server.url();

    let source_html = format!(
        r#"<html><body>
            <form id="search-form" action="{base}/search" method="GET">
                <input type="text" name="q">
            </form>
        </body></html>"#,
        base = base_url,
    );

    let (targets, _eb, ctx) = setup_ctx_with_html("", "");
    insert_target(&targets, "t1", &source_html, &base_url).await;

    let result = interact_with_fields(
        &ctx,
        "t1",
        "submit",
        "#search-form",
        serde_json::json!({ "q": "rust lang" }),
    )
    .await;

    assert_eq!(result["success"], true);
    assert_eq!(result["action"], "submit");

    let map = targets.lock().await;
    let entry = map.get("t1").unwrap();
    assert!(entry.html.as_ref().unwrap().contains("Search Results"));

    mock.assert_async().await;
}

#[tokio::test]
async fn test_type_then_click_submits_with_typed_values() {
    let mut server = mockito::Server::new_async().await;

    let response_html = r#"<html><body><h1>Welcome alice</h1></body></html>"#;
    let mock = server
        .mock("POST", "/login")
        .match_header("content-type", "application/x-www-form-urlencoded")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(response_html)
        .create_async()
        .await;

    let base_url = server.url();

    let source_html = format!(
        r#"<html><body>
            <form action="{base}/login" method="POST">
                <input type="text" name="username" value="">
                <input type="password" name="password" value="">
                <button type="submit">Login</button>
            </form>
        </body></html>"#,
        base = base_url,
    );

    let (targets, _eb, ctx) = setup_ctx_with_html("", "");
    insert_target(&targets, "t1", &source_html, &base_url).await;

    // Type into fields
    interact_with_value(&ctx, "t1", "type", r#"input[name="username"]"#, "alice").await;
    interact_with_value(&ctx, "t1", "type", r#"input[name="password"]"#, "s3cret").await;

    // Verify form_state accumulated both values
    {
        let map = targets.lock().await;
        let entry = map.get("t1").unwrap();
        assert_eq!(entry.form_state.get("username"), Some(&"alice".to_string()));
        assert_eq!(
            entry.form_state.get("password"),
            Some(&"s3cret".to_string())
        );
    }

    // Click the submit button — should include typed values in submission
    let result = interact(&ctx, "t1", "click", "button").await;
    assert_eq!(result["success"], true);
    assert_eq!(result["navigated"], true);

    // Verify target updated with response
    let map = targets.lock().await;
    let entry = map.get("t1").unwrap();
    assert!(entry.html.as_ref().unwrap().contains("Welcome alice"));

    // Form state cleared after navigation
    assert!(entry.form_state.is_empty());

    mock.assert_async().await;
}

// ---------------------------------------------------------------------------
// Tests: toggle and select (no HTTP needed)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_toggle_checkbox() {
    let (targets, _eb, ctx) = setup_ctx_with_html("", "");
    insert_target(
        &targets,
        "t1",
        r#"<html><body>
            <form>
                <input type="checkbox" name="agree" value="yes">
            </form>
        </body></html>"#,
        "https://example.com",
    )
    .await;

    let result = interact(&ctx, "t1", "toggle", r#"input[name="agree"]"#).await;
    assert_eq!(result["success"], true);
    assert_eq!(result["action"], "toggle");
    assert_eq!(result["checked"], true);

    // Verify form_state records the checked value
    let map = targets.lock().await;
    let entry = map.get("t1").unwrap();
    assert_eq!(entry.form_state.get("agree"), Some(&"yes".to_string()));
}

#[tokio::test]
async fn test_select_option() {
    let (targets, _eb, ctx) = setup_ctx_with_html("", "");
    insert_target(
        &targets,
        "t1",
        r#"<html><body>
            <form>
                <select name="country">
                    <option value="us">US</option>
                    <option value="uk">UK</option>
                    <option value="de">DE</option>
                </select>
            </form>
        </body></html>"#,
        "https://example.com",
    )
    .await;

    let result = interact_with_value(&ctx, "t1", "select", "select", "uk").await;
    assert_eq!(result["success"], true);
    assert_eq!(result["action"], "select");
    assert_eq!(result["value"], "uk");

    // Verify form_state records the selection
    let map = targets.lock().await;
    let entry = map.get("t1").unwrap();
    assert_eq!(entry.form_state.get("country"), Some(&"uk".to_string()));
}
