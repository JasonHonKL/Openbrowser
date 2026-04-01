use scraper::{Html, Selector};
use std::collections::HashSet;
use url::Url;

use crate::record::{Initiator, NetworkRecord, ResourceType};

pub fn discover_subresources(html: &Html, base_url: &str, start_id: usize) -> Vec<NetworkRecord> {
    let mut records = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let base = Url::parse(base_url).ok();

    let mut try_add =
        |url_str: &str, resource_type: ResourceType, initiator: Initiator, desc: &str| {
            let resolved = if let Some(ref base) = base {
                base.join(url_str)
                    .map(|u| u.to_string())
                    .unwrap_or_else(|_| url_str.to_string())
            } else {
                url_str.to_string()
            };

            if seen.insert(resolved.clone()) {
                let id = start_id + records.len();
                records.push(NetworkRecord::discovered(
                    id,
                    resource_type,
                    desc.to_string(),
                    resolved,
                    initiator,
                ));
            }
        };

    // <link rel="stylesheet" href="...">
    if let Ok(sel) = Selector::parse(r#"link[rel="stylesheet"]"#) {
        for el in html.select(&sel) {
            if let Some(href) = el.value().attr("href") {
                let desc = extract_filename(href);
                try_add(href, ResourceType::Stylesheet, Initiator::Link, &desc);
            }
        }
    }

    // <link rel="preload" href="..." as="...">
    if let Ok(sel) = Selector::parse(r#"link[rel="preload"]"#) {
        for el in html.select(&sel) {
            if let Some(href) = el.value().attr("href") {
                let as_attr = el.value().attr("as").unwrap_or("");
                let (rt, init) = match as_attr {
                    "script" => (ResourceType::Script, Initiator::Link),
                    "style" => (ResourceType::Stylesheet, Initiator::Link),
                    "font" => (ResourceType::Font, Initiator::Link),
                    "image" => (ResourceType::Image, Initiator::Link),
                    _ => (ResourceType::Other, Initiator::Link),
                };
                let desc = extract_filename(href);
                try_add(href, rt, init, &desc);
            }
        }
    }

    // <link rel="icon" href="...">, <link rel="shortcut icon" href="...">
    if let Ok(sel) = Selector::parse(r#"link[rel~="icon"]"#) {
        for el in html.select(&sel) {
            if let Some(href) = el.value().attr("href") {
                let desc = extract_filename(href);
                try_add(href, ResourceType::Other, Initiator::Link, &desc);
            }
        }
    }

    // Other <link> tags with href
    if let Ok(sel) = Selector::parse("link[href]") {
        for el in html.select(&sel) {
            let rel = el.value().attr("rel").unwrap_or("").to_lowercase();
            if rel.contains("stylesheet")
                || rel.contains("preload")
                || rel.contains("icon")
                || rel.contains("canonical")
            {
                continue;
            }
            if let Some(href) = el.value().attr("href") {
                let desc = extract_filename(href);
                try_add(href, ResourceType::Other, Initiator::Link, &desc);
            }
        }
    }

    // <script src="...">
    if let Ok(sel) = Selector::parse("script[src]") {
        for el in html.select(&sel) {
            if let Some(src) = el.value().attr("src") {
                let desc = extract_filename(src);
                try_add(src, ResourceType::Script, Initiator::Script, &desc);
            }
        }
    }

    // <img src="...">
    if let Ok(sel) = Selector::parse("img[src]") {
        for el in html.select(&sel) {
            if let Some(src) = el.value().attr("src") {
                let desc = extract_filename(src);
                try_add(src, ResourceType::Image, Initiator::Img, &desc);
            }
            // srcset
            if let Some(srcset) = el.value().attr("srcset") {
                for part in srcset.split(',') {
                    let url_candidate = part.split_whitespace().next().unwrap_or("");
                    if !url_candidate.is_empty() {
                        let desc = extract_filename(url_candidate);
                        try_add(url_candidate, ResourceType::Image, Initiator::Img, &desc);
                    }
                }
            }
        }
    }

    // <img srcset="..."> (without src)
    if let Ok(sel) = Selector::parse("img[srcset]:not([src])") {
        for el in html.select(&sel) {
            if let Some(srcset) = el.value().attr("srcset") {
                for part in srcset.split(',') {
                    let url_candidate = part.split_whitespace().next().unwrap_or("");
                    if !url_candidate.is_empty() {
                        let desc = extract_filename(url_candidate);
                        try_add(url_candidate, ResourceType::Image, Initiator::Img, &desc);
                    }
                }
            }
        }
    }

    // <picture><source srcset="...">
    if let Ok(sel) = Selector::parse("picture source[srcset]") {
        for el in html.select(&sel) {
            if let Some(srcset) = el.value().attr("srcset") {
                for part in srcset.split(',') {
                    let url_candidate = part.split_whitespace().next().unwrap_or("");
                    if !url_candidate.is_empty() {
                        let desc = extract_filename(url_candidate);
                        try_add(url_candidate, ResourceType::Image, Initiator::Parser, &desc);
                    }
                }
            }
        }
    }

    // <video src="...">, <video><source src="...">
    if let Ok(sel) = Selector::parse("video[src]") {
        for el in html.select(&sel) {
            if let Some(src) = el.value().attr("src") {
                let desc = extract_filename(src);
                try_add(src, ResourceType::Media, Initiator::Parser, &desc);
            }
        }
    }
    if let Ok(sel) = Selector::parse("video source[src]") {
        for el in html.select(&sel) {
            if let Some(src) = el.value().attr("src") {
                let desc = extract_filename(src);
                try_add(src, ResourceType::Media, Initiator::Parser, &desc);
            }
        }
    }

    // <audio src="...">, <audio><source src="...">
    if let Ok(sel) = Selector::parse("audio[src]") {
        for el in html.select(&sel) {
            if let Some(src) = el.value().attr("src") {
                let desc = extract_filename(src);
                try_add(src, ResourceType::Media, Initiator::Parser, &desc);
            }
        }
    }
    if let Ok(sel) = Selector::parse("audio source[src]") {
        for el in html.select(&sel) {
            if let Some(src) = el.value().attr("src") {
                let desc = extract_filename(src);
                try_add(src, ResourceType::Media, Initiator::Parser, &desc);
            }
        }
    }

    // <iframe src="...">
    if let Ok(sel) = Selector::parse("iframe[src]") {
        for el in html.select(&sel) {
            if let Some(src) = el.value().attr("src") {
                let desc = extract_filename(src);
                try_add(src, ResourceType::Document, Initiator::Parser, &desc);
            }
        }
    }

    // <embed src="...">
    if let Ok(sel) = Selector::parse("embed[src]") {
        for el in html.select(&sel) {
            if let Some(src) = el.value().attr("src") {
                let desc = extract_filename(src);
                try_add(src, ResourceType::Other, Initiator::Parser, &desc);
            }
        }
    }

    // <object data="...">
    if let Ok(sel) = Selector::parse("object[data]") {
        for el in html.select(&sel) {
            if let Some(data) = el.value().attr("data") {
                let desc = extract_filename(data);
                try_add(data, ResourceType::Other, Initiator::Parser, &desc);
            }
        }
    }

    // Inline CSS url() references for fonts
    if let Ok(sel) = Selector::parse("style") {
        for el in html.select(&sel) {
            let css: String = el.text().collect();
            extract_urls_from_css(&css, &mut seen, &mut records, start_id, &base);
        }
    }

    records
}

