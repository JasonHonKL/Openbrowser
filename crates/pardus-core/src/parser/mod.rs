//! High-performance streaming HTML parser
//!
//! Uses lol-html for streaming transformations and html5ever for full DOM construction.
//! Implements arena allocation and lazy parsing for memory efficiency.

pub mod streaming;
pub mod arena_dom;
pub mod lazy;
pub mod preload_scanner;
pub mod streaming_semantic;

pub use streaming::{StreamingParser, ParseOptions, ParseResult};
pub use arena_dom::{ArenaDom, Node, NodeId, NodeType};
pub use lazy::{LazyHtml, LazyParse, LazyDom};
pub use preload_scanner::{PreloadScanner, ResourceHint, ResourceType, Priority};
pub use streaming_semantic::{StreamingEventSink, StreamingParseStats};

use bytes::Bytes;
use std::sync::Arc;

/// Unified parser that can switch between streaming and full DOM modes
#[derive(Debug, Clone)]
pub struct UnifiedParser {
    options: ParseOptions,
}

impl UnifiedParser {
    pub fn new(options: ParseOptions) -> Self {
        Self { options }
    }

    /// Parse with automatic mode selection based on content size
    pub fn parse(&self, html: Bytes, url: &str) -> ParseResult {
        if html.len() > self.options.streaming_threshold {
            StreamingParser::new(self.options.clone())
                .parse_streaming(html, url)
        } else {
            StreamingParser::new(self.options.clone())
                .parse_full(html, url)
        }
    }

    /// Fast path for small documents
    pub fn parse_small(&self, html: &str) -> anyhow::Result<Arc<LazyDom>> {
        LazyDom::parse(html)
    }
}

impl Default for UnifiedParser {
    fn default() -> Self {
        Self::new(ParseOptions::default())
    }
}

/// Size thresholds for parser mode selection
impl ParseOptions {
    pub const SMALL_DOC_THRESHOLD: usize = 50_000;      // 50KB - fast parse
    pub const STREAMING_THRESHOLD: usize = 500_000;   // 500KB - streaming mode
    pub const LARGE_DOC_THRESHOLD: usize = 5_000_000; // 5MB - use arena + lazy
}
