//! Streaming semantic HTML parser using lol_html.
//!
//! Discovers semantic elements (links, buttons, inputs, headings, etc.) as HTML
//! chunks arrive from the network. Bytes pass through unchanged so the existing
//! full-DOM `scraper::Html` path can run afterward.

use lol_html::element;
use lol_html::AsciiCompatibleEncoding;
use lol_html::HtmlRewriter;
use lol_html::MemorySettings;
use lol_html::Settings;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Instant;
use url::Url;

use crate::semantic::extract::{
    compute_action, compute_name_from_attrs, compute_role, check_interactive, AttrMap,
};

// ---------------------------------------------------------------------------
// StreamingSemanticNode
// ---------------------------------------------------------------------------

/// Lightweight semantic element discovered during streaming HTML parsing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingSemanticNode {
    pub role: String,
    pub name: Option<String>,
    pub tag: String,
    pub interactive: bool,
    pub disabled: bool,
    pub href: Option<String>,
    pub action: Option<String>,
    pub input_type: Option<String>,
    pub placeholder: Option<String>,
    /// Discovery ordinal (1-based, monotonically increasing).
    pub ordinal: usize,
}

// ---------------------------------------------------------------------------
// StreamingParseResult / Stats
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct StreamingParseResult {
    pub body: Vec<u8>,
    pub nodes: Vec<StreamingSemanticNode>,
    pub stats: StreamingParseStats,
}

#[derive(Debug, Default)]
pub struct StreamingParseStats {
    pub bytes_processed: usize,
    pub elements_seen: usize,
    pub interactive_elements: usize,
    pub first_node_latency_us: Option<u64>,
    pub total_time_us: u64,
}

// ---------------------------------------------------------------------------
// StreamingEventSink
// ---------------------------------------------------------------------------

/// Callback for receiving streaming semantic nodes.
pub trait StreamingEventSink: Send + Sync {
    fn emit(&self, node: StreamingSemanticNode);
}

/// Collects nodes into a Vec for testing.
pub struct VecSink(pub Mutex<Vec<StreamingSemanticNode>>);

impl VecSink {
    pub fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }
}

impl StreamingEventSink for VecSink {
    fn emit(&self, node: StreamingSemanticNode) {
        self.0.lock().unwrap().push(node);
    }
}

// ---------------------------------------------------------------------------
// Internal callback state
// ---------------------------------------------------------------------------

struct CallbackState {
    base_url: String,
    pending_nodes: Vec<StreamingSemanticNode>,
    all_nodes: Vec<StreamingSemanticNode>,
    ordinal: usize,
    start: Instant,
    first_node_seen: bool,
    first_node_latency_us: Option<u64>,
}

/// Tags that never carry semantic meaning for agents.
const SKIP_TAGS: &[&str] = &[
    "script", "style", "link", "meta", "noscript", "head", "html", "body", "br", "hr",
    "col", "colgroup", "thead", "tbody", "tfoot", "tr", "td", "th", "caption", "title",
    "base", "iframe", "frame",
];

// ---------------------------------------------------------------------------
// StreamingHtmlParser
// ---------------------------------------------------------------------------

pub struct StreamingHtmlParser {
    rewriter: HtmlRewriter<'static, Box<dyn FnMut(&[u8])>>,
    output_buffer: Rc<RefCell<Vec<u8>>>,
    callback_state: Rc<RefCell<CallbackState>>,
    sink: Option<Rc<dyn StreamingEventSink>>,
}