fn extract_urls_from_css(
    css: &str,
    seen: &mut HashSet<String>,
    records: &mut Vec<NetworkRecord>,
    start_id: usize,
    base: &Option<Url>,
) {
    let lower = css.to_lowercase();
    let mut search_from = 0;

    while let Some(pos) = lower[search_from..].find("url(") {
        let abs_pos = search_from + pos;
        let paren_start = abs_pos + 4;

        if let Some(close) = css[paren_start..].find(')') {
            let url_str = css[paren_start..paren_start + close]
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .trim();

            if !url_str.is_empty() && !url_str.starts_with("data:") && !url_str.starts_with("DATA:")
            {
                let resolved = if let Some(base) = base {
                    base.join(url_str)
                        .map(|u| u.to_string())
                        .unwrap_or_else(|_| url_str.to_string())
                } else {
                    url_str.to_string()
                };

                if seen.insert(resolved.clone()) {
                    let rt = if resolved.ends_with(".woff2")
                        || resolved.ends_with(".woff")
                        || resolved.ends_with(".ttf")
                        || resolved.ends_with(".otf")
                        || resolved.ends_with(".eot")
                    {
                        ResourceType::Font
                    } else {
                        ResourceType::Other
                    };
                    let id = start_id + records.len();
                    let desc = extract_filename(url_str);
                    records.push(NetworkRecord::discovered(
                        id,
                        rt,
                        desc,
                        resolved,
                        Initiator::Parser,
                    ));
                }
            }

            search_from = paren_start + close + 1;
        } else {
            break;
        }
    }
}

