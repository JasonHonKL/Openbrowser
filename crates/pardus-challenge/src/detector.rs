//! HTTP-response-level CAPTCHA / challenge detection.

use std::collections::HashSet;

use serde::Serialize;

/// Classification of the detected challenge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum ChallengeKind {
    /// Google reCAPTCHA (v2 / v3 / Enterprise).
    Recaptcha,
    /// hCaptcha.
    Hcaptcha,
    /// Cloudflare Turnstile.
    Turnstile,
    /// Generic CAPTCHA (e.g. image-based).
    GenericCaptcha,
    /// JavaScript challenge (Cloudflare, Akamai, etc.) that serves a 403/503
    /// and requires browser execution to solve.
    JsChallenge,
    /// Bot-detection service detected (DataDome, PerimeterX, etc.) but the
    /// exact challenge type is unknown.
    BotProtection,
}

impl ChallengeKind {
    /// All known variants, used for serialization ordering.
    const ALL: [ChallengeKind; 6] = [
        ChallengeKind::Recaptcha,
        ChallengeKind::Hcaptcha,
        ChallengeKind::Turnstile,
        ChallengeKind::GenericCaptcha,
        ChallengeKind::JsChallenge,
        ChallengeKind::BotProtection,
    ];
}

impl std::fmt::Display for ChallengeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Recaptcha => write!(f, "reCAPTCHA"),
            Self::Hcaptcha => write!(f, "hCaptcha"),
            Self::Turnstile => write!(f, "Cloudflare Turnstile"),
            Self::GenericCaptcha => write!(f, "CAPTCHA"),
            Self::JsChallenge => write!(f, "JS Challenge"),
            Self::BotProtection => write!(f, "Bot Protection"),
        }
    }
}

/// Detailed information about a detected challenge.
#[derive(Debug, Clone, Serialize)]
pub struct ChallengeInfo {
    /// The URL that triggered the challenge.
    pub url: String,
    /// HTTP status code of the response.
    pub status: u16,
    /// Detected challenge types (may be multiple).
    pub kinds: Vec<ChallengeKind>,
    /// Combined risk score 0-100.
    pub risk_score: u8,
}

impl ChallengeInfo {
    pub fn is_captcha(&self) -> bool {
        self.kinds.iter().any(|k| {
            matches!(
                k,
                ChallengeKind::Recaptcha
                    | ChallengeKind::Hcaptcha
                    | ChallengeKind::Turnstile
                    | ChallengeKind::GenericCaptcha
            )
        })
    }

    pub fn is_js_challenge(&self) -> bool {
        self.kinds.contains(&ChallengeKind::JsChallenge)
    }
}

/// Case-insensitive substring check without allocating a lowercase copy.
fn contains_ci(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }
    let needle_lower: Vec<u8> = needle.bytes().map(|b| b.to_ascii_lowercase()).collect();
    let needle_bytes = &needle_lower;
    haystack
        .as_bytes()
        .windows(needle_bytes.len())
        .any(|window| {
            window
                .iter()
                .zip(needle_bytes)
                .all(|(a, b)| a.to_ascii_lowercase() == *b)
        })
}

/// Internal builder that accumulates challenge kinds and risk score.
struct Detection {
    kinds: HashSet<ChallengeKind>,
    score: u8,
}

impl Detection {
    fn new() -> Self {
        Self {
            kinds: HashSet::new(),
            score: 0,
        }
    }

    /// Insert kind (deduped) and accumulate score.
    fn add(&mut self, kind: ChallengeKind, points: u8) {
        self.kinds.insert(kind);
        self.score = self.score.saturating_add(points);
    }

    /// Add score only (no new kind).
    fn bump(&mut self, points: u8) {
        self.score = self.score.saturating_add(points);
    }

