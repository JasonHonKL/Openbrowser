use scraper::{Html, Selector};
use serde::Serialize;

use crate::record::{NetworkLog, ResourceType};

// ---------------------------------------------------------------------------
// Coverage report types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct CoverageReport {
    pub url: String,
    pub css: CssCoverage,
    pub js: JsCoverage,
    pub summary: CoverageSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoverageSummary {
    pub total_css_rules: usize,
    pub matched_css_rules: usize,
    pub unmatched_css_rules: usize,
    pub untestable_css_rules: usize,
    pub total_inline_scripts: usize,
    pub total_external_scripts: usize,
    pub fetched_external_scripts: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CssCoverage {
    pub stylesheets: Vec<StylesheetCoverage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StylesheetCoverage {
    pub source: String,
    pub rules: Vec<RuleCoverage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleCoverage {
    pub selector: String,
    pub match_count: usize,
    pub status: String,
    pub declarations: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsCoverage {
    pub inline_scripts: Vec<InlineScriptCoverage>,
    pub external_scripts: Vec<ExternalScriptCoverage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InlineScriptCoverage {
    pub index: usize,
    pub preview: String,
    pub size: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalScriptCoverage {
    pub url: String,
    pub fetched: bool,
    pub status: Option<u16>,
    pub size: Option<usize>,
}

// ---------------------------------------------------------------------------
// Build coverage report
// ---------------------------------------------------------------------------

impl CoverageReport {
    pub fn build(
        url: &str,
        html: &Html,
        css_sources: &[(String, String)],
        network_log: &NetworkLog,
    ) -> Self {
        let css = analyze_css(html, css_sources);
        let js = analyze_js(html, network_log);
        let summary = CoverageSummary {
            total_css_rules: css.stylesheets.iter().map(|s| s.rules.len()).sum(),
            matched_css_rules: css.stylesheets.iter()
                .flat_map(|s| &s.rules)
                .filter(|r| r.status == "matched")
                .count(),
            unmatched_css_rules: css.stylesheets.iter()
                .flat_map(|s| &s.rules)
                .filter(|r| r.status == "unmatched")
                .count(),
            untestable_css_rules: css.stylesheets.iter()
                .flat_map(|s| &s.rules)
                .filter(|r| r.status == "untestable")
                .count(),
            total_inline_scripts: js.inline_scripts.len(),
            total_external_scripts: js.external_scripts.len(),
            fetched_external_scripts: js.external_scripts.iter().filter(|s| s.fetched).count(),
        };
        Self {
            url: url.to_string(),
            css,
            js,
            summary,
        }
    }
}

// ---------------------------------------------------------------------------
// CSS analysis
// ---------------------------------------------------------------------------

fn analyze_css(html: &Html, css_sources: &[(String, String)]) -> CssCoverage {
    let mut stylesheets = Vec::new();

    for (source, css_text) in css_sources {
        let rules = extract_and_match_rules(html, css_text);
        stylesheets.push(StylesheetCoverage {
            source: source.clone(),
            rules,
        });
    }

    CssCoverage { stylesheets }
}

/// Extract CSS rules from a stylesheet text and test each selector against the DOM.
fn extract_and_match_rules(html: &Html, css_text: &str) -> Vec<RuleCoverage> {
    let mut rules = Vec::new();
    let mut depth = 0i32;
    let mut selector_start = 0;
    let mut decl_start: Option<usize> = None;
    let chars: Vec<char> = css_text.chars().collect();
    let len = chars.len();

    let mut i = 0;
    while i < len {
        let c = chars[i];

        // Skip @-rules at the top level
        if depth == 0 && c == '@' {
            let mut at_depth = 0i32;
            while i < len {
                if chars[i] == '{' { at_depth += 1; }
                else if chars[i] == '}' {
                    at_depth -= 1;
                    if at_depth == 0 { break; }
                }
                i += 1;
            }
            i += 1;
            selector_start = i;
            continue;
        }

        if c == '{' {
            if depth == 0 {
                let selector_text: String = chars[selector_start..i].iter().collect();
                decl_start = Some(i + 1);
                let selectors = split_selectors(&selector_text);
                for sel_str in selectors {
                    let sel_str = sel_str.trim().to_string();
                    if sel_str.is_empty() { continue; }
                    let sel_for_parse = sel_str.clone();
                    let parse_result = Selector::parse(&sel_for_parse);
                    match parse_result {
                        Ok(sel) => {
                            let count = html.select(&sel).count();
                            let status_str = if count > 0 { "matched".into() } else { "unmatched".into() };
                            rules.push(RuleCoverage {
                                selector: sel_str,
                                match_count: count,
                                status: status_str,
                                declarations: String::new(),
                            });
                        }
                        Err(_) => {
                            rules.push(RuleCoverage {
                                selector: sel_str,
                                match_count: 0,
                                status: "untestable".into(),
                                declarations: String::new(),
                            });
                        }
                    }
                }
            }
            depth += 1;
        } else if c == '}' {
            depth -= 1;
            if depth == 0 {
                if let Some(ds) = decl_start.take() {
                    let decl_text: String = chars[ds..i].iter().collect();
                    let decl_text = decl_text.trim().to_string();
                    for rule in rules.iter_mut().rev() {
                        if rule.declarations.is_empty() {
                            rule.declarations = decl_text.clone();
                        } else {
                            break;
                        }
                    }
                }
                selector_start = i + 1;
            }
        }

        i += 1;
    }

    rules
}

/// Split a CSS selector list on commas, respecting parentheses.
fn split_selectors(input: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;

    for (i, c) in input.chars().enumerate() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                result.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < input.len() {
        result.push(&input[start..]);
    }
    result
}

// ---------------------------------------------------------------------------
// JS analysis
// ---------------------------------------------------------------------------

fn analyze_js(html: &Html, network_log: &NetworkLog) -> JsCoverage {
    let mut inline_scripts = Vec::new();

    if let Ok(sel) = Selector::parse("script:not([src])") {
        for (idx, el) in html.select(&sel).enumerate() {
            let text: String = el.text().collect();
            let text = text.trim();
            inline_scripts.push(InlineScriptCoverage {
                index: idx,
                preview: text.chars().take(200).collect(),
                size: text.len(),
            });
        }
    }

    let mut external_scripts = Vec::new();
    if let Ok(sel) = Selector::parse("script[src]") {
        for el in html.select(&sel) {
            if let Some(src) = el.value().attr("src") {
                // Check network log for fetch status
                let log_entry = network_log.records.iter()
                    .find(|r| r.url.ends_with(src) && r.resource_type == ResourceType::Script);

                let (fetched, status, size) = match log_entry {
                    Some(r) if r.status.is_some() => (true, r.status, r.body_size),
                    Some(r) if r.error.is_some() => (false, None, None),
                    _ => (false, None, None),
                };

                external_scripts.push(ExternalScriptCoverage {
                    url: src.to_string(),
                    fetched,
                    status,
                    size,
                });
            }
        }
    }

    JsCoverage { inline_scripts, external_scripts }
}

/// Extract inline `<style>` text from HTML as (source_label, css_text) pairs.
pub fn extract_inline_styles(html: &Html) -> Vec<(String, String)> {
    let mut sources = Vec::new();
    if let Ok(sel) = Selector::parse("style") {
        for (idx, el) in html.select(&sel).enumerate() {
            let text: String = el.text().collect();
            sources.push((format!("inline-{}", idx), text));
        }
    }
    sources
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::NetworkRecord;

    fn parse(html: &str) -> Html {
        Html::parse_document(html)
    }

    #[test]
    fn test_css_simple_match() {
        let html = parse(r#"<html><body><div class="box">hello</div></body></html>"#);
        let css = r#".box { color: red; }"#;
        let rules = extract_and_match_rules(&html, css);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].status, "matched");
        assert_eq!(rules[0].match_count, 1);
        assert_eq!(rules[0].declarations.trim(), "color: red;");
    }

    #[test]
    fn test_css_unmatched() {
        let html = parse("<html><body><p>hi</p></body></html>");
        let css = r#".nonexistent { display: none; }"#;
        let rules = extract_and_match_rules(&html, css);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].status, "unmatched");
        assert_eq!(rules[0].match_count, 0);
    }

    #[test]
    fn test_css_untestable_pseudo() {
        let html = parse("<html><body><p>hi</p></body></html>");
        let css = r#"p::before { content: "x"; }"#;
        let rules = extract_and_match_rules(&html, css);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].status, "untestable");
    }

    #[test]
    fn test_css_multiple_selectors() {
        let html = parse(r#"<html><body><h1>a</h1><h2>b</h2></body></html>"#);
        let css = r#"h1, h2 { font-weight: bold; }"#;
        let rules = extract_and_match_rules(&html, css);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].status, "matched");
        assert_eq!(rules[1].status, "matched");
    }

    #[test]
    fn test_css_at_rule_skipped() {
        let html = parse("<html><body></body></html>");
        let css = r#"@media (max-width: 600px) { .box { color: red; } } .visible { display: block; }"#;
        let rules = extract_and_match_rules(&html, css);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].selector, ".visible");
    }

    #[test]
    fn test_js_inline_scripts() {
        let html = parse(r#"<html><body>
            <script>console.log("a");</script>
            <script>console.log("b");</script>
        </body></html>"#);
        let log = NetworkLog::new();
        let js = analyze_js(&html, &log);
        assert_eq!(js.inline_scripts.len(), 2);
    }

    #[test]
    fn test_js_external_scripts() {
        let html = parse(r#"<html><body>
            <script src="/app.js"></script>
            <script src="/vendor.js"></script>
        </body></html>"#);
        let mut log = NetworkLog::new();
        let mut r = NetworkRecord::discovered(
            1, ResourceType::Script, "app.js".into(), "https://example.com/app.js".into(), Initiator::Script,
        );
        r.status = Some(200);
        r.body_size = Some(4096);
        log.push(r);

        let js = analyze_js(&html, &log);
        assert_eq!(js.external_scripts.len(), 2);
        assert!(js.external_scripts[0].fetched);
        assert_eq!(js.external_scripts[0].status, Some(200));
        assert!(!js.external_scripts[1].fetched);
    }

    #[test]
    fn test_extract_inline_styles() {
        let html = parse(r#"<html><head>
            <style>body { color: red; }</style>
            <style>h1 { font-size: 2em; }</style>
        </head></html>"#);
        let sources = extract_inline_styles(&html);
        assert_eq!(sources.len(), 2);
        assert!(sources[0].1.contains("color: red"));
    }

    #[test]
    fn test_coverage_report_build() {
        let html = parse(r#"<html><head>
            <style>.box { color: red; } .missing { display: none; }</style>
        </head><body><div class="box">hi</div>
            <script src="/app.js"></script>
        </body></html>"#);
        let css_sources = extract_inline_styles(&html);
        let log = NetworkLog::new();
        let report = CoverageReport::build("https://example.com", &html, &css_sources, &log);
        assert_eq!(report.summary.total_css_rules, 2);
        assert_eq!(report.summary.matched_css_rules, 1);
        assert_eq!(report.summary.unmatched_css_rules, 1);
        assert_eq!(report.summary.total_external_scripts, 1);
    }

    #[test]
    fn test_split_selectors() {
        let result = split_selectors("h1, h2, h3");
        assert_eq!(result, vec!["h1", " h2", " h3"]);
    }

    #[test]
    fn test_split_selectors_with_parens() {
        let result = split_selectors(":not(.foo), .bar");
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_css_multiple_rules_one_stylesheet() {
        let html = parse(r#"<html><body><div class="box">hi</div><div id="main">content</div></body></html>"#);
        let css = r#".box { color: red; } .hidden { display: none; } #main { font-size: 16px; }"#;
        let rules = extract_and_match_rules(&html, css);

        assert_eq!(rules.len(), 3);

        // .box - matched
        assert_eq!(rules[0].selector, ".box");
        assert_eq!(rules[0].status, "matched");
        assert_eq!(rules[0].match_count, 1);

        // .hidden - unmatched (no element with class "hidden")
        assert_eq!(rules[1].selector, ".hidden");
        assert_eq!(rules[1].status, "unmatched");
        assert_eq!(rules[1].match_count, 0);

        // #main - matched
        assert_eq!(rules[2].selector, "#main");
        assert_eq!(rules[2].status, "matched");
        assert_eq!(rules[2].match_count, 1);
    }

    #[test]
    fn test_css_empty_stylesheet() {
        let html = parse("<html><body></body></html>");
        let css = "";
        let rules = extract_and_match_rules(&html, css);
        assert!(rules.is_empty());
    }

    #[test]
    fn test_css_nested_at_rules() {
        let html = parse(r#"<html><body><p class="visible">hi</p></body></html>"#);
        let css = r#"@media (max-width: 600px) { @font-face { font-family: "Test"; } } .visible { display: block; }"#;
        let rules = extract_and_match_rules(&html, css);

        // The entire @media block (including nested @font-face) should be skipped.
        // Only .visible should remain.
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].selector, ".visible");
        assert_eq!(rules[0].status, "matched");
    }

    #[test]
    fn test_js_inline_script_preview_truncation() {
        let long_script = "x".repeat(300);
        let html = parse(&format!(
            r#"<html><body><script>{}</script></body></html>"#,
            long_script
        ));
        let log = NetworkLog::new();
        let js = analyze_js(&html, &log);

        assert_eq!(js.inline_scripts.len(), 1);
        assert_eq!(js.inline_scripts[0].preview.len(), 200);
        assert_eq!(js.inline_scripts[0].size, 300);
    }

    #[test]
    fn test_js_no_scripts() {
        let html = parse("<html><body><p>no scripts here</p></body></html>");
        let log = NetworkLog::new();
        let js = analyze_js(&html, &log);

        assert!(js.inline_scripts.is_empty());
        assert!(js.external_scripts.is_empty());
    }

    #[test]
    fn test_coverage_report_empty_page() {
        let html = parse("<html><head></head><body></body></html>");
        let css_sources: Vec<(String, String)> = vec![];
        let log = NetworkLog::new();
        let report = CoverageReport::build("https://example.com", &html, &css_sources, &log);

        assert_eq!(report.summary.total_css_rules, 0);
        assert_eq!(report.summary.matched_css_rules, 0);
        assert_eq!(report.summary.unmatched_css_rules, 0);
        assert_eq!(report.summary.untestable_css_rules, 0);
        assert_eq!(report.summary.total_inline_scripts, 0);
        assert_eq!(report.summary.total_external_scripts, 0);
        assert_eq!(report.summary.fetched_external_scripts, 0);
    }

    #[test]
    fn test_css_descendant_selector() {
        let html = parse(r#"<html><body><div><p>hello</p></div><section>no p here directly</section></body></html>"#);
        let css = r#"div p { color: blue; }"#;
        let rules = extract_and_match_rules(&html, css);

        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].selector, "div p");
        assert_eq!(rules[0].status, "matched");
        assert_eq!(rules[0].match_count, 1);
    }

    #[test]
    fn test_css_class_and_id_selectors() {
        let html = parse(r#"<html><body><div class="myclass">a</div><span id="myid">b</span></body></html>"#);
        let css = r#".myclass { color: red; } #myid { font-weight: bold; }"#;
        let rules = extract_and_match_rules(&html, css);

        assert_eq!(rules.len(), 2);

        assert_eq!(rules[0].selector, ".myclass");
        assert_eq!(rules[0].status, "matched");
        assert_eq!(rules[0].match_count, 1);

        assert_eq!(rules[1].selector, "#myid");
        assert_eq!(rules[1].status, "matched");
        assert_eq!(rules[1].match_count, 1);
    }
}
