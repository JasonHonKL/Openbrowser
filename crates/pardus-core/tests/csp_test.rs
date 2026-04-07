//! Tests for Content Security Policy parsing and evaluation.

use pardus_core::csp::{
    CspPolicySet, CspDirectiveKind, CspSource,
    parser::CspPolicy,
    source::{SourceMatchContext, compute_hash},
    directive::HashAlgorithm,
    eval::CspEvaluator,
};

// ---------------------------------------------------------------------------
// CspPolicy parsing
// ---------------------------------------------------------------------------

#[test]
fn parse_simple_script_src() {
    let policy = CspPolicy::parse("script-src 'self' https://cdn.example.com");
    assert!(policy.has_directive(CspDirectiveKind::ScriptSrc));
    let sources = policy.effective_sources(CspDirectiveKind::ScriptSrc);
    assert!(!sources.is_empty());
}

#[test]
fn parse_default_src() {
    let policy = CspPolicy::parse("default-src 'none'");
    assert!(policy.has_directive(CspDirectiveKind::DefaultSrc));
}

#[test]
fn parse_img_src() {
    let policy = CspPolicy::parse("img-src data: https:");
    assert!(policy.has_directive(CspDirectiveKind::ImgSrc));
}

#[test]
fn parse_multiple_directives() {
    let policy = CspPolicy::parse(
        "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'",
    );
    assert!(policy.has_directive(CspDirectiveKind::DefaultSrc));
    assert!(policy.has_directive(CspDirectiveKind::ScriptSrc));
    assert!(policy.has_directive(CspDirectiveKind::StyleSrc));
}

#[test]
fn parse_empty_policy() {
    let policy = CspPolicy::parse("");
    assert!(policy.is_empty());
}

#[test]
fn parse_unknown_directive_ignored() {
    let policy = CspPolicy::parse("unknown-directive 'self'");
    assert!(!policy.has_directive(CspDirectiveKind::DefaultSrc));
}

#[test]
fn policy_raw_preserves_input() {
    let raw = "default-src 'self'; script-src 'unsafe-eval'";
    let policy = CspPolicy::parse(raw);
    assert_eq!(policy.raw(), raw);
}

// ---------------------------------------------------------------------------
// CspPolicySet from headers
// ---------------------------------------------------------------------------

#[test]
fn policy_set_from_csp_header() {
    let headers = vec![
        ("content-security-policy".into(), "default-src 'self'".into()),
    ];
    let set = CspPolicySet::from_headers(&headers);
    assert!(!set.is_empty());
}

#[test]
fn policy_set_empty_when_no_headers() {
    let headers: Vec<(String, String)> = vec![];
    let set = CspPolicySet::from_headers(&headers);
    assert!(set.is_empty());
}

#[test]
fn policy_set_from_report_only_header() {
    let headers = vec![
        ("content-security-policy-report-only".into(), "default-src 'self'".into()),
    ];
    let set = CspPolicySet::from_headers(&headers);
    assert!(!set.is_empty());
}

#[test]
fn upgrade_insecure_detected() {
    let headers = vec![
        ("content-security-policy".into(), "upgrade-insecure-requests".into()),
    ];
    let set = CspPolicySet::from_headers(&headers);
    assert!(set.should_upgrade_insecure());
}

#[test]
fn no_upgrade_insecure_without_directive() {
    let headers = vec![
        ("content-security-policy".into(), "default-src 'self'".into()),
    ];
    let set = CspPolicySet::from_headers(&headers);
    assert!(!set.should_upgrade_insecure());
}

// ---------------------------------------------------------------------------
// CspSource parsing
// ---------------------------------------------------------------------------

#[test]
fn source_self_keyword() {
    let src = CspSource::parse("'self'");
    assert!(src.is_some());
}

#[test]
fn source_none_keyword() {
    let src = CspSource::parse("'none'");
    assert!(src.is_some());
}

#[test]
fn source_unsafe_inline() {
    let src = CspSource::parse("'unsafe-inline'");
    assert!(src.is_some());
}

#[test]
fn source_unsafe_eval() {
    let src = CspSource::parse("'unsafe-eval'");
    assert!(src.is_some());
}

#[test]
fn source_scheme() {
    let src = CspSource::parse("https:");
    assert!(src.is_some());
}

#[test]
fn source_data_scheme() {
    let src = CspSource::parse("data:");
    assert!(src.is_some());
}

#[test]
fn source_host() {
    let src = CspSource::parse("example.com");
    assert!(src.is_some());
}

