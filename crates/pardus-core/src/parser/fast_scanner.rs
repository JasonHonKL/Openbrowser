//! High-performance SIMD-accelerated HTML scanner
//!
//! Uses byte-level scanning with SIMD where available for maximum speed.
//! Designed for rapid extraction of key elements without full parsing.

use memchr::memmem;
use std::simd::{Simd, SimdUint};

/// SIMD width for HTML scanning (64 bytes)
const SIMD_WIDTH: usize = 64;

/// Fast HTML scanner for extracting key information without full DOM construction
pub struct FastScanner {
    /// Pre-computed patterns for common tags
    patterns: TagPatterns,
}

/// Pre-computed byte patterns for fast matching
#[derive(Debug, Clone)]
struct TagPatterns {
    script_open: Vec<u8>,
    script_close: Vec<u8>,
    style_open: Vec<u8>,
    style_close: Vec<u8>,
    link_open: Vec<u8>,
    img_open: Vec<u8>,
    anchor_open: Vec<u8>,
}

impl TagPatterns {
    fn new() -> Self {
        Self {
            script_open: b"<script".to_vec(),
            script_close: b"</script".to_vec(),
            style_open: b"<style".to_vec(),
            style_close: b"</style".to_vec(),
            link_open: b"<link".to_vec(),
            img_open: b"<img".to_vec(),
            anchor_open: b"<a".to_vec(),
        }
    }
}

/// Scan result containing key elements found
#[derive(Debug, Default, Clone)]
pub struct FastScanResult {
    pub scripts: Vec<ScriptTag>,
    pub styles: Vec<StyleTag>,
    pub links: Vec<LinkTag>,
    pub images: Vec<ImageTag>,
    pub anchors: Vec<AnchorTag>,
    pub estimated_element_count: usize,
}

#[derive(Debug, Clone)]
pub struct ScriptTag {
    pub src: Option<String>,
    pub async_: bool,
    pub defer_: bool,
    pub content: Option<String>,
    pub position: usize,
}

#[derive(Debug, Clone)]
pub struct StyleTag {
    pub content: Option<String>,
    pub position: usize,
}

#[derive(Debug, Clone)]
pub struct LinkTag {
    pub href: Option<String>,
    pub rel: Option<String>,
    pub media: Option<String>,
    pub position: usize,
}

#[derive(Debug, Clone)]
pub struct ImageTag {
    pub src: Option<String>,
    pub alt: Option<String>,
    pub position: usize,
}

#[derive(Debug, Clone)]
pub struct AnchorTag {
    pub href: Option<String>,
    pub text: Option<String>,
    pub position: usize,
}

impl FastScanner {
    pub fn new() -> Self {
        Self {
            patterns: TagPatterns::new(),
        }
    }

    /// Scan HTML content using fast SIMD-accelerated byte scanning
    pub fn scan(&self, html: &[u8]) -> FastScanResult {
        let mut result = FastScanResult::default();
        
        // Count angle brackets for element estimation
        result.estimated_element_count = self.count_elements(html);
        
        // Scan for critical resources in parallel
        self.scan_scripts(html, &mut result);
        self.scan_links(html, &mut result);
        self.scan_images(html, &mut result);
        self.scan_anchors(html, &mut result);
        
        result
    }

    /// Quick element count using SIMD byte counting
    fn count_elements(&self, html: &[u8]) -> usize {
        // Use memchr for fast byte counting (SIMD-accelerated)
        memmem::find_iter(html, b"<").count()
    }

