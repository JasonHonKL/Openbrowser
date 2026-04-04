//! Tests for RedirectChain, RedirectHop, and PageSnapshot.

use pardus_core::{RedirectChain, RedirectHop};

// ---------------------------------------------------------------------------
// RedirectHop
// ---------------------------------------------------------------------------

#[test]
fn test_redirect_hop_fields() {
    let hop = RedirectHop {
        from: "https://a.com".to_string(),
        to: "https://b.com".to_string(),
        status: 301,
    };
    assert_eq!(hop.from, "https://a.com");
    assert_eq!(hop.to, "https://b.com");
    assert_eq!(hop.status, 301);
}

// ---------------------------------------------------------------------------
// RedirectChain
// ---------------------------------------------------------------------------

#[test]
fn test_empty_chain() {
    let chain = RedirectChain::default();
    assert!(chain.is_empty());
    assert!(chain.hops.is_empty());
    assert!(chain.original_url().is_none());
}

#[test]
fn test_single_hop() {
    let chain = RedirectChain {
        hops: vec![RedirectHop {
            from: "https://a.com".to_string(),
            to: "https://b.com".to_string(),
            status: 301,
        }],
    };
    assert!(!chain.is_empty());
    assert_eq!(chain.original_url(), Some("https://a.com"));
    assert_eq!(chain.hops.len(), 1);
}

#[test]
fn test_multi_hop_chain() {
    let chain = RedirectChain {
        hops: vec![
            RedirectHop {
                from: "https://a.com".to_string(),
                to: "https://b.com".to_string(),
                status: 301,
            },
            RedirectHop {
                from: "https://b.com".to_string(),
                to: "https://c.com".to_string(),
                status: 302,
            },
            RedirectHop {
                from: "https://c.com".to_string(),
                to: "https://d.com".to_string(),
                status: 301,
            },
        ],
    };
    assert!(!chain.is_empty());
    assert_eq!(chain.original_url(), Some("https://a.com"));
    assert_eq!(chain.hops.last().unwrap().to, "https://d.com");
    assert_eq!(chain.hops.len(), 3);
}

#[test]
fn test_chain_serialization() {
    let chain = RedirectChain {
        hops: vec![RedirectHop {
            from: "https://example.com".to_string(),
            to: "https://example.com/new".to_string(),
            status: 301,
        }],
    };
    let json = serde_json::to_string(&chain).expect("should serialize");
    assert!(json.contains("example.com"));
    assert!(json.contains("301"));

    let deserialized: RedirectChain =
        serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(deserialized.hops.len(), 1);
    assert_eq!(deserialized.hops[0].status, 301);
}
