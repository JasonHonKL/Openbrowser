//! Pattern matching rules for interceptors.

use pardus_debug::ResourceType;

use crate::intercept::RequestContext;

/// A pattern-matching rule used by built-in interceptors.
pub enum InterceptorRule {
    /// Glob pattern matched against the full URL (e.g. `*/images/*`).
    UrlGlob(String, regex::Regex),
    /// Regex matched against the full URL.
    UrlRegex(regex::Regex),
    /// Exact or wildcard domain match (e.g. `example.com`, `*.example.com`).
    Domain(String),
    /// Match by resource type(s).
    ResourceType(Vec<ResourceType>),
    /// URL path prefix match (e.g. `/api/`).
    PathPrefix(String),
    /// Custom predicate function.
    Custom(Box<dyn Fn(&RequestContext) -> bool + Send + Sync>),
}

impl std::fmt::Debug for InterceptorRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UrlGlob(p, _) => f.debug_tuple("UrlGlob").field(p).finish(),
            Self::UrlRegex(re) => f.debug_tuple("UrlRegex").field(&re.to_string()).finish(),
            Self::Domain(d) => f.debug_tuple("Domain").field(d).finish(),
            Self::ResourceType(types) => f.debug_tuple("ResourceType").field(types).finish(),
            Self::PathPrefix(p) => f.debug_tuple("PathPrefix").field(p).finish(),
            Self::Custom(_) => f.debug_tuple("Custom").field(&"<predicate>").finish(),
        }
    }
}

impl InterceptorRule {
    /// Create a URL glob rule from a glob pattern.
    pub fn url_glob(pattern: impl Into<String>) -> Self {
        let p = pattern.into();
        let regex_str = glob_to_regex(&p);
        let re = regex::Regex::new(&regex_str)
            .unwrap_or_else(|_| regex::Regex::new(&regex::escape(&p)).unwrap());
        Self::UrlGlob(p, re)
    }

    /// Check if this rule matches the given request context.
    pub fn matches(&self, ctx: &RequestContext) -> bool {
        match self {
            Self::UrlGlob(_, re) => re.is_match(&ctx.url),
            Self::UrlRegex(re) => re.is_match(&ctx.url),
            Self::Domain(domain) => domain_matches(domain, &ctx.url),
            Self::ResourceType(types) => types.contains(&ctx.resource_type),
            Self::PathPrefix(prefix) => {
                let path = url_path(&ctx.url);
                path.starts_with(prefix)
            }
            Self::Custom(pred) => pred(ctx),
        }
    }
}

/// Convert a glob pattern to a regex and test the URL.
fn glob_matches(pattern: &str, url: &str) -> bool {
    let regex_str = glob_to_regex(pattern);
    match regex::Regex::new(&regex_str) {
        Ok(re) => re.is_match(url),
        Err(_) => url.contains(pattern),
    }
}

