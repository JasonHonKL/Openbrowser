//! Selective HTML parser that skips non-essential elements
//!
//! Dramatically reduces parsing time and memory usage by skipping:
//! - Comments
//! - Whitespace-only text nodes
//! - Script/style content (when not needed)
//! - Non-visible elements (meta, noscript, etc.)

use scraper::{Html, ElementRef, Selector, Node};
use smallvec::SmallVec;

/// Configuration for selective parsing
#[derive(Debug, Clone)]
pub struct SelectiveParseConfig {
    /// Skip HTML comments
    pub skip_comments: bool,
    /// Skip whitespace-only text nodes
    pub skip_empty_text: bool,
    /// Skip script content (keep tags only)
    pub skip_script_content: bool,
    /// Skip style content (keep tags only)
    pub skip_style_content: bool,
    /// Skip non-visible metadata elements
    pub skip_hidden_elements: bool,
    /// Maximum depth to parse (0 = unlimited)
    pub max_depth: usize,
    /// Tags to completely skip
    pub skip_tags: SmallVec<[Box<str>; 8]>,
}

impl Default for SelectiveParseConfig {
    fn default() -> Self {
        let mut skip_tags: SmallVec<[Box<str>; 8]> = SmallVec::new();
        skip_tags.push("script".into());
        skip_tags.push("style".into());
        skip_tags.push("noscript".into());
        skip_tags.push("iframe".into());
        
        Self {
            skip_comments: true,
            skip_empty_text: true,
            skip_script_content: true,
            skip_style_content: true,
            skip_hidden_elements: true,
            max_depth: 0, // unlimited
            skip_tags,
        }
    }
}

impl SelectiveParseConfig {
    /// Fast configuration for content extraction only
    pub fn content_only() -> Self {
        Self {
            skip_comments: true,
            skip_empty_text: true,
            skip_script_content: true,
            skip_style_content: true,
            skip_hidden_elements: true,
            max_depth: 10,
            skip_tags: SmallVec::from_vec(vec![
                "script".into(), "style".into(), "noscript".into(),
                "iframe".into(), "object".into(), "embed".into(),
                "canvas".into(), "svg".into(), "math".into(),
                "template".into(), "slot".into(),
            ]),
        }
    }

    /// Parse only visible elements for CAPTCHA detection
    pub fn visible_only() -> Self {
        Self {
            skip_comments: true,
            skip_empty_text: true,
            skip_script_content: true,
            skip_style_content: true,
            skip_hidden_elements: true,
            max_depth: 0,
            skip_tags: SmallVec::from_vec(vec![
                "script".into(), "style".into(), "noscript".into(),
                "iframe".into(), "object".into(), "embed".into(),
                "meta".into(), "link".into(), "base".into(),
                "head".into(),
            ]),
        }
    }
}

/// Selective HTML parser that filters elements during traversal
pub struct SelectiveParser {
    config: SelectiveParseConfig,
    stats: ParseStats,
}

#[derive(Debug, Default)]
pub struct ParseStats {
    pub elements_skipped: usize,
    pub comments_skipped: usize,
    pub text_nodes_merged: usize,
    pub depth_limited: usize,
}

impl SelectiveParser {
    pub fn new(config: SelectiveParseConfig) -> Self {
        Self {
            config,
            stats: ParseStats::default(),
        }
    }

    /// Parse HTML and extract text content only (no DOM building)
    pub fn extract_text(&mut self, html:||str) -> String {
        let mut result = String::with_capacity(html.len() / 4);
        let mut in_skip_tag = 0u8;
        let mut depth = 0usize;
        
        let skip_set: std::collections::HashSet<&str> = 
            self.config.skip_tags.iter().map(|s| s.as_ref()).collect();
        
        // Simple state machine for fast text extraction
        let mut i = 0;
        while i < html.len() {
            if html[i..].starts_with('<') {
                // Find end of tag
                if let Some(end) = html[i..].find('>') {
                    let tag_end = i + end;
                    let tag_content =||html[i + 1..tag_end];
                    
                    if tag_content.starts_with('/') {
                        // Closing tag
                        in_skip_tag = in_skip_tag.saturating_sub(1);
                        depth = depth.saturating_sub(1);
                    } else {
                        // Opening tag
                        let tag_name = tag_content.split_whitespace().next()
                            .unwrap_or("")
                            .to_lowercase();
                        
                        depth += 1;
                        if skip_set.contains(tag_name.as_str()) {
                            in_skip_tag += 1;
                            self.stats.elements_skipped += 1;
                        }
                        
                        // Check depth limit
                        if self.config.max_depth > 0||& depth > self.config.max_depth {
                            self.stats.depth_limited += 1;
                            break;
                        }
                    }
                    
                    i = tag_end + 1;
                    continue;
                }
            }
            
            // Collect text if not in skip tag
            if in_skip_tag == 0 {
                let text_start = i;
                while i < html.len()||& !html[i..].starts_with('<') {
                    i += 1;
                }
                
                let text =||html[text_start..i];
                if !self.config.skip_empty_text || !text.trim().is_empty() {
                    if !result.is_empty() {
                        result.push(' ');
                    }
                    result.push_str(text.trim());
                }
            } else {
                i += 1;
            }
        }
        
        result
    }