impl StreamingHtmlParser {
    pub fn new(base_url: &str, sink: Option<Rc<dyn StreamingEventSink>>) -> anyhow::Result<Self> {
        let output_buffer = Rc::new(RefCell::new(Vec::with_capacity(64 * 1024)));
        let callback_state = Rc::new(RefCell::new(CallbackState {
            base_url: base_url.to_string(),
            pending_nodes: Vec::new(),
            all_nodes: Vec::new(),
            ordinal: 0,
            start: Instant::now(),
            first_node_seen: false,
            first_node_latency_us: None,
        }));

        let cb = callback_state.clone();

        let handler = move |el: &mut lol_html::html_content::Element| {
            let tag = el.tag_name();
            let tag_lower = tag.to_ascii_lowercase();

            if SKIP_TAGS.contains(&tag_lower.as_str()) {
                return Ok(());
            }

            let attrs: Vec<(String, String)> = el
                .attributes()
                .iter()
                .map(|a| (a.name().to_ascii_lowercase(), a.value()))
                .collect();

            let attr_map = AttrMap::new(tag_lower.clone(), attrs);

            if attr_map.attr("hidden").is_some()
                || attr_map.attr("aria-hidden") == Some("true")
            {
                return Ok(());
            }
            if tag_lower == "input" {
                if let Some(t) = attr_map.attr("type") {
                    if t.eq_ignore_ascii_case("hidden") {
                        return Ok(());
                    }
                }
            }

            let name_from_attrs = compute_name_from_attrs(&attr_map);
            let has_name = name_from_attrs.is_some();
            let role = compute_role(&tag_lower, &attr_map, has_name);
            let is_interactive = check_interactive(&tag_lower, &attr_map);
            let action = compute_action(&tag_lower, &attr_map, is_interactive);
            let is_disabled = attr_map.attr("disabled").is_some();

            let href = if tag_lower == "a" {
                attr_map.attr("href").map(|h| {
                    Url::parse(&cb.borrow().base_url)
                        .and_then(|base| base.join(&h))
                        .map(|u| u.to_string())
                        .unwrap_or_else(|_| h.to_string())
                })
            } else {
                None
            };

            let input_type = if tag_lower == "input" {
                attr_map.attr("type").map(|s| s.to_string())
            } else {
                None
            };
            let placeholder = if matches!(tag_lower.as_str(), "input" | "textarea") {
                attr_map.attr("placeholder").map(|s| s.to_string())
            } else {
                None
            };

            let mut st = cb.borrow_mut();
            st.ordinal += 1;
            if !st.first_node_seen {
                st.first_node_seen = true;
                st.first_node_latency_us = Some(st.start.elapsed().as_micros() as u64);
            }

            let node = StreamingSemanticNode {
                role: format!("{}", role),
                name: name_from_attrs,
                tag: tag_lower,
                interactive: is_interactive,
                disabled: is_disabled,
                href,
                action,
                input_type,
                placeholder,
                ordinal: st.ordinal,
            };

            st.pending_nodes.push(node.clone());
            st.all_nodes.push(node);
            Ok(())
        };

        let ob_clone = output_buffer.clone();

        let settings = Settings {
            element_content_handlers: vec![
                element!("*", handler),
            ],
            document_content_handlers: vec![],
            encoding: AsciiCompatibleEncoding::utf_8(),
            memory_settings: MemorySettings {
                max_allowed_memory_usage: 10 * 1024 * 1024,
                ..MemorySettings::default()
            },
            strict: false,
            enable_esi_tags: false,
            adjust_charset_on_meta_tag: false,
        };

        let output_sink: Box<dyn FnMut(&[u8])> = Box::new(move |chunk: &[u8]| {
            ob_clone.borrow_mut().extend_from_slice(chunk);
        });

        let rewriter = HtmlRewriter::new(settings, output_sink);

        Ok(Self {
            rewriter,
            output_buffer,
            callback_state,
            sink,
        })
    }

    /// Feed a chunk of HTML bytes. Returns new nodes discovered by this chunk.
    pub fn feed(&mut self, chunk: &[u8]) -> anyhow::Result<Vec<StreamingSemanticNode>> {
        self.rewriter.write(chunk)?;

        let new_nodes: Vec<StreamingSemanticNode> =
            self.callback_state.borrow_mut().pending_nodes.drain(..).collect();

        if let Some(ref sink) = self.sink {
            for node in &new_nodes {
                sink.emit(node.clone());
            }
        }

        Ok(new_nodes)
    }