fn extract_filename(url: &str) -> String {
    let path = url.split('?').next().unwrap_or(url);
    let path = path.split('#').next().unwrap_or(path);
    let name = path.rsplit('/').next().unwrap_or(url);
    if name.is_empty() || name == "/" {
        url.to_string()
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://example.com";

    fn parse(html: &str) -> Html {
        Html::parse_document(html)
    }

    // -- extract_filename --
    #[test]
    fn test_extract_filename_simple() {
        assert_eq!(
            extract_filename("https://example.com/styles.css"),
            "styles.css"
        );
    }

    #[test]
    fn test_extract_filename_with_query() {
        assert_eq!(extract_filename("https://example.com/app.js?v=2"), "app.js");
    }

    #[test]
    fn test_extract_filename_with_hash() {
        assert_eq!(extract_filename("https://example.com/page#section"), "page");
    }

    #[test]
    fn test_extract_filename_nested_path() {
        assert_eq!(
            extract_filename("https://cdn.example.com/css/fonts/v2/main.woff2"),
            "main.woff2"
        );
    }

    #[test]
    fn test_extract_filename_root_url() {
        assert_eq!(
            extract_filename("https://example.com/"),
            "https://example.com/"
        );
    }

    #[test]
    fn test_extract_filename_no_path() {
        assert_eq!(extract_filename("https://example.com"), "example.com");
    }

    #[test]
    fn test_extract_filename_bare_filename() {
        assert_eq!(extract_filename("script.js"), "script.js");
    }

    #[test]
    fn test_extract_filename_query_and_fragment() {
        assert_eq!(
            extract_filename("https://example.com/bundle.min.css?v=3#hash"),
            "bundle.min.css"
        );
    }

    // -- discover: empty HTML --
    #[test]
    fn test_discover_empty() {
        let html = parse("<html><body></body></html>");
        let records = discover_subresources(&html, BASE, 1);
        assert!(records.is_empty());
    }

    // -- discover: <link rel="stylesheet"> --
    #[test]
    fn test_discover_stylesheet() {
        let html = parse(r#"<html><head><link rel="stylesheet" href="/styles.css"></head></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Stylesheet);
        assert_eq!(records[0].initiator, Initiator::Link);
        assert_eq!(records[0].url, "https://example.com/styles.css");
        assert_eq!(records[0].description, "styles.css");
    }

    #[test]
    fn test_discover_multiple_stylesheets() {
        let html = parse(
            r#"<html><head>
            <link rel="stylesheet" href="/main.css">
            <link rel="stylesheet" href="https://cdn.example.com/reset.css">
        </head></html>"#,
        );
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].url, "https://example.com/main.css");
        assert_eq!(records[1].url, "https://cdn.example.com/reset.css");
    }

    // -- discover: <link rel="preload"> --
    #[test]
    fn test_discover_preload_script() {
        let html =
            parse(r#"<html><head><link rel="preload" href="/app.js" as="script"></head></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Script);
        assert_eq!(records[0].url, "https://example.com/app.js");
    }

    #[test]
    fn test_discover_preload_font() {
        let html =
            parse(r#"<html><head><link rel="preload" href="/font.woff2" as="font"></head></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Font);
    }

    #[test]
    fn test_discover_preload_image() {
        let html =
            parse(r#"<html><head><link rel="preload" href="/hero.jpg" as="image"></head></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Image);
    }

    #[test]
    fn test_discover_preload_style() {
        let html = parse(
            r#"<html><head><link rel="preload" href="/critical.css" as="style"></head></html>"#,
        );
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Stylesheet);
    }

    #[test]
    fn test_discover_preload_unknown_as() {
        let html =
            parse(r#"<html><head><link rel="preload" href="/data.json" as="fetch"></head></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Other);
    }

    // -- discover: <link rel="icon"> --
    #[test]
    fn test_discover_icon() {
        let html = parse(r#"<html><head><link rel="icon" href="/favicon.ico"></head></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Other);
        assert_eq!(records[0].url, "https://example.com/favicon.ico");
    }

    #[test]
    fn test_discover_shortcut_icon() {
        let html =
            parse(r#"<html><head><link rel="shortcut icon" href="/icon.png"></head></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].url, "https://example.com/icon.png");
    }

    // -- discover: canonical skipped --
    #[test]
    fn test_discover_canonical_skipped() {
        let html = parse(
            r#"<html><head><link rel="canonical" href="https://example.com/page"></head></html>"#,
        );
        let records = discover_subresources(&html, BASE, 1);
        assert!(records.is_empty());
    }

    // -- discover: other <link> with unknown rel --
    #[test]
    fn test_discover_other_link() {
        let html = parse(
            r#"<html><head><link rel="preload" href="/manifest.json" crossorigin="use-credentials"></head></html>"#,
        );
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].url, "https://example.com/manifest.json");
    }

    // -- discover: <script src> --
    #[test]
    fn test_discover_script() {
        let html = parse(r#"<html><body><script src="/app.js"></script></body></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Script);
        assert_eq!(records[0].initiator, Initiator::Script);
        assert_eq!(records[0].url, "https://example.com/app.js");
    }

    #[test]
    fn test_discover_inline_script_ignored() {
        let html = parse(r#"<html><body><script>console.log('hi');</script></body></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert!(records.is_empty());
    }

    // -- discover: <img src> --
    #[test]
    fn test_discover_image() {
        let html = parse(r#"<html><body><img src="/logo.png"></body></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Image);
        assert_eq!(records[0].initiator, Initiator::Img);
        assert_eq!(records[0].url, "https://example.com/logo.png");
    }

    #[test]
    fn test_discover_image_srcset() {
        let html =
            parse(r#"<html><body><img srcset="/small.jpg 1x, /large.jpg 2x"></body></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].url, "https://example.com/small.jpg");
        assert_eq!(records[1].url, "https://example.com/large.jpg");
    }

    #[test]
    fn test_discover_image_src_and_srcset() {
        let html =
            parse(r#"<html><body><img src="/fallback.jpg" srcset="/retina.jpg 2x"></body></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 2);
    }

    // -- discover: <picture><source srcset> --
    #[test]
    fn test_discover_picture_source() {
        let html = parse(
            r#"<html><body><picture><source srcset="/avif.webp" type="image/avif"></picture></body></html>"#,
        );
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Image);
        assert_eq!(records[0].initiator, Initiator::Parser);
        assert_eq!(records[0].url, "https://example.com/avif.webp");
    }

    // -- discover: <video> --
    #[test]
    fn test_discover_video_src() {
        let html = parse(r#"<html><body><video src="/clip.mp4"></video></body></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Media);
        assert_eq!(records[0].initiator, Initiator::Parser);
        assert_eq!(records[0].url, "https://example.com/clip.mp4");
    }

    #[test]
    fn test_discover_video_source() {
        let html = parse(r#"<html><body><video><source src="/video.webm"></video></body></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Media);
    }

    // -- discover: <audio> --
    #[test]
    fn test_discover_audio_src() {
        let html = parse(r#"<html><body><audio src="/song.mp3"></audio></body></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Media);
    }

    #[test]
    fn test_discover_audio_source() {
        let html = parse(r#"<html><body><audio><source src="/audio.ogg"></audio></body></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Media);
    }

    // -- discover: <iframe> --
    #[test]
    fn test_discover_iframe() {
        let html = parse(r#"<html><body><iframe src="/embedded.html"></iframe></body></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Document);
        assert_eq!(records[0].initiator, Initiator::Parser);
        assert_eq!(records[0].url, "https://example.com/embedded.html");
    }

    // -- discover: <embed> --
    #[test]
    fn test_discover_embed() {
        let html = parse(r#"<html><body><embed src="/widget.swf"></body></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Other);
        assert_eq!(records[0].url, "https://example.com/widget.swf");
    }

    // -- discover: <object data> --
    #[test]
    fn test_discover_object() {
        let html = parse(r#"<html><body><object data="/content.pdf"></object></body></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Other);
        assert_eq!(records[0].url, "https://example.com/content.pdf");
    }

    // -- discover: <style> with CSS url() --
    #[test]
    fn test_discover_css_font_url() {
        let html = parse(
            r#"<html><head><style>@font-face { font-family: 'Test'; src: url('/fonts/test.woff2') format('woff2'); }</style></head></html>"#,
        );
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Font);
        assert_eq!(records[0].url, "https://example.com/fonts/test.woff2");
        assert_eq!(records[0].initiator, Initiator::Parser);
    }

    #[test]
    fn test_discover_css_background_image() {
        let html = parse(
            r#"<html><head><style>body { background: url('/bg.png'); }</style></head></html>"#,
        );
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].resource_type, ResourceType::Other);
        assert_eq!(records[0].url, "https://example.com/bg.png");
    }

    #[test]
    fn test_discover_css_data_url_skipped() {
        let html = parse(
            r#"<html><head><style>body { background: url('data:image/png;base64,abc'); }</style></head></html>"#,
        );
        let records = discover_subresources(&html, BASE, 1);
        assert!(records.is_empty());
    }

    #[test]
    fn test_discover_css_multiple_fonts() {
        let html = parse(
            r#"<html><head><style>
            @font-face { src: url('/fonts/regular.woff2'); }
            @font-face { src: url('/fonts/bold.woff'); }
        </style></head></html>"#,
        );
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 2);
        assert!(records
            .iter()
            .all(|r| r.resource_type == ResourceType::Font));
    }

    #[test]
    fn test_discover_css_font_type_detection() {
        let cases = vec![
            ("test.woff2", ResourceType::Font),
            ("test.woff", ResourceType::Font),
            ("test.ttf", ResourceType::Font),
            ("test.otf", ResourceType::Font),
            ("test.eot", ResourceType::Font),
            ("test.png", ResourceType::Other),
            ("test.jpg", ResourceType::Other),
            ("test.svg", ResourceType::Other),
        ];
        for (ext, expected) in cases {
            let html = parse(&format!(
                r#"<html><head><style>body {{ background: url('/img.{}'); }}</style></head></html>"#,
                ext
            ));
            let records = discover_subresources(&html, BASE, 1);
            assert_eq!(records.len(), 1, "Expected 1 record for {ext}");
            assert_eq!(records[0].resource_type, expected, "Wrong type for {ext}");
        }
    }

    // -- discover: dedup --
    #[test]
    fn test_discover_dedup() {
        let html = parse(
            r#"<html><head>
            <link rel="stylesheet" href="/main.css">
            <link rel="stylesheet" href="/main.css">
        </head></html>"#,
        );
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn test_discover_dedup_across_selectors() {
        let html = parse(
            r#"<html><head>
            <link rel="preload" href="/app.js" as="script">
        </head><body>
            <script src="/app.js"></script>
        </body></html>"#,
        );
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
    }

    // -- discover: start_id --
    #[test]
    fn test_discover_start_id() {
        let html = parse(
            r#"<html><body><script src="/a.js"></script><script src="/b.js"></script></body></html>"#,
        );
        let records = discover_subresources(&html, BASE, 10);
        assert_eq!(records[0].id, 10);
        assert_eq!(records[1].id, 11);
    }

    // -- discover: URL resolution with query params --
    #[test]
    fn test_discover_url_with_query() {
        let html = parse(
            r#"<html><head><link rel="stylesheet" href="/styles.css?v=2&hash=abc"></head></html>"#,
        );
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].url,
            "https://example.com/styles.css?v=2&hash=abc"
        );
        assert_eq!(records[0].description, "styles.css");
    }

    // -- discover: full page --
    #[test]
    fn test_discover_full_page() {
        let html = parse(
            r#"<html><head>
            <link rel="stylesheet" href="/main.css">
            <link rel="icon" href="/favicon.ico">
            <link rel="preload" href="/font.woff2" as="font">
            <script src="/app.js"></script>
            <style>@font-face { src: url('/fallback.woff'); }</style>
        </head><body>
            <img src="/logo.png" srcset="/logo@2x.png 2x">
            <iframe src="/widget.html"></iframe>
            <video><source src="/video.mp4"></video>
        </body></html>"#,
        );
        let records = discover_subresources(&html, BASE, 1);
        let urls: Vec<&str> = records.iter().map(|r| r.url.as_str()).collect();
        assert!(urls.contains(&"https://example.com/main.css"));
        assert!(urls.contains(&"https://example.com/favicon.ico"));
        assert!(urls.contains(&"https://example.com/font.woff2"));
        assert!(urls.contains(&"https://example.com/app.js"));
        assert!(urls.contains(&"https://example.com/fallback.woff"));
        assert!(urls.contains(&"https://example.com/logo.png"));
        assert!(urls.contains(&"https://example.com/logo@2x.png"));
        assert!(urls.contains(&"https://example.com/widget.html"));
        assert!(urls.contains(&"https://example.com/video.mp4"));
    }

    // -- discover: invalid base URL --
    #[test]
    fn test_discover_invalid_base_url() {
        let html = parse(r#"<html><body><script src="/app.js"></script></body></html>"#);
        let records = discover_subresources(&html, "not-a-url", 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].url, "/app.js");
    }

    // -- extract_urls_from_css edge cases --
    #[test]
    fn test_css_empty_parentheses_skipped() {
        let html = parse(r#"<html><head><style>body { color: rgb(); }</style></head></html>"#);
        let records = discover_subresources(&html, BASE, 1);
        assert!(records.is_empty());
    }

    #[test]
    fn test_css_data_url_variants_skipped() {
        let html = parse(
            r#"<html><head><style>
            body { background: url("data:image/svg+xml,<svg></svg>"); }
            img { content: url('DATA:skip'); }
        </style></head></html>"#,
        );
        let records = discover_subresources(&html, BASE, 1);
        assert!(records.is_empty());
    }

    #[test]
    fn test_css_dedup() {
        let html = parse(
            r#"<html><head><style>
            body { background: url('/bg.png'); }
            div { background: url('/bg.png'); }
        </style></head></html>"#,
        );
        let records = discover_subresources(&html, BASE, 1);
        assert_eq!(records.len(), 1);
    }
}
