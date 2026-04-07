//! Tests for RSS/Atom feed detection and semantic tree extraction.

use pardus_core::feed::is_feed_content;

// ---------------------------------------------------------------------------
// is_feed_content — content-type detection
// ---------------------------------------------------------------------------

#[test]
fn detect_rss_content_type() {
    assert!(is_feed_content(b"<rss/>", Some("application/rss+xml")));
}

#[test]
fn detect_atom_content_type() {
    assert!(is_feed_content(b"<feed/>", Some("application/atom+xml")));
}

#[test]
fn detect_json_feed_content_type() {
    assert!(is_feed_content(b"{}", Some("application/feed+json")));
}

#[test]
fn reject_html_content_type() {
    assert!(!is_feed_content(b"<html/>", Some("text/html")));
}

#[test]
fn reject_no_content_type_without_rss_body() {
    assert!(!is_feed_content(b"<html><body>Hello</body></html>", None));
}

// ---------------------------------------------------------------------------
// is_feed_content — body-based detection
// ---------------------------------------------------------------------------

#[test]
fn detect_rss_body() {
    let rss = br#"<rss version="2.0"><channel><title>Test</title></channel></rss>"#;
    assert!(is_feed_content(rss, None));
}

#[test]
fn detect_rdf_body() {
    let rdf = br#"<rdf:RDF xmlns="http://purl.org/rss/1.0/"><channel><title>T</title></channel></rdf:RDF>"#;
    assert!(is_feed_content(rdf, None));
}

#[test]
fn detect_atom_body() {
    let atom = br#"<feed xmlns="http://www.w3.org/2005/Atom"><title>T</title></feed>"#;
    assert!(is_feed_content(atom, None));
}

#[test]
fn reject_html_body() {
    assert!(!is_feed_content(b"<html><body>Not a feed</body></html>", None));
}

#[test]
fn reject_empty_body() {
    assert!(!is_feed_content(b"", None));
}

#[test]
fn detect_feed_in_first_kb() {
    let mut body = b"<rss version=\"2.0\">".to_vec();
    body.extend_from_slice(&vec![b' '; 1200]); // push past 1KB
    body.extend_from_slice(b"</rss>");
    assert!(is_feed_content(&body, None));
}

// ---------------------------------------------------------------------------
// is_feed_content — edge cases
// ---------------------------------------------------------------------------

#[test]
fn content_type_case_insensitive() {
    assert!(is_feed_content(b"", Some("Application/RSS+XML")));
    assert!(is_feed_content(b"", Some("APPLICATION/ATOM+XML")));
}

#[test]
fn content_type_with_charset() {
    assert!(is_feed_content(b"", Some("application/rss+xml; charset=utf-8")));
}

#[test]
fn reject_plain_xml() {
    assert!(!is_feed_content(b"<?xml version=\"1.0\"?><data/>", None));
}

#[test]
fn reject_binary_garbage() {
    assert!(!is_feed_content(&[0xFF, 0xFE, 0x00, 0x01], None));
}