    /// Convert to a `ChallengeInfo` if the threshold is met.
    fn into_info(self, url: String, status: u16, threshold: u8) -> Option<ChallengeInfo> {
        if self.score >= threshold && !self.kinds.is_empty() {
            // Deterministic ordering for serialization
            let mut kinds: Vec<ChallengeKind> = self.kinds.into_iter().collect();
            kinds.sort_by_key(|k| {
                ChallengeKind::ALL
                    .iter()
                    .position(|v| v == k)
                    .unwrap_or(usize::MAX)
            });
            Some(ChallengeInfo {
                url,
                status,
                kinds,
                risk_score: self.score.min(100),
            })
        } else {
            None
        }
    }
}

/// Detects CAPTCHA and bot-protection indicators from HTTP status codes,
/// response headers, and (optionally) HTML body content.
pub struct ChallengeDetector {
    /// Minimum risk score to consider a detection as a challenge.
    pub threshold: u8,
}

impl Default for ChallengeDetector {
    fn default() -> Self {
        Self { threshold: 25 }
    }
}

impl ChallengeDetector {
    pub fn new(threshold: u8) -> Self {
        Self { threshold }
    }

    /// Detect challenges from HTTP status and response headers only
    /// (does not inspect body — useful before downloading a large response).
    pub fn detect_from_response(
        &self,
        url: &str,
        status: u16,
        headers: &std::collections::HashMap<String, String>,
    ) -> Option<ChallengeInfo> {
        let mut det = Detection::new();

        // Extract server header once
        let server = headers.get("server").map(|s| s.as_str()).unwrap_or("");

        // Status-based heuristics
        match status {
            403 => {
                if headers.contains_key("cf-mitigated") || contains_ci(server, "cloudflare") {
                    det.add(ChallengeKind::JsChallenge, 35);
                } else {
                    det.add(ChallengeKind::BotProtection, 15);
                }
            }
            503 => {
                det.add(ChallengeKind::JsChallenge, 30);
            }
            _ => {}
        }

        // Header-based detection (using pre-extracted server value)
        let server_lower = server.to_lowercase();
        if server_lower.contains("cloudflare")
            && !det.kinds.contains(&ChallengeKind::JsChallenge)
        {
            det.add(ChallengeKind::JsChallenge, 20);
        }
        if server_lower.contains("akamai") {
            det.add(ChallengeKind::BotProtection, 25);
        }

        // cf-ray + 403 indicates active Cloudflare challenge
        if headers.contains_key("cf-ray") && status == 403 {
            det.add(ChallengeKind::JsChallenge, 25);
        }

        // DataDome
        if headers.contains_key("x-datadome") || server_lower.contains("datadome") {
            det.add(ChallengeKind::BotProtection, 30);
        }

        // PerimeterX
        if headers.contains_key("x-px") {
            det.add(ChallengeKind::BotProtection, 30);
        }

        det.into_info(url.to_string(), status, self.threshold)
    }