#[test]
fn source_host_with_path() {
    let src = CspSource::parse("example.com/path");
    assert!(src.is_some());
}

#[test]
fn source_wildcard_host() {
    let src = CspSource::parse("*.example.com");
    assert!(src.is_some());
}

#[test]
fn source_empty_returns_none() {
    let src = CspSource::parse("");
    assert!(src.is_none());
}

// ---------------------------------------------------------------------------
// Hash computation
// ---------------------------------------------------------------------------

#[test]
fn sha256_hash() {
    let hash = compute_hash(&HashAlgorithm::Sha256, b"console.log(1)");
    assert!(hash.starts_with("sha256-"));
    assert!(hash.len() > 10);
}

#[test]
fn sha384_hash() {
    let hash = compute_hash(&HashAlgorithm::Sha384, b"console.log(1)");
    assert!(hash.starts_with("sha384-"));
}

#[test]
fn sha512_hash() {
    let hash = compute_hash(&HashAlgorithm::Sha512, b"console.log(1)");
    assert!(hash.starts_with("sha512-"));
}

#[test]
fn hash_deterministic() {
    let h1 = compute_hash(&HashAlgorithm::Sha256, b"test");
    let h2 = compute_hash(&HashAlgorithm::Sha256, b"test");
    assert_eq!(h1, h2);
}

#[test]
fn hash_different_content() {
    let h1 = compute_hash(&HashAlgorithm::Sha256, b"foo");
    let h2 = compute_hash(&HashAlgorithm::Sha256, b"bar");
    assert_ne!(h1, h2);
}

// ---------------------------------------------------------------------------
// CspDirectiveKind fallbacks
// ---------------------------------------------------------------------------

#[test]
fn script_src_fallback_to_default() {
    assert_eq!(CspDirectiveKind::ScriptSrc.fallback(), Some(CspDirectiveKind::DefaultSrc));
}

#[test]
fn style_src_fallback_to_default() {
    assert_eq!(CspDirectiveKind::StyleSrc.fallback(), Some(CspDirectiveKind::DefaultSrc));
}

#[test]
fn img_src_fallback_to_default() {
    assert_eq!(CspDirectiveKind::ImgSrc.fallback(), Some(CspDirectiveKind::DefaultSrc));
}

#[test]
fn default_src_no_fallback() {
    assert_eq!(CspDirectiveKind::DefaultSrc.fallback(), None);
}

// ---------------------------------------------------------------------------
// CspDirectiveKind from_name
// ---------------------------------------------------------------------------

#[test]
fn directive_from_name() {
    assert_eq!(CspDirectiveKind::from_name("script-src"), Some(CspDirectiveKind::ScriptSrc));
    assert_eq!(CspDirectiveKind::from_name("default-src"), Some(CspDirectiveKind::DefaultSrc));
    assert_eq!(CspDirectiveKind::from_name("img-src"), Some(CspDirectiveKind::ImgSrc));
    assert_eq!(CspDirectiveKind::from_name("style-src"), Some(CspDirectiveKind::StyleSrc));
    assert_eq!(CspDirectiveKind::from_name("connect-src"), Some(CspDirectiveKind::ConnectSrc));
    assert_eq!(CspDirectiveKind::from_name("font-src"), Some(CspDirectiveKind::FontSrc));
    assert_eq!(CspDirectiveKind::from_name("media-src"), Some(CspDirectiveKind::MediaSrc));
    assert_eq!(CspDirectiveKind::from_name("frame-src"), Some(CspDirectiveKind::FrameSrc));
    assert_eq!(CspDirectiveKind::from_name("object-src"), Some(CspDirectiveKind::ObjectSrc));
    assert_eq!(CspDirectiveKind::from_name("form-action"), Some(CspDirectiveKind::FormAction));
    assert_eq!(CspDirectiveKind::from_name("base-uri"), Some(CspDirectiveKind::BaseUri));
    assert_eq!(CspDirectiveKind::from_name("navigate-to"), Some(CspDirectiveKind::NavigateTo));
}

#[test]
fn directive_from_unknown_name() {
    assert_eq!(CspDirectiveKind::from_name("unknown-src"), None);
    assert_eq!(CspDirectiveKind::from_name(""), None);
}

#[test]
fn directive_name_roundtrip() {
    for name in &[
        "default-src", "script-src", "style-src", "img-src", "connect-src",
        "font-src", "media-src", "frame-src", "object-src", "form-action",
        "base-uri", "navigate-to",
    ] {
        let kind = CspDirectiveKind::from_name(name).unwrap();
        assert_eq!(kind.name(), *name);
    }
}