    /// Finish the parse. Returns accumulated body, nodes, and stats.
    pub fn finish(self) -> anyhow::Result<StreamingParseResult> {
        self.rewriter.end()?;

        let st = self.callback_state.borrow();
        let ob = self.output_buffer.borrow();

        let total_time_us = st.start.elapsed().as_micros() as u64;
        let interactive = st.all_nodes.iter().filter(|n| n.interactive).count();

        let stats = StreamingParseStats {
            bytes_processed: ob.len(),
            elements_seen: st.ordinal,
            interactive_elements: interactive,
            first_node_latency_us: st.first_node_latency_us,
            total_time_us,
        };

        Ok(StreamingParseResult {
            body: ob.clone(),
            nodes: st.all_nodes.clone(),
            stats,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_streaming_parse() {
        let mut parser = StreamingHtmlParser::new("https://example.com", None).unwrap();

        let html = b"<html><body>\
            <h1>Hello World</h1>\
            <nav><a href=\"/docs\">Documentation</a></nav>\
            <main>\
              <form>\
                <input type=\"text\" name=\"email\" placeholder=\"Email\">\
                <button type=\"submit\">Subscribe</button>\
              </form>\
            </main>\
          </body></html>";

        let new = parser.feed(html).unwrap();
        assert!(!new.is_empty());

        let result = parser.finish().unwrap();
        assert_eq!(result.body.as_slice(), html);

        let interactive: Vec<_> = result.nodes.iter().filter(|n| n.interactive).collect();
        assert!(
            interactive.len() >= 3,
            "at least 3 interactive: link, input, button"
        );

        let link = result.nodes.iter().find(|n| n.tag == "a").unwrap();
        assert_eq!(link.href.as_deref(), Some("https://example.com/docs"));
        assert_eq!(link.action.as_deref(), Some("navigate"));

        let input = result.nodes.iter().find(|n| n.tag == "input").unwrap();
        assert_eq!(input.action.as_deref(), Some("fill"));
        assert_eq!(input.placeholder.as_deref(), Some("Email"));

        let button = result.nodes.iter().find(|n| n.tag == "button").unwrap();
        assert_eq!(button.action.as_deref(), Some("click"));
    }

    #[test]
    fn test_chunked_input() {
        let mut parser = StreamingHtmlParser::new("https://example.com", None).unwrap();

        let chunk1 = b"<html><body><h1>Title</h1>";
        let chunk2 = b"<a href=\"/page\">Link</a></body></html>";

        let nodes1 = parser.feed(chunk1).unwrap();
        let nodes2 = parser.feed(chunk2).unwrap();
        let result = parser.finish().unwrap();

        assert_eq!(nodes1.len() + nodes2.len(), result.nodes.len());

        let mut expected = Vec::new();
        expected.extend_from_slice(chunk1);
        expected.extend_from_slice(chunk2);
        assert_eq!(result.body, expected);
    }

    #[test]
    fn test_skips_hidden_and_script() {
        let mut parser = StreamingHtmlParser::new("https://example.com", None).unwrap();

        let html = b"<html><body>\
            <script>alert('hi')</script>\
            <style>body{color:red}</style>\
            <div hidden>Hidden</div>\
            <input type=\"hidden\" name=\"token\" value=\"abc\">\
            <button>Visible</button>\
          </body></html>";

        parser.feed(html).unwrap();
        let result = parser.finish().unwrap();

        assert!(result.nodes.iter().all(|n| n.tag != "script"));
        assert!(result.nodes.iter().all(|n| n.tag != "style"));
        let button = result.nodes.iter().find(|n| n.tag == "button");
        assert!(button.is_some());
    }

    #[test]
    fn test_ordinal_ordering() {
        let mut parser = StreamingHtmlParser::new("https://example.com", None).unwrap();

        parser
            .feed(
                b"<html><body><a href=\"/1\">First</a><button>Second</button><input type=\"text\" name=\"q\"></body></html>",
            )
            .unwrap();
        let result = parser.finish().unwrap();

        let ordinals: Vec<usize> = result.nodes.iter().map(|n| n.ordinal).collect();
        assert_eq!(ordinals, vec![1, 2, 3]);
    }

    #[test]
    fn test_vec_sink() {
        let sink = Rc::new(VecSink::new());
        let mut parser =
            StreamingHtmlParser::new("https://example.com", Some(sink.clone())).unwrap();

        parser
            .feed(b"<html><body><a href=\"/test\">Link</a><button>Click</button></body></html>")
            .unwrap();
        parser.finish().unwrap();

        let collected = sink.0.lock().unwrap();
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn test_relative_url_resolution() {
        let mut parser = StreamingHtmlParser::new("https://example.com/page", None).unwrap();

        parser
            .feed(b"<html><body><a href=\"/docs\">Docs</a><a href=\"sub\">Sub</a></body></html>")
            .unwrap();
        let result = parser.finish().unwrap();

        let links: Vec<_> = result.nodes.iter().filter(|n| n.tag == "a").collect();
        assert_eq!(links[0].href.as_deref(), Some("https://example.com/docs"));
        assert_eq!(links[1].href.as_deref(), Some("https://example.com/sub"));
    }

    #[test]
    fn test_first_node_latency() {
        let mut parser = StreamingHtmlParser::new("https://example.com", None).unwrap();
        parser
            .feed(b"<html><body><h1>Title</h1></body></html>")
            .unwrap();
        let result = parser.finish().unwrap();
        assert!(result.stats.first_node_latency_us.is_some());
        assert!(result.stats.first_node_latency_us.unwrap() < 1_000_000);
    }

    #[test]
    fn test_empty_input() {
        let mut parser = StreamingHtmlParser::new("https://example.com", None).unwrap();
        parser.feed(b"").unwrap();
        let result = parser.finish().unwrap();
        assert!(result.nodes.is_empty());
        assert!(result.stats.first_node_latency_us.is_none());
    }
}