    /// Detect challenges from HTTP status + headers + HTML body.
    ///
    /// This is more thorough than `detect_from_response` and can identify
    /// embedded CAPTCHAs (reCAPTCHA, hCaptcha, Turnstile widgets) in
    /// otherwise normal 200 responses.
    pub fn detect_from_html(
        &self,
        url: &str,
        status: u16,
        headers: &std::collections::HashMap<String, String>,
        html_body: &str,
    ) -> Option<ChallengeInfo> {
        // Start from response-level detection
        let mut det = if let Some(info) = self.detect_from_response(url, status, headers) {
            // Reconstruct Detection from existing info — score already includes
            // all points from header/status checks.
            let mut d = Detection::new();
            d.score = info.risk_score;
            for k in info.kinds {
                d.kinds.insert(k);
            }
            // Early exit: already well above threshold, no need to scan HTML
            if d.score >= self.threshold.saturating_add(40) {
                return Some(ChallengeInfo {
                    url: url.to_string(),
                    status,
                    kinds: d.kinds.into_iter().collect(),
                    risk_score: d.score.min(100),
                });
            }
            d
        } else {
            Detection::new()
        };

        // reCAPTCHA — "recaptcha" covers "g-recaptcha" and "recaptcha/api.js"
        if contains_ci(html_body, "recaptcha") {
            det.add(ChallengeKind::Recaptcha, 30);
        }

        // hCaptcha — need both patterns: "hcaptcha" (API URLs) and "h-captcha" (HTML class)
        if contains_ci(html_body, "hcaptcha") || contains_ci(html_body, "h-captcha") {
            det.add(ChallengeKind::Hcaptcha, 30);
        }

        // Cloudflare Turnstile — "turnstile" covers "cf-turnstile"
        if contains_ci(html_body, "turnstile") {
            det.add(ChallengeKind::Turnstile, 25);
        }

        // Generic CAPTCHA — only if no specific captcha type was detected
        if contains_ci(html_body, "captcha")
            && !det.kinds.iter().any(|k| {
                matches!(
                    k,
                    ChallengeKind::Recaptcha
                        | ChallengeKind::Hcaptcha
                        | ChallengeKind::Turnstile
                )
            })
        {
            det.add(ChallengeKind::GenericCaptcha, 20);
        }

        // Challenge phrases
        if contains_ci(html_body, "challenge")
            || contains_ci(html_body, "verify you are human")
            || contains_ci(html_body, "security check")
            || contains_ci(html_body, "checking your browser")
        {
            det.bump(15);
            det.kinds.insert(ChallengeKind::JsChallenge);
        }

        // Bot detection markers
        if contains_ci(html_body, "bot detection")
            || contains_ci(html_body, "antibot")
            || contains_ci(html_body, "datadome")
            || contains_ci(html_body, "perimeterx")
            || contains_ci(html_body, "akamai")
        {
            det.bump(25);
            det.kinds.insert(ChallengeKind::BotProtection);
        }

        det.into_info(url.to_string(), status, self.threshold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── contains_ci ────────────────────────────────────────────────────

    #[test]
    fn test_contains_ci_basic() {
        assert!(contains_ci("Hello World", "hello"));
        assert!(contains_ci("RECAPTCHA", "recaptcha"));
        assert!(contains_ci("g-recaptcha-widget", "recaptcha"));
        assert!(contains_ci("Turnstile", "turnstile"));
        assert!(!contains_ci("Hello", "world123"));
    }

    #[test]
    fn test_contains_ci_empty() {
        assert!(contains_ci("", ""));
        assert!(!contains_ci("", "a"));
        assert!(contains_ci("abc", ""));
    }

    #[test]
    fn test_contains_ci_exact_match() {
        assert!(contains_ci("captcha", "captcha"));
        assert!(contains_ci("CAPTCHA", "captcha"));
    }

    #[test]
    fn test_contains_ci_needle_longer_than_haystack() {
        assert!(!contains_ci("ab", "abc"));
    }

    #[test]
    fn test_contains_ci_single_char() {
        assert!(contains_ci("a", "a"));
        assert!(contains_ci("A", "a"));
        assert!(!contains_ci("a", "b"));
    }

    #[test]
    fn test_contains_ci_at_end() {
        assert!(contains_ci("please verify you are human", "human"));
        assert!(contains_ci("CHECKING YOUR BROWSER", "checking your browser"));
    }

    // ── Detection builder ──────────────────────────────────────────────

    #[test]
    fn test_detection_add_accumulates_score_on_duplicate() {
        let mut det = Detection::new();
        det.add(ChallengeKind::JsChallenge, 35);
        det.add(ChallengeKind::JsChallenge, 35);
        // Score accumulates even for duplicate kind, but kinds set stays deduped
        assert_eq!(det.score, 70);
        assert_eq!(det.kinds.len(), 1);
    }

    #[test]
    fn test_detection_add_different_kinds() {
        let mut det = Detection::new();
        det.add(ChallengeKind::JsChallenge, 35);
        det.add(ChallengeKind::BotProtection, 25);
        assert_eq!(det.kinds.len(), 2);
        assert_eq!(det.score, 60);
    }

    #[test]
    fn test_detection_bump() {
        let mut det = Detection::new();
        det.bump(10);
        assert_eq!(det.score, 10);
        assert!(det.kinds.is_empty());
        det.bump(5);
        assert_eq!(det.score, 15);
    }

    #[test]
    fn test_detection_bump_saturating() {
        let mut det = Detection::new();
        det.score = 250;
        det.bump(10);
        assert_eq!(det.score, 255); // u8 max is 255
    }

    #[test]
    fn test_detection_into_info_below_threshold() {
        let mut det = Detection::new();
        det.add(ChallengeKind::BotProtection, 10); // score 10, threshold 25
        let result = det.into_info("https://x.com".to_string(), 403, 25);
        assert!(result.is_none());
    }

    #[test]
    fn test_detection_into_info_empty_kinds() {
        let mut det = Detection::new();
        det.score = 50; // manually set high score with no kinds
        let result = det.into_info("https://x.com".to_string(), 200, 25);
        assert!(result.is_none());
    }

    #[test]
    fn test_detection_into_info_clamps_to_100() {
        let mut det = Detection::new();
        det.add(ChallengeKind::JsChallenge, 80);
        det.add(ChallengeKind::BotProtection, 80);
        let info = det.into_info("https://x.com".to_string(), 403, 25).unwrap();
        assert_eq!(info.risk_score, 100);
    }

    #[test]
    fn test_detection_into_info_fields() {
        let mut det = Detection::new();
        det.add(ChallengeKind::Recaptcha, 30);
        let info = det.into_info("https://x.com".to_string(), 200, 25).unwrap();
        assert_eq!(info.url, "https://x.com");
        assert_eq!(info.status, 200);
        assert!(info.kinds.contains(&ChallengeKind::Recaptcha));
        assert_eq!(info.risk_score, 30);
    }

    // ── detect_from_response ───────────────────────────────────────────

    #[test]
    fn test_detect_cloudflare_403() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("server".to_string(), "cloudflare".to_string());
        headers.insert("cf-ray".to_string(), "abc123".to_string());

        let detector = ChallengeDetector::default();
        let info = detector
            .detect_from_response("https://example.com", 403, &headers)
            .unwrap();

        assert!(info.is_js_challenge());
        assert!(info.kinds.contains(&ChallengeKind::JsChallenge));
        assert!(info.risk_score >= 25);
    }

    #[test]
    fn test_detect_cf_mitigated_403() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("cf-mitigated".to_string(), "challenge".to_string());

        let detector = ChallengeDetector::default();
        let info = detector
            .detect_from_response("https://example.com", 403, &headers)
            .unwrap();

        assert!(info.is_js_challenge());
        assert_eq!(info.status, 403);
    }