    /// Scan for script tags using fast byte search
    fn scan_scripts(&self, html: &[u8], result: &mut FastScanResult) {
        let mut pos = 0;
        
        while let Some(start) = memmem::find(&html[pos..], &self.patterns.script_open) {
            let start_pos = pos + start;
            let after_tag = start_pos + 7; // len("<script")
            
            // Find tag end
            if let Some(end_pos) = memmem::find(&html[after_tag..], b">") {
                let tag_end = after_tag + end_pos;
                let attrs = std::str::from_utf8(&html[after_tag..tag_end]).unwrap_or("");
                
                let src = self.extract_attr(attrs, "src");
                let is_async = attrs.contains("async");
                let is_defer = attrs.contains("defer");
                
                // If inline script, extract content
                let content = if src.is_none() {
                    if let Some(close) = memmem::find(&html[tag_end + 1..], &self.patterns.script_close) {
                        let content_end = tag_end + 1 + close;
                        Some(String::from_utf8_lossy(&html[tag_end + 1..content_end]
                        ).to_string())
                    } else {
                        None
                    }
                } else {
                    None
                };
                
                result.scripts.push(ScriptTag {
                    src: src.map(|s| s.to_string()),
                    async_: is_async,
                    defer_: is_defer,
                    content,
                    position: start_pos,
                });
                
                pos = tag_end + 1;
            } else {
                break;
            }
        }
    }

    /// Scan for link tags
    fn scan_links(&self, html: &[u8], result: &mut FastScanResult) {
        let mut pos = 0;
        
        while let Some(start) = memmem::find(&html[pos..], &self.patterns.link_open) {
            let start_pos = pos + start;
            let after_tag = start_pos + 5; // len("<link")
            
            if let Some(end_pos) = memmem::find(&html[after_tag..], b">") {
                let tag_end = after_tag + end_pos;
                let attrs = std::str::from_utf8(&html[after_tag..tag_end]).unwrap_or("");
                
                result.links.push(LinkTag {
                    href: self.extract_attr(attrs, "href").map(|s| s.to_string()),
                    rel: self.extract_attr(attrs, "rel").map(|s| s.to_string()),
                    media: self.extract_attr(attrs, "media").map(|s| s.to_string()),
                    position: start_pos,
                });
                
                pos = tag_end + 1;
            } else {
                break;
            }
        }
    }

    /// Scan for image tags
    fn scan_images(&self, html: &[u8], result: &mut FastScanResult) {
        let mut pos = 0;
        
        while let Some(start) = memmem::find(&html[pos..], &self.patterns.img_open) {
            let start_pos = pos + start;
            let after_tag = start_pos + 4; // len("<img")
            
            if let Some(end_pos) = memmem::find(&html[after_tag..], b">") {
                let tag_end = after_tag + end_pos;
                let attrs = std::str::from_utf8(&html[after_tag..tag_end]).unwrap_or("");
                
                result.images.push(ImageTag {
                    src: self.extract_attr(attrs, "src").map(|s| s.to_string()),
                    alt: self.extract_attr(attrs, "alt").map(|s| s.to_string()),
                    position: start_pos,
                });
                
                pos = tag_end + 1;
            } else {
                break;
            }
        }
    }

    /// Scan for anchor tags
    fn scan_anchors(&self, html: &[u8], result: &mut FastScanResult) {
        let mut pos = 0;
        
        while let Some(start) = memmem::find(&html[pos..], &self.patterns.anchor_open) {
            let start_pos = pos + start;
            let after_tag = start_pos + 2; // len("<a")
            
            // Make sure it's actually an anchor tag, not <abbr> or similar
            if let Some(next_char) = html.get(after_tag) {
                if matches!(next_char, b' ' | b'\t' | b'\n' | b'\r' | b'\x3e') {
                    if let Some(end_pos) = memmem::find(&html[after_tag..], b">") {
                        let tag_end = after_tag + end_pos;
                        let attrs = std::str::from_utf8(&html[after_tag..tag_end]).unwrap_or("");
                        
                        // Extract href
                        let href = self.extract_attr(attrs, "href");
                        
                        // Try to extract text content
                        let text = if let Some(close) = memmem::find(
                            &html[tag_end + 1..], b"</a>"
                        ) {
                            let text_content = &html[tag_end + 1..tag_end + 1 + close];
                            Some(String::from_utf8_lossy(text_content).trim().to_string())
                        } else {
                            None
                        };
                        
                        result.anchors.push(AnchorTag {
                            href: href.map(|s| s.to_string()),
                            text,
                            position: start_pos,
                        });
                        
                        pos = tag_end + 1;
                    } else {
                        break;
                    }
                } else {
                    pos = after_tag;
                }
            } else {
                break;
            }
        }
    }

