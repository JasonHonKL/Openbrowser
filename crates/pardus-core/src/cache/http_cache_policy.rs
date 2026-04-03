//! HTTP cache compliance per RFC 7234.
//!
//! Parses response headers to determine freshness lifetime, validators,
//! and builds conditional request headers (If-None-Match, If-Modified-Since).

use chrono::{DateTime, Utc};
use rquest::header::HeaderMap;
use std::time::{Duration, Instant};

const HEURISTIC_FRACTION: f64 = 0.1;

#[derive(Debug, Clone)]
pub struct CachePolicy {
    pub max_age: Option<Duration>,
    pub no_store: bool,
    pub no_cache: bool,
    pub must_revalidate: bool,
    pub immutable: bool,
    pub expires: Option<DateTime<Utc>>,
    pub date: Option<DateTime<Utc>>,
    pub age: Duration,
    pub etag: Option<String>,
    pub last_modified: Option<DateTime<Utc>>,
    pub has_validator: bool,
}

impl CachePolicy {
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let mut max_age = None;
        let mut no_store = false;
        let mut no_cache = false;
        let mut must_revalidate = false;
        let mut immutable = false;

        if let Some(cc) = headers.get("cache-control").and_then(|v| v.to_str().ok()) {
            for directive in cc.split(',') {
                let d = directive.trim();
                if d.eq_ignore_ascii_case("no-store") {
                    no_store = true;
                } else if d.eq_ignore_ascii_case("no-cache") {
                    no_cache = true;
                } else if d.eq_ignore_ascii_case("must-revalidate") {
                    must_revalidate = true;
                } else if d.eq_ignore_ascii_case("immutable") {
                    immutable = true;
                } else if let Some(secs) = d
                    .strip_prefix("max-age=")
                    .and_then(|s| s.trim().parse::<u64>().ok())
                {
                    max_age = Some(Duration::from_secs(secs));
                } else if let Some(secs) = d
                    .strip_prefix("s-maxage=")
                    .and_then(|s| s.trim().parse::<u64>().ok())
                {
                    max_age = Some(Duration::from_secs(secs));
                }
            }
        }

        let etag = headers
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let last_modified =
            parse_http_date(headers.get("last-modified").and_then(|v| v.to_str().ok()));
        let expires = parse_http_date(headers.get("expires").and_then(|v| v.to_str().ok()));
        let date = parse_http_date(headers.get("date").and_then(|v| v.to_str().ok()));

        let age = headers
            .get("age")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or(Duration::ZERO);

        let has_validator = etag.is_some() || last_modified.is_some();

        Self {
            max_age,
            no_store,
            no_cache,
            must_revalidate,
            immutable,
            expires,
            date,
            age,
            etag,
            last_modified,
            has_validator,
        }
    }

    pub fn can_cache(&self) -> bool {
        !self.no_store
    }

    pub fn heuristic_freshness(&self) -> Option<Duration> {
        let lm = self.last_modified?;
        let now = self.date.unwrap_or_else(Utc::now);
        let delta = now.signed_duration_since(lm);
        if delta.num_seconds() <= 0 {
            return None;
        }
        let secs = (delta.num_seconds() as f64 * HEURISTIC_FRACTION) as u64;
        Some(Duration::from_secs(secs.max(1).min(86400)))
    }

    pub fn freshness_lifetime(&self) -> Duration {
        if self.immutable {
            return Duration::from_secs(365 * 24 * 3600);
        }
        if let Some(ma) = self.max_age {
            return ma.saturating_sub(self.age);
        }
        if let Some(expires) = self.expires {
            if let Some(date) = self.date {
                let lifetime = expires.signed_duration_since(date);
                if lifetime.num_seconds() > 0 {
                    return self.age + Duration::from_secs(lifetime.num_seconds() as u64);
                }
            }
        }
        self.heuristic_freshness()
            .unwrap_or(Duration::from_secs(60))
    }

    pub fn is_fresh(&self, stored_at: Instant) -> bool {
        if self.no_cache {
            return false;
        }
        if self.immutable {
            return true;
        }
        let elapsed = stored_at.elapsed();
        elapsed < self.freshness_lifetime()
    }

    pub fn needs_validation(&self, stored_at: Instant) -> bool {
        if self.no_cache || self.must_revalidate {
            return true;
        }
        !self.is_fresh(stored_at)
    }

    pub fn conditional_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(ref etag) = self.etag {
            if let Ok(val) = etag.parse() {
                headers.insert("if-none-match", val);
            }
        } else if let Some(lm) = self.last_modified {
            let s = lm.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
            if let Ok(val) = s.parse() {
                headers.insert("if-modified-since", val);
            }
        }
        headers
    }

    pub fn update_from_304(&mut self, headers: &HeaderMap) {
        if let Some(etag) = headers.get("etag").and_then(|v| v.to_str().ok()) {
            self.etag = Some(etag.to_string());
            self.has_validator = true;
        }
        if let Some(lm) =
            parse_http_date(headers.get("last-modified").and_then(|v| v.to_str().ok()))
        {
            self.last_modified = Some(lm);
            self.has_validator = true;
        }
        if let Some(expires) = parse_http_date(headers.get("expires").and_then(|v| v.to_str().ok()))
        {
            self.expires = Some(expires);
        }
        if let Some(cc) = headers.get("cache-control").and_then(|v| v.to_str().ok()) {
            for directive in cc.split(',') {
                let d = directive.trim();
                if d.eq_ignore_ascii_case("no-cache") {
                    self.no_cache = true;
                } else if d.eq_ignore_ascii_case("must-revalidate") {
                    self.must_revalidate = true;
                } else if d.eq_ignore_ascii_case("immutable") {
                    self.immutable = true;
                } else if let Some(secs) = d
                    .strip_prefix("max-age=")
                    .and_then(|s| s.trim().parse::<u64>().ok())
                {
                    self.max_age = Some(Duration::from_secs(secs));
                }
            }
        }
        if let Some(date) = parse_http_date(headers.get("date").and_then(|v| v.to_str().ok())) {
            self.date = Some(date);
        }
    }
}

