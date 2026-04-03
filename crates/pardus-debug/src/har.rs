use serde::Serialize;
use url::Url;

use crate::record::NetworkLog;

// ---------------------------------------------------------------------------
// HAR 1.2 spec types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct HarFile {
    pub log: HarLog,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarLog {
    pub version: String,
    pub creator: HarCreator,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<HarPage>,
    pub entries: Vec<HarEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarCreator {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarPage {
    pub started_date_time: String,
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pageref: Option<String>,
    pub started_date_time: String,
    pub time: f64,
    pub request: HarRequest,
    pub response: HarResponse,
    pub timings: HarTimings,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_ip_address: Option<String>,
    pub connection: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarRequest {
    pub method: String,
    pub url: String,
    pub http_version: String,
    pub headers: Vec<HarHeader>,
    pub query_string: Vec<HarNameValuePair>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_data: Option<HarPostData>,
    pub headers_size: i64,
    pub body_size: i64,
    pub cookies: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarResponse {
    pub status: u16,
    pub status_text: String,
    pub http_version: String,
    pub headers: Vec<HarHeader>,
    pub content: HarContent,
    pub redirect_url: String,
    pub headers_size: i64,
    pub body_size: i64,
    pub cookies: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarTimings {
    pub blocked: f64,
    pub dns: f64,
    pub connect: f64,
    pub send: f64,
    pub wait: f64,
    pub receive: f64,
    pub ssl: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarNameValuePair {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarPostData {
    pub mime_type: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarContent {
    pub size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

impl HarFile {
    /// Convert a NetworkLog into a HAR 1.2 file.
    pub fn from_network_log(log: &NetworkLog) -> Self {
        let entries: Vec<HarEntry> = log.records.iter().map(|r| {
            let started = r.started_at.clone().unwrap_or_else(|| "unknown".to_string());
            let total_time = r.timing_ms.unwrap_or(0) as f64;
            let http_ver = r.http_version.clone().unwrap_or_else(|| "HTTP/1.1".to_string());

            let req_headers: Vec<HarHeader> = r.request_headers.iter()
                .map(|(k, v)| HarHeader { name: k.clone(), value: v.clone() })
                .collect();

            let resp_headers: Vec<HarHeader> = r.response_headers.iter()
                .map(|(k, v)| HarHeader { name: k.clone(), value: v.clone() })
                .collect();

            let query_string = parse_query_string(&r.url);

            HarEntry {
                pageref: None,
                started_date_time: started,
                time: total_time,
                request: HarRequest {
                    method: r.method.clone(),
                    url: r.url.clone(),
                    http_version: http_ver.clone(),
                    headers: req_headers,
                    query_string,
                    post_data: None,
                    headers_size: -1,
                    body_size: -1,
                    cookies: vec![],
                },
                response: HarResponse {
                    status: r.status.unwrap_or(0),
                    status_text: r.status_text.clone().unwrap_or_default(),
                    http_version: http_ver,
                    headers: resp_headers,
                    content: HarContent {
                        size: r.body_size.unwrap_or(0) as i64,
                        mime_type: r.content_type.clone(),
                    },
                    redirect_url: r.redirect_url.clone().unwrap_or_default(),
                    headers_size: -1,
                    body_size: r.body_size.unwrap_or(0) as i64,
                    cookies: vec![],
                },
                timings: HarTimings {
                    blocked: -1.0,
                    dns: -1.0,
                    connect: -1.0,
                    send: 0.0,
                    wait: total_time,
                    receive: -1.0,
                    ssl: -1.0,
                },
                server_ip_address: None,
                connection: r.id.to_string(),
            }
        }).collect();

        let creator_version = env!("CARGO_PKG_VERSION").to_string();

        Self {
            log: HarLog {
                version: "1.2".to_string(),
                creator: HarCreator {
                    name: "PardusBrowser".to_string(),
                    version: creator_version,
                },
                pages: vec![],
                entries,
            },
        }
    }
}

fn parse_query_string(raw_url: &str) -> Vec<HarNameValuePair> {
    let Ok(parsed) = Url::parse(raw_url) else {
        return vec![];
    };
    parsed.query_pairs()
        .map(|(k, v)| HarNameValuePair {
            name: k.to_string(),
            value: v.to_string(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{Initiator, NetworkRecord, ResourceType};

    fn make_log_with_one_record() -> NetworkLog {
        let mut log = NetworkLog::new();
        let mut r = NetworkRecord::fetched(
            1,
            "GET".into(),
            ResourceType::Document,
            "navigation".into(),
            "https://example.com/page?q=hello".into(),
            Initiator::Navigation,
        );
        r.status = Some(200);
        r.status_text = Some("OK".into());
        r.content_type = Some("text/html".into());
        r.body_size = Some(4096);
        r.timing_ms = Some(150);
        r.started_at = Some("2025-01-01T00:00:00.000Z".into());
        r.http_version = Some("HTTP/2".into());
        r.response_headers.push(("content-type".into(), "text/html".into()));
        log.push(r);
        log
    }

    #[test]
    fn test_har_from_empty_log() {
        let log = NetworkLog::new();
        let har = HarFile::from_network_log(&log);
        assert!(har.log.entries.is_empty());
        assert_eq!(har.log.version, "1.2");
        assert_eq!(har.log.creator.name, "PardusBrowser");
    }

    #[test]
    fn test_har_from_log_with_record() {
        let log = make_log_with_one_record();
        let har = HarFile::from_network_log(&log);

        assert_eq!(har.log.entries.len(), 1);
        let entry = &har.log.entries[0];

        assert_eq!(entry.started_date_time, "2025-01-01T00:00:00.000Z");
        assert_eq!(entry.time, 150.0);
        assert_eq!(entry.request.method, "GET");
        assert_eq!(entry.request.url, "https://example.com/page?q=hello");
        assert_eq!(entry.request.http_version, "HTTP/2");
        assert_eq!(entry.response.status, 200);
        assert_eq!(entry.response.status_text, "OK");
        assert_eq!(entry.response.content.size, 4096);
        assert_eq!(entry.response.content.mime_type, Some("text/html".into()));
        assert_eq!(entry.response.headers.len(), 1);
        assert_eq!(entry.response.headers[0].name, "content-type");
        assert_eq!(entry.timings.wait, 150.0);
        assert_eq!(entry.timings.blocked, -1.0);
    }

    #[test]
    fn test_har_query_string_parsed() {
        let log = make_log_with_one_record();
        let har = HarFile::from_network_log(&log);
        let qs = &har.log.entries[0].request.query_string;

        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].name, "q");
        assert_eq!(qs[0].value, "hello");
    }

    #[test]
    fn test_har_record_without_timing() {
        let mut log = NetworkLog::new();
        let r = NetworkRecord::fetched(
            1, "GET".into(), ResourceType::Document, "nav".into(),
            "https://example.com".into(), Initiator::Navigation,
        );
        log.push(r);

        let har = HarFile::from_network_log(&log);
        let entry = &har.log.entries[0];
        assert_eq!(entry.time, 0.0);
        assert_eq!(entry.started_date_time, "unknown");
        assert_eq!(entry.request.http_version, "HTTP/1.1"); // default
        assert_eq!(entry.response.status, 0); // no status set
    }

    #[test]
    fn test_har_serializes_to_valid_json() {
        let log = make_log_with_one_record();
        let har = HarFile::from_network_log(&log);
        let json = serde_json::to_string(&har).unwrap();

        assert!(json.contains(r#""version":"1.2""#));
        assert!(json.contains(r#""creator""#));
        assert!(json.contains(r#""entries""#));
        assert!(json.contains(r#""request""#));
        assert!(json.contains(r#""response""#));
        assert!(json.contains(r#""timings""#));
        // optional fields should be skipped when default
        assert!(!json.contains("pageref"));
        assert!(!json.contains("serverIpAddress"));
    }

    #[test]
    fn test_har_multiple_entries() {
        let mut log = NetworkLog::new();

        let mut r1 = NetworkRecord::fetched(
            1, "GET".into(), ResourceType::Document, "navigation".into(),
            "https://example.com/".into(), Initiator::Navigation,
        );
        r1.status = Some(200);
        r1.status_text = Some("OK".into());
        r1.started_at = Some("2025-01-01T00:00:00.000Z".into());
        log.push(r1);

        let mut r2 = NetworkRecord::fetched(
            2, "GET".into(), ResourceType::Stylesheet, "stylesheet".into(),
            "https://example.com/style.css".into(), Initiator::Link,
        );
        r2.status = Some(304);
        r2.status_text = Some("Not Modified".into());
        r2.started_at = Some("2025-01-01T00:00:01.000Z".into());
        log.push(r2);

        let mut r3 = NetworkRecord::fetched(
            3, "GET".into(), ResourceType::Script, "script".into(),
            "https://example.com/app.js".into(), Initiator::Script,
        );
        r3.status = Some(404);
        r3.status_text = Some("Not Found".into());
        r3.started_at = Some("2025-01-01T00:00:02.000Z".into());
        log.push(r3);

        let har = HarFile::from_network_log(&log);
        assert_eq!(har.log.entries.len(), 3);

        assert_eq!(har.log.entries[0].response.status, 200);
        assert_eq!(har.log.entries[0].response.status_text, "OK");

        assert_eq!(har.log.entries[1].response.status, 304);
        assert_eq!(har.log.entries[1].response.status_text, "Not Modified");

        assert_eq!(har.log.entries[2].response.status, 404);
        assert_eq!(har.log.entries[2].response.status_text, "Not Found");
    }

    #[test]
    fn test_har_error_record() {
        let mut log = NetworkLog::new();
        let mut r = NetworkRecord::fetched(
            1, "GET".into(), ResourceType::Script, "script".into(),
            "https://example.com/fail.js".into(), Initiator::Script,
        );
        r.error = Some("connection refused".into());
        r.started_at = Some("2025-01-01T00:00:00.000Z".into());
        log.push(r);

        let har = HarFile::from_network_log(&log);
        assert_eq!(har.log.entries.len(), 1);
        let entry = &har.log.entries[0];

        // status is None, so status.unwrap_or(0) => 0
        assert_eq!(entry.response.status, 0);
        assert_eq!(entry.response.status_text, "");
        // The error is stored on the record itself; the HAR entry just gets
        // status 0 because no status was set on the record.
    }

    #[test]
    fn test_har_redirect_entry() {
        let mut log = NetworkLog::new();
        let mut r = NetworkRecord::fetched(
            1, "GET".into(), ResourceType::Document, "navigation".into(),
            "https://example.com/old-page".into(), Initiator::Navigation,
        );
        r.status = Some(301);
        r.status_text = Some("Moved Permanently".into());
        r.redirect_url = Some("https://example.com/new-page".into());
        r.started_at = Some("2025-01-01T00:00:00.000Z".into());
        log.push(r);

        let har = HarFile::from_network_log(&log);
        let entry = &har.log.entries[0];
        assert_eq!(entry.response.status, 301);
        assert_eq!(entry.response.redirect_url, "https://example.com/new-page");
    }

    #[test]
    fn test_har_request_headers_populated() {
        let mut log = NetworkLog::new();
        let mut r = NetworkRecord::fetched(
            1, "GET".into(), ResourceType::Document, "navigation".into(),
            "https://example.com/".into(), Initiator::Navigation,
        );
        r.status = Some(200);
        r.request_headers.push(("accept".into(), "text/html".into()));
        r.request_headers.push(("user-agent".into(), "PardusBrowser/1.0".into()));
        r.response_headers.push(("content-type".into(), "text/html".into()));
        r.response_headers.push(("server".into(), "nginx".into()));
        log.push(r);

        let har = HarFile::from_network_log(&log);
        let entry = &har.log.entries[0];

        assert_eq!(entry.request.headers.len(), 2);
        assert_eq!(entry.request.headers[0].name, "accept");
        assert_eq!(entry.request.headers[0].value, "text/html");
        assert_eq!(entry.request.headers[1].name, "user-agent");
        assert_eq!(entry.request.headers[1].value, "PardusBrowser/1.0");

        assert_eq!(entry.response.headers.len(), 2);
        assert_eq!(entry.response.headers[0].name, "content-type");
        assert_eq!(entry.response.headers[0].value, "text/html");
        assert_eq!(entry.response.headers[1].name, "server");
        assert_eq!(entry.response.headers[1].value, "nginx");
    }

    #[test]
    fn test_har_no_query_string() {
        let mut log = NetworkLog::new();
        let mut r = NetworkRecord::fetched(
            1, "GET".into(), ResourceType::Document, "navigation".into(),
            "https://example.com/page".into(), Initiator::Navigation,
        );
        r.status = Some(200);
        log.push(r);

        let har = HarFile::from_network_log(&log);
        let qs = &har.log.entries[0].request.query_string;
        assert!(qs.is_empty());
    }

    #[test]
    fn test_har_http_versions() {
        let versions = vec!["HTTP/1.0", "HTTP/1.1", "HTTP/2"];
        let expected_statuses = vec![200u16, 200, 200];

        for (idx, ver) in versions.iter().enumerate() {
            let mut log = NetworkLog::new();
            let mut r = NetworkRecord::fetched(
                (idx + 1) as usize,
                "GET".into(),
                ResourceType::Document,
                "navigation".into(),
                "https://example.com/".into(),
                Initiator::Navigation,
            );
            r.status = Some(expected_statuses[idx]);
            r.http_version = Some(ver.to_string());
            log.push(r);

            let har = HarFile::from_network_log(&log);
            let entry = &har.log.entries[0];

            assert_eq!(entry.request.http_version, *ver);
            assert_eq!(entry.response.http_version, *ver);
        }
    }
}