    /// Extract attribute value from tag attributes string
    fn extract_attr(&self, attrs: &str, name: &str) -> Option<&str> {
        let name_pattern = format!("{}=\"", name);
        if let Some(start) = attrs.to_lowercase().find(&name_pattern.to_lowercase()) {
            let after_name = start + name_pattern.len();
            if let Some(end) = attrs[after_name..].find('\"') {
                return Some(&attrs[after_name..after_name + end]);
            }
        }
        
        // Try single quotes
        let name_pattern_single = format!("{}='", name);
        if let Some(start) = attrs.to_lowercase().find(&name_pattern_single.to_lowercase()) {
            let after_name = start + name_pattern_single.len();
            if let Some(end) = attrs[after_name..].find('\'') {
                return Some(&attrs[after_name..after_name + end]);
            }
        }
        
        // Try value-less attribute (for boolean attrs)
        if let Some(pos) = attrs.to_lowercase().find(name) {
            let after = pos + name.len();
            if attrs[after..].starts_with(' ') || attrs[after..].starts_with('\t') 
                || attrs[after..].starts_with('\n') || attrs[after..].starts_with('\r')
                || attrs[after..].starts_with('\x3e') || after == attrs.len() {
                return Some("");
            }
        }
        
        None
    }

    /// Parallel scan for large documents using rayon
    #[cfg(feature = "parallel")]
    pub fn scan_parallel(&self, html: &[u8], chunk_size: usize) -> FastScanResult {
        use rayon::prelude::*;
        
        // Split HTML into chunks (with overlap for boundary handling)
        let chunks: Vec<&[u8]> = html
            .chunks(chunk_size + 1024) // +1024 for overlap
            .collect();
        
        let results: Vec<FastScanResult> = chunks
            .par_iter()
            .map(|chunk| self.scan(chunk))
            .collect();
        
        // Merge results
        self.merge_results(results)
    }

    fn merge_results(&self, results: Vec<FastScanResult>) -> FastScanResult {
        let mut merged = FastScanResult::default();
        for r in results {
            merged.scripts.extend(r.scripts);
            merged.styles.extend(r.styles);
            merged.links.extend(r.links);
            merged.images.extend(r.images);
            merged.anchors.extend(r.anchors);
            merged.estimated_element_count += r.estimated_element_count;
        }
        merged
    }
}

impl Default for FastScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fast_scanner_scripts() {
        let html = br#"
            <html>
                <head>
                    <script src="/app.js" async></script>
                    <script>console.log('inline');</script>
                </head>
            </html>
        "#;
        
        let scanner = FastScanner::new();
        let result = scanner.scan(html);
        
        assert_eq!(result.scripts.len(), 2);
        assert_eq!(result.scripts[0].src, Some("/app.js".to_string()));
        assert!(result.scripts[0].async_);
        assert_eq!(result.scripts[1].content, Some("console.log('inline');".to_string()));
    }

    #[test]
    fn test_fast_scanner_links() {
        let html = br#"
            <link rel="stylesheet" href="/style.css">
            <link rel="preconnect" href="https://cdn.example.com">
        "#;
        
        let scanner = FastScanner::new();
        let result = scanner.scan(html);
        
        assert_eq!(result.links.len(), 2);
        assert!(result.links.iter().any(|l| l.href == Some("/style.css".to_string())));
    }

    #[test]
    fn test_element_count() {
        let html = b"<html><body><div></div></body></html>";
        let scanner = FastScanner::new();
        let result = scanner.scan(html);
        assert!(result.estimated_element_count >= 5); // html, body, div, /div, /body, /html
    }
}