/// Simple glob-to-regex conversion.
/// Supports `*` (any chars) and `?` (single char).
fn glob_to_regex(pattern: &str) -> String {
    let mut result = String::with_capacity(pattern.len() * 2);
    result.push('^');
    for ch in pattern.chars() {
        match ch {
            '*' => result.push_str(".*"),
            '?' => result.push('.'),
            '.' | '^' | '$' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '\\' | '|' => {
                result.push('\\');
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }
    result.push('$');
    result
}

/// Check if a URL's host matches a domain pattern.
/// Supports exact match and `*.domain.com` wildcard.
fn domain_matches(domain: &str, url: &str) -> bool {
    let host = match url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
    {
        Some(h) => h,
        None => return false,
    };

    let domain_lower = domain.to_lowercase();

    if domain_lower.starts_with("*.") {
        // Wildcard: *.example.com matches sub.example.com and example.com
        let suffix = &domain_lower[2..];
        host == suffix || host.ends_with(&format!(".{}", suffix))
    } else {
        host == domain_lower
    }
}

/// Extract the path portion of a URL.
fn url_path(url: &str) -> String {
    url::Url::parse(url)
        .map(|u| u.path().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use pardus_debug::Initiator;

    use super::*;

    fn ctx_with_url(url: &str) -> RequestContext {
        RequestContext {
            url: url.to_string(),
            method: "GET".to_string(),
            headers: HashMap::new(),
            body: None,
            resource_type: ResourceType::Document,
            initiator: Initiator::Navigation,
            is_navigation: true,
        }
    }

    fn ctx_with_resource_type(rt: ResourceType) -> RequestContext {
        RequestContext {
            url: "https://example.com/resource".to_string(),
            method: "GET".to_string(),
            headers: HashMap::new(),
            body: None,
            resource_type: rt,
            initiator: Initiator::Navigation,
            is_navigation: false,
        }
    }

    // --- UrlGlob tests ---

    #[test]
    fn test_glob_star_matches_any() {
        let rule = InterceptorRule::url_glob("*example.com*");
        assert!(rule.matches(&ctx_with_url("https://example.com/page")));
        assert!(rule.matches(&ctx_with_url("https://sub.example.com/page")));
        assert!(!rule.matches(&ctx_with_url("https://other.com/page")));
    }

    #[test]
    fn test_glob_path_pattern() {
        let rule = InterceptorRule::url_glob("*/api/*");
        assert!(rule.matches(&ctx_with_url("https://example.com/api/users")));
        assert!(!rule.matches(&ctx_with_url("https://example.com/page")));
    }

    #[test]
    fn test_glob_extension() {
        let rule = InterceptorRule::url_glob("*.css");
        assert!(rule.matches(&ctx_with_url("https://example.com/styles/main.css")));
        assert!(!rule.matches(&ctx_with_url("https://example.com/styles/main.js")));
    }

    // --- UrlRegex tests ---

    #[test]
    fn test_regex_match() {
        let rule =
            InterceptorRule::UrlRegex(regex::Regex::new(r"https://.*\.example\.com/.*").unwrap());
        assert!(rule.matches(&ctx_with_url("https://api.example.com/v1/data")));
        assert!(!rule.matches(&ctx_with_url("https://example.com/page")));
    }

    // --- Domain tests ---

    #[test]
    fn test_domain_exact() {
        let rule = InterceptorRule::Domain("example.com".to_string());
        assert!(rule.matches(&ctx_with_url("https://example.com/page")));
        assert!(!rule.matches(&ctx_with_url("https://other.com/page")));
    }

    #[test]
    fn test_domain_wildcard() {
        let rule = InterceptorRule::Domain("*.example.com".to_string());
        assert!(rule.matches(&ctx_with_url("https://sub.example.com/page")));
        assert!(rule.matches(&ctx_with_url("https://deep.sub.example.com/page")));
        assert!(rule.matches(&ctx_with_url("https://example.com/page"))); // apex matches too
        assert!(!rule.matches(&ctx_with_url("https://other.com/page")));
    }

    #[test]
    fn test_domain_case_insensitive() {
        let rule = InterceptorRule::Domain("Example.COM".to_string());
        assert!(rule.matches(&ctx_with_url("https://example.com/page")));
    }

    // --- ResourceType tests ---

    #[test]
    fn test_resource_type_match() {
        let rule =
            InterceptorRule::ResourceType(vec![ResourceType::Stylesheet, ResourceType::Image]);
        assert!(rule.matches(&ctx_with_resource_type(ResourceType::Stylesheet)));
        assert!(rule.matches(&ctx_with_resource_type(ResourceType::Image)));
        assert!(!rule.matches(&ctx_with_resource_type(ResourceType::Document)));
    }

    // --- PathPrefix tests ---

    #[test]
    fn test_path_prefix() {
        let rule = InterceptorRule::PathPrefix("/api/".to_string());
        assert!(rule.matches(&ctx_with_url("https://example.com/api/users")));
        assert!(!rule.matches(&ctx_with_url("https://example.com/page")));
    }
}