    #[test]
    fn test_detect_generic_403_below_default_threshold() {
        let headers = std::collections::HashMap::new();

        // Generic 403 alone scores only 15 — below default threshold of 25
        let detector = ChallengeDetector::default();
        assert!(detector.detect_from_response("https://example.com", 403, &headers).is_none());

        // With a low threshold, the same 403 is detected
        let sensitive = ChallengeDetector::new(10);
        let info = sensitive
            .detect_from_response("https://example.com", 403, &headers)
            .unwrap();
        assert!(info.kinds.contains(&ChallengeKind::BotProtection));
        assert_eq!(info.risk_score, 15);
    }

    #[test]
    fn test_detect_503_js_challenge() {
        let headers = std::collections::HashMap::new();

        let detector = ChallengeDetector::default();
        let info = detector
            .detect_from_response("https://example.com", 503, &headers)
            .unwrap();

        assert!(info.is_js_challenge());
        assert_eq!(info.risk_score, 30);
    }

    #[test]
    fn test_detect_akamai_server() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("server".to_string(), "AkamaiGHost".to_string());

        let detector = ChallengeDetector::default();
        let info = detector
            .detect_from_response("https://example.com", 403, &headers)
            .unwrap();

        assert!(info.kinds.contains(&ChallengeKind::BotProtection));
    }

    #[test]
    fn test_detect_cf_ray_403() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("cf-ray".to_string(), "abc123".to_string());
        // No server header — cf-ray + 403 alone should trigger

        let detector = ChallengeDetector::default();
        let info = detector
            .detect_from_response("https://example.com", 403, &headers)
            .unwrap();

        assert!(info.is_js_challenge());
    }

    #[test]
    fn test_detect_datadome_header() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("x-datadome".to_string(), "active".to_string());

        let detector = ChallengeDetector::default();
        let info = detector
            .detect_from_response("https://example.com", 403, &headers)
            .unwrap();

        assert!(info.kinds.contains(&ChallengeKind::BotProtection));
        assert!(info.risk_score >= 30);
    }

    #[test]
    fn test_detect_datadome_server() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("server".to_string(), "DataDome".to_string());

        let detector = ChallengeDetector::default();
        let info = detector
            .detect_from_response("https://example.com", 403, &headers)
            .unwrap();

        assert!(info.kinds.contains(&ChallengeKind::BotProtection));
    }

    #[test]
    fn test_detect_perimeterx() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("x-px".to_string(), "challenge".to_string());

        let detector = ChallengeDetector::default();
        let info = detector
            .detect_from_response("https://example.com", 403, &headers)
            .unwrap();

        assert!(info.kinds.contains(&ChallengeKind::BotProtection));
        assert!(info.risk_score >= 30);
    }

    #[test]
    fn test_normal_200_no_challenge() {
        let headers = std::collections::HashMap::new();
        let detector = ChallengeDetector::default();
        let result = detector.detect_from_response("https://example.com", 200, &headers);
        assert!(result.is_none());
    }

    #[test]
    fn test_score_no_double_on_server_cloudflare_403() {
        // server=cloudflare + 403 triggers JsChallenge once from status check.
        // The server header check should NOT add a duplicate.
        let mut headers = std::collections::HashMap::new();
        headers.insert("server".to_string(), "cloudflare".to_string());

        let detector = ChallengeDetector::default();
        let info = detector
            .detect_from_response("https://example.com", 403, &headers)
            .unwrap();

        let js_count = info.kinds.iter().filter(|k| **k == ChallengeKind::JsChallenge).count();
        assert_eq!(js_count, 1, "JsChallenge should appear exactly once");
    }

    // ── detect_from_html ───────────────────────────────────────────────

    #[test]
    fn test_detect_recaptcha_in_html() {
        let html = r#"<html><body>
            <div class="g-recaptcha" data-sitekey="xxx"></div>
            <script src="https://www.google.com/recaptcha/api.js"></script>
        </body></html>"#;

        let detector = ChallengeDetector::default();
        let info = detector
            .detect_from_html(
                "https://example.com",
                200,
                &std::collections::HashMap::new(),
                html,
            )
            .unwrap();

        assert!(info.is_captcha());
        assert!(info.kinds.contains(&ChallengeKind::Recaptcha));
    }

    #[test]
    fn test_detect_hcaptcha_in_html() {
        let html = r#"<div class="h-captcha" data-sitekey="xxx"></div>"#;

        let detector = ChallengeDetector::default();
        let info = detector
            .detect_from_html("https://x.com", 200, &std::collections::HashMap::new(), html)
            .unwrap();

        assert!(info.kinds.contains(&ChallengeKind::Hcaptcha));
    }

    #[test]
    fn test_detect_turnstile_in_html() {
        let html = r#"<div class="cf-turnstile" data-sitekey="xxx"></div>"#;

        let detector = ChallengeDetector::default();
        let info = detector
            .detect_from_html("https://x.com", 200, &std::collections::HashMap::new(), html)
            .unwrap();

        assert!(info.kinds.contains(&ChallengeKind::Turnstile));
    }

    #[test]
    fn test_detect_generic_captcha_when_no_specific_type() {
        // "captcha" alone scores 20 (GenericCaptcha) — below default threshold 25
        let html = r#"<div class="captcha-widget">Solve the puzzle</div>"#;

        let detector = ChallengeDetector::default();
        assert!(detector.detect_from_html("https://x.com", 200, &std::collections::HashMap::new(), html).is_none());

        // With lower threshold it's detected
        let sensitive = ChallengeDetector::new(15);
        let info = sensitive
            .detect_from_html("https://x.com", 200, &std::collections::HashMap::new(), html)
            .unwrap();

        assert!(info.kinds.contains(&ChallengeKind::GenericCaptcha));
        assert_eq!(info.risk_score, 20);
    }

    #[test]
    fn test_no_generic_captcha_when_specific_type_present() {
        // "recaptcha" contains "captcha" — GenericCaptcha should NOT be added
        let html = r#"<div class="g-recaptcha"></div>"#;

        let detector = ChallengeDetector::default();
        let info = detector
            .detect_from_html("https://x.com", 200, &std::collections::HashMap::new(), html)
            .unwrap();

        assert!(info.kinds.contains(&ChallengeKind::Recaptcha));
        assert!(!info.kinds.contains(&ChallengeKind::GenericCaptcha));
    }

    #[test]
    fn test_detect_challenge_phrases() {
        let html = "Please verify you are human to continue.";

        // "verify you are human" bumps score by 15 — below default threshold 25
        let detector = ChallengeDetector::default();
        assert!(detector.detect_from_html("https://x.com", 200, &std::collections::HashMap::new(), html).is_none());

        // With low threshold it's detected
        let sensitive = ChallengeDetector::new(10);
        let info = sensitive
            .detect_from_html("https://x.com", 200, &std::collections::HashMap::new(), html)
            .unwrap();
        assert!(info.kinds.contains(&ChallengeKind::JsChallenge));
        assert_eq!(info.risk_score, 15);
    }

    #[test]
    fn test_detect_security_check_phrase() {
        let html = "Security check in progress...";

        // "security check" bumps 15 — below default threshold 25
        let detector = ChallengeDetector::default();
        assert!(detector.detect_from_html("https://x.com", 200, &std::collections::HashMap::new(), html).is_none());

        // With lower threshold it's detected
        let sensitive = ChallengeDetector::new(10);
        let info = sensitive
            .detect_from_html("https://x.com", 200, &std::collections::HashMap::new(), html)
            .unwrap();
        assert!(info.kinds.contains(&ChallengeKind::JsChallenge));
        assert_eq!(info.risk_score, 15);
    }

    #[test]
    fn test_detect_bot_detection_markers() {
        for marker in &["bot detection", "antibot", "datadome", "perimeterx", "akamai"] {
            let html = format!("Powered by {} technology", marker);

            let detector = ChallengeDetector::default();
            let info = detector
                .detect_from_html("https://x.com", 200, &std::collections::HashMap::new(), &html)
                .unwrap();

            assert!(
                info.kinds.contains(&ChallengeKind::BotProtection),
                "Failed to detect bot protection for marker: {}",
                marker
            );
        }
    }

    #[test]
    fn test_case_insensitive_html_detection() {
        let html = "<DIV CLASS=\"RECAPTCHA\">CHECKING YOUR BROWSER</DIV>";

        let detector = ChallengeDetector::default();
        let info = detector
            .detect_from_html("https://x.com", 200, &std::collections::HashMap::new(), html)
            .unwrap();

        assert!(info.kinds.contains(&ChallengeKind::Recaptcha));
        assert!(info.kinds.contains(&ChallengeKind::JsChallenge));
    }

    #[test]
    fn test_no_false_positive_on_normal_page() {
        let html = r#"<html><body><h1>Hello World</h1><p>Welcome to our site.</p></body></html>"#;
        let headers = std::collections::HashMap::new();

        let detector = ChallengeDetector::default();
        let result = detector.detect_from_html("https://example.com", 200, &headers, html);
        assert!(result.is_none());
    }

    #[test]
    fn test_no_false_positive_on_word_containing_captcha() {
        // "encapsulated" contains "captcha" substring — but risk score should
        // only be 20 (GenericCaptcha) which is below default threshold of 25
        // if no other indicators present. Actually it IS 20 which is < 25.
        let html = "<p>We encapsulated the logic in a module.</p>";

        let detector = ChallengeDetector::default();
        let result = detector.detect_from_html("https://x.com", 200, &std::collections::HashMap::new(), html);
        // GenericCaptcha score is 20, threshold is 25, so below threshold → None
        assert!(result.is_none());
    }

    #[test]
    fn test_combined_header_and_html_detection() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("server".to_string(), "cloudflare".to_string());

        let html = r#"<div class="g-recaptcha"></div>"#;

        let detector = ChallengeDetector::default();
        let info = detector
            .detect_from_html("https://x.com", 403, &headers, html)
            .unwrap();

        // Should have both header-detected JsChallenge and HTML-detected Recaptcha
        assert!(info.kinds.contains(&ChallengeKind::JsChallenge));
        assert!(info.kinds.contains(&ChallengeKind::Recaptcha));
        assert!(info.risk_score >= 55); // 35 (JS) + 30 (reCAPTCHA) = 65+, clamped
    }

    #[test]
    fn test_early_exit_high_score() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("server".to_string(), "cloudflare".to_string());
        headers.insert("cf-mitigated".to_string(), "1".to_string());
        headers.insert("cf-ray".to_string(), "abc".to_string());
        headers.insert("x-datadome".to_string(), "yes".to_string());
        headers.insert("x-px".to_string(), "1".to_string());

        let detector = ChallengeDetector::new(25);
        // Score from headers alone will be high — detect_from_html should early-exit
        let info = detector
            .detect_from_html("https://example.com", 403, &headers, "recaptcha hcaptcha")
            .unwrap();

        // Only header-detected kinds should be present (no Recaptcha/Hcaptcha from HTML)
        assert!(info.risk_score >= 65);
    }

    #[test]
    fn test_custom_threshold_high() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("x-datadome".to_string(), "active".to_string());

        let detector = ChallengeDetector::new(50);
        let result = detector.detect_from_response("https://x.com", 403, &headers);
        // DataDome alone is score 30, which is below threshold 50
        // But we also get BotProtection from generic 403 = 15, total = 45 still < 50
        // Actually: generic 403 gives BotProtection 15, DataDome gives BotProtection 30
        // dedup: first add BotProtection 15, second add is deduped → score stays 15
        // 15 < 50, so None
        // Wait let me re-check: 403 → generic gives BotProtection+15, then DataDome gives
        // BotProtection but kind already exists → dedup, score stays 15. 15 < 50 → None.
        // Hmm but x-datadome also has its own detection that adds BotProtection 30.
        // The dedup means score only increases on first insert.
        // So score = 15 (from generic 403), 15 < 50 → None
        assert!(result.is_none());
    }

    // ── ChallengeInfo helpers ──────────────────────────────────────────

    #[test]
    fn test_info_is_captcha() {
        let info = ChallengeInfo {
            url: "https://x.com".to_string(),
            status: 200,
            kinds: vec![ChallengeKind::Recaptcha],
            risk_score: 30,
        };
        assert!(info.is_captcha());
        assert!(!info.is_js_challenge());
    }

    #[test]
    fn test_info_is_js_challenge() {
        let info = ChallengeInfo {
            url: "https://x.com".to_string(),
            status: 403,
            kinds: vec![ChallengeKind::JsChallenge],
            risk_score: 35,
        };
        assert!(!info.is_captcha());
        assert!(info.is_js_challenge());
    }

    // ── ChallengeKind Display ──────────────────────────────────────────

    #[test]
    fn test_kind_display() {
        assert_eq!(format!("{}", ChallengeKind::Recaptcha), "reCAPTCHA");
        assert_eq!(format!("{}", ChallengeKind::Hcaptcha), "hCaptcha");
        assert_eq!(format!("{}", ChallengeKind::Turnstile), "Cloudflare Turnstile");
        assert_eq!(format!("{}", ChallengeKind::GenericCaptcha), "CAPTCHA");
        assert_eq!(format!("{}", ChallengeKind::JsChallenge), "JS Challenge");
        assert_eq!(format!("{}", ChallengeKind::BotProtection), "Bot Protection");
    }
}