fn parse_http_date(s: Option<&str>) -> Option<DateTime<Utc>> {
    let s = s?;
    // Try IMF-fixdate first, then common alternatives
    DateTime::parse_from_rfc2822(s)
        .or_else(|_| DateTime::parse_from_str(s, "%a, %d %b %Y %H:%M:%S GMT"))
        .or_else(|_| DateTime::parse_from_str(s, "%A, %d-%b-%y %H:%M:%S GMT"))
        .or_else(|_| DateTime::parse_from_str(s, "%d %b %Y %H:%M:%S GMT"))
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_map(pairs: &[(&str, &str)]) -> HeaderMap {
        use rquest::header::HeaderName;
        let mut m = HeaderMap::new();
        for (k, v) in pairs {
            let name = HeaderName::from_bytes(k.as_bytes()).expect("invalid header name");
            m.insert(name, v.parse().unwrap());
        }
        m
    }

    #[test]
    fn test_no_store() {
        let h = header_map(&[("cache-control", "no-store")]);
        let p = CachePolicy::from_headers(&h);
        assert!(p.no_store);
        assert!(!p.can_cache());
    }

    #[test]
    fn test_max_age_fresh() {
        let h = header_map(&[("cache-control", "max-age=3600")]);
        let p = CachePolicy::from_headers(&h);
        assert!(p.can_cache());
        assert!(p.is_fresh(Instant::now()));
    }

    #[test]
    fn test_max_age_stale() {
        let h = header_map(&[("cache-control", "max-age=1")]);
        let p = CachePolicy::from_headers(&h);
        let stored = Instant::now() - Duration::from_secs(2);
        assert!(!p.is_fresh(stored));
        assert!(p.needs_validation(stored));
    }

    #[test]
    fn test_no_cache_needs_validation() {
        let h = header_map(&[("cache-control", "no-cache")]);
        let p = CachePolicy::from_headers(&h);
        assert!(p.needs_validation(Instant::now()));
    }

    #[test]
    fn test_etag_conditional_headers() {
        let h = header_map(&[("etag", "\"abc123\"")]);
        let p = CachePolicy::from_headers(&h);
        let ch = p.conditional_headers();
        assert_eq!(ch.get("if-none-match").unwrap(), "\"abc123\"");
    }

    #[test]
    fn test_last_modified_conditional_headers() {
        let h = header_map(&[("last-modified", "Wed, 21 Oct 2015 07:28:00 GMT")]);
        let p = CachePolicy::from_headers(&h);
        let ch = p.conditional_headers();
        assert!(ch.get("if-modified-since").is_some());
        assert!(ch.get("if-none-match").is_none());
    }

    #[test]
    fn test_etag_preferred_over_last_modified() {
        let h = header_map(&[
            ("etag", "\"abc\""),
            ("last-modified", "Wed, 21 Oct 2015 07:28:00 GMT"),
        ]);
        let p = CachePolicy::from_headers(&h);
        let ch = p.conditional_headers();
        assert_eq!(ch.get("if-none-match").unwrap(), "\"abc\"");
        assert!(ch.get("if-modified-since").is_none());
    }

    #[test]
    fn test_has_validator() {
        let h1 = header_map(&[("etag", "\"x\"")]);
        assert!(CachePolicy::from_headers(&h1).has_validator);

        let h2 = header_map(&[("last-modified", "Wed, 21 Oct 2015 07:28:00 GMT")]);
        assert!(CachePolicy::from_headers(&h2).has_validator);

        let h3 = header_map(&[("cache-control", "max-age=60")]);
        assert!(!CachePolicy::from_headers(&h3).has_validator);
    }

    #[test]
    fn test_heuristic_freshness() {
        let h = header_map(&[
            ("last-modified", "Mon, 01 Jan 2024 00:00:00 GMT"),
            ("date", "Mon, 01 Jul 2024 00:00:00 GMT"),
        ]);
        let p = CachePolicy::from_headers(&h);
        // ~183 days * 0.1 = ~18.3 days in seconds
        let hf = p.heuristic_freshness().unwrap();
        assert!(hf.as_secs() > 60);
        assert!(hf.as_secs() < 86400 * 30);
    }

    #[test]
    fn test_update_from_304() {
        let h = header_map(&[("cache-control", "max-age=60")]);
        let mut p = CachePolicy::from_headers(&h);
        let update = header_map(&[("etag", "\"new\""), ("cache-control", "max-age=600")]);
        p.update_from_304(&update);
        assert_eq!(p.etag.as_deref(), Some("\"new\""));
        assert_eq!(p.max_age, Some(Duration::from_secs(600)));
        assert!(p.has_validator);
    }

    #[test]
    fn test_immutable() {
        let h = header_map(&[("cache-control", "immutable")]);
        let p = CachePolicy::from_headers(&h);
        assert!(p.is_fresh(Instant::now() - Duration::from_secs(365 * 24 * 3600)));
    }

    #[test]
    fn test_age_header() {
        let h = header_map(&[("cache-control", "max-age=300"), ("age", "200")]);
        let p = CachePolicy::from_headers(&h);
        // freshness = max-age(300) - age(200) = 100s
        assert!(p.is_fresh(Instant::now()));
        let stored = Instant::now() - Duration::from_secs(101);
        assert!(!p.is_fresh(stored));
    }
}