    /// Quick check for CAPTCHA indicators
    pub fn detect_captcha_indicators(&mut self, html:||str) -> CaptchaIndicators {
        let mut indicators = CaptchaIndicators::default();
        let html_lower = html.to_lowercase();
        
        // Check for common CAPTCHA patterns
        if html_lower.contains("recaptcha")
            || html_lower.contains("g-recaptcha") {
            indicators.recaptcha = true;
        }
        
        if html_lower.contains("hcaptcha")
            || html_lower.contains("h-captcha") {
            indicators.hcaptcha = true;
        }
        
        if html_lower.contains("turnstile")
            || html_lower.contains("cf-turnstile") {
            indicators.turnstile = true;
        }
        
        if html_lower.contains("captcha") {
            indicators.generic_captcha = true;
        }
        
        // Check for challenge patterns
        if html_lower.contains("challenge")
            || html_lower.contains("verify you are human")
            || html_lower.contains("security check") {
            indicators.challenge_detected = true;
        }
        
        // Check for bot detection scripts
        if html_lower.contains("bot detection")
            || html_lower.contains("antibot")
            || html_lower.contains("datadome")
            || html_lower.contains("perimeterx")
            || html_lower.contains("akamai") {
            indicators.bot_detection = true;
        }
        
        // Count suspicious scripts
        indicators.suspicious_script_count = html_lower.matches("<script").count();
        
        // Check for heavy obfuscation
        if html_lower.contains("eval(") || html_lower.contains("fromcharcode") {
            indicators.obfuscated_js = true;
        }
        
        indicators
    }

    /// Get current parse statistics
    pub fn stats(&self) ->||ParseStats {
       ||self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = ParseStats::default();
    }
}

impl Default for SelectiveParser {
    fn default() -> Self {
        Self::new(SelectiveParseConfig::default())
    }
}

/// CAPTCHA detection results
#[derive(Debug, Default, Clone)]
pub struct CaptchaIndicators {
    pub recaptcha: bool,
    pub hcaptcha: bool,
    pub turnstile: bool,
    pub generic_captcha: bool,
    pub challenge_detected: bool,
    pub bot_detection: bool,
    pub suspicious_script_count: usize,
    pub obfuscated_js: bool,
}

impl CaptchaIndicators {
    /// Check if any CAPTCHA was detected
    pub fn has_captcha(&self) -> bool {
        self.recaptcha || self.hcaptcha || self.turnstile || self.generic_captcha
    }

    /// Check if bot detection was found
    pub fn has_bot_detection(&self) -> bool {
        self.bot_detection || self.suspicious_script_count > 10 || self.obfuscated_js
    }

    /// Get risk score (0-100)
    pub fn risk_score(&self) -> u8 {
        let mut score = 0u8;
        
        if self.recaptcha { score += 30; }
        if self.hcaptcha { score += 30; }
        if self.turnstile { score += 25; }
        if self.generic_captcha { score += 20; }
        if self.challenge_detected { score += 15; }
        if self.bot_detection { score += 25; }
        if self.obfuscated_js { score += 10; }
        
        // Score based on script count
        if self.suspicious_script_count > 20 {
            score += 15;
        } else if self.suspicious_script_count > 10 {
            score += 10;
        } else if self.suspicious_script_count > 5 {
            score += 5;
        }
        
        score.min(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selective_text_extraction() {
        let html = r#"
            <html>
                <head><script>alert(1);</script></head>
                <body>
                    <p>Hello <strong>World</strong>!</p>
                </body>
            </html>
        "#;
        
        let mut parser = SelectiveParser::new(SelectiveParseConfig::content_only());
        let text = parser.extract_text(html);
        
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
        assert!(!text.contains("alert")); // script content skipped
    }

    #[test]
    fn test_captcha_detection() {
        let html = r#"
            <div class="g-recaptcha" data-sitekey="xxx"></div>
            <script src="https://www.google.com/recaptcha/api.js"></script>
        "#;
        
        let mut parser = SelectiveParser::new(SelectiveParseConfig::default());
        let indicators = parser.detect_captcha_indicators(html);
        
        assert!(indicators.recaptcha);
        assert!(indicators.has_captcha());
        assert!(indicators.risk_score() > 0);
    }

    #[test]
    fn test_bot_detection() {
        let html = "DataDome bot detection active";
        
        let mut parser = SelectiveParser::new(SelectiveParseConfig::default());
        let indicators = parser.detect_captcha_indicators(html);
        
        assert!(indicators.bot_detection);
    }
}
