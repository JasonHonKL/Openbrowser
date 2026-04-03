use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceType {
    Document,
    Stylesheet,
    Script,
    Image,
    Font,
    Media,
    Fetch,
    Xhr,
    WebSocket,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Initiator {
    Navigation,
    Link,
    Script,
    Img,
    Fetch,
    Parser,
    Other,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkRecord {
    pub id: usize,
    pub method: String,
    #[serde(rename = "type")]
    pub resource_type: ResourceType,
    pub description: String,
    pub url: String,
    pub initiator: Initiator,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing_ms: Option<u128>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_url: Option<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub request_headers: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub response_headers: Vec<(String, String)>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_cache: Option<bool>,

    /// ISO 8601 timestamp of when the request was initiated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,

    /// HTTP protocol version (e.g., "HTTP/1.1", "HTTP/2").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_version: Option<String>,
}

impl NetworkRecord {
    pub fn fetched(
        id: usize,
        method: String,
        resource_type: ResourceType,
        description: String,
        url: String,
        initiator: Initiator,
    ) -> Self {
        Self {
            id,
            method,
            resource_type,
            description,
            url,
            initiator,
            status: None,
            status_text: None,
            content_type: None,
            body_size: None,
            timing_ms: None,
            redirect_url: None,
            request_headers: Vec::new(),
            response_headers: Vec::new(),
            error: None,
            from_cache: None,
            started_at: None,
            http_version: None,
        }
    }

    pub fn discovered(
        id: usize,
        resource_type: ResourceType,
        description: String,
        url: String,
        initiator: Initiator,
    ) -> Self {
        Self {
            id,
            method: "GET".to_string(),
            resource_type,
            description,
            url,
            initiator,
            status: None,
            status_text: None,
            content_type: None,
            body_size: None,
            timing_ms: None,
            redirect_url: None,
            request_headers: Vec::new(),
            response_headers: Vec::new(),
            error: None,
            from_cache: None,
            started_at: None,
            http_version: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NetworkLog {
    pub records: Vec<NetworkRecord>,
}

impl NetworkLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, record: NetworkRecord) {
        self.records.push(record);
    }

    pub fn total_bytes(&self) -> usize {
        self.records.iter().map(|r| r.body_size.unwrap_or(0)).sum()
    }

    pub fn total_time_ms(&self) -> u128 {
        self.records
            .iter()
            .map(|r| r.timing_ms.unwrap_or(0))
            .max()
            .unwrap_or(0)
    }

    pub fn next_id(&self) -> usize {
        self.records.len() + 1
    }

    pub fn total_requests(&self) -> usize {
        self.records.len()
    }

    pub fn failed_count(&self) -> usize {
        self.records
            .iter()
            .filter(|r| r.error.is_some() || r.status.is_some_and(|s| s >= 400))
            .count()
    }
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document => write!(f, "document"),
            Self::Stylesheet => write!(f, "stylesheet"),
            Self::Script => write!(f, "script"),
            Self::Image => write!(f, "image"),
            Self::Font => write!(f, "font"),
            Self::Media => write!(f, "media"),
            Self::Fetch => write!(f, "fetch"),
            Self::Xhr => write!(f, "xhr"),
            Self::WebSocket => write!(f, "websocket"),
            Self::Other => write!(f, "other"),
        }
    }
}

impl fmt::Display for Initiator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Navigation => write!(f, "navigation"),
            Self::Link => write!(f, "link"),
            Self::Script => write!(f, "script"),
            Self::Img => write!(f, "img"),
            Self::Fetch => write!(f, "fetch"),
            Self::Parser => write!(f, "parser"),
            Self::Other => write!(f, "other"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- NetworkRecord::fetched --
    #[test]
    fn test_fetched_defaults() {
        let r = NetworkRecord::fetched(
            1,
            "GET".into(),
            ResourceType::Document,
            "navigation".into(),
            "https://example.com".into(),
            Initiator::Navigation,
        );
        assert_eq!(r.id, 1);
        assert_eq!(r.method, "GET");
        assert_eq!(r.resource_type, ResourceType::Document);
        assert_eq!(r.description, "navigation");
        assert_eq!(r.url, "https://example.com");
        assert_eq!(r.initiator, Initiator::Navigation);
        assert!(r.status.is_none());
        assert!(r.status_text.is_none());
        assert!(r.content_type.is_none());
        assert!(r.body_size.is_none());
        assert!(r.timing_ms.is_none());
        assert!(r.redirect_url.is_none());
        assert!(r.request_headers.is_empty());
        assert!(r.response_headers.is_empty());
        assert!(r.error.is_none());
    }

    #[test]
    fn test_fetched_custom_method() {
        let r = NetworkRecord::fetched(
            2,
            "POST".into(),
            ResourceType::Fetch,
            "api call".into(),
            "https://api.example.com".into(),
            Initiator::Fetch,
        );
        assert_eq!(r.method, "POST");
        assert_eq!(r.resource_type, ResourceType::Fetch);
    }

    #[test]
    fn test_fetched_with_mutation() {
        let mut r = NetworkRecord::fetched(
            1,
            "GET".into(),
            ResourceType::Document,
            "nav".into(),
            "https://example.com".into(),
            Initiator::Navigation,
        );
        r.status = Some(200);
        r.status_text = Some("OK".into());
        r.content_type = Some("text/html".into());
        r.body_size = Some(4096);
        r.timing_ms = Some(142);
        assert_eq!(r.status, Some(200));
        assert_eq!(r.body_size, Some(4096));
        assert_eq!(r.timing_ms, Some(142));
    }

    // -- NetworkRecord::discovered --
    #[test]
    fn test_discovered_defaults() {
        let r = NetworkRecord::discovered(
            3,
            ResourceType::Stylesheet,
            "styles.css".into(),
            "https://example.com/styles.css".into(),
            Initiator::Link,
        );
        assert_eq!(r.id, 3);
        assert_eq!(r.method, "GET");
        assert!(r.status.is_none());
        assert!(r.error.is_none());
    }

    // -- NetworkLog --
    #[test]
    fn test_network_log_new() {
        let log = NetworkLog::new();
        assert!(log.records.is_empty());
    }

    #[test]
    fn test_network_log_push() {
        let mut log = NetworkLog::new();
        let r = NetworkRecord::discovered(
            1,
            ResourceType::Script,
            "app.js".into(),
            "https://example.com/app.js".into(),
            Initiator::Script,
        );
        log.push(r);
        assert_eq!(log.records.len(), 1);
        assert_eq!(log.records[0].url, "https://example.com/app.js");
    }

    #[test]
    fn test_network_log_next_id() {
        let mut log = NetworkLog::new();
        assert_eq!(log.next_id(), 1);
        log.push(NetworkRecord::discovered(
            1,
            ResourceType::Script,
            "a.js".into(),
            "https://a".into(),
            Initiator::Script,
        ));
        assert_eq!(log.next_id(), 2);
        log.push(NetworkRecord::discovered(
            2,
            ResourceType::Script,
            "b.js".into(),
            "https://b".into(),
            Initiator::Script,
        ));
        assert_eq!(log.next_id(), 3);
    }

    #[test]
    fn test_network_log_total_requests() {
        let mut log = NetworkLog::new();
        assert_eq!(log.total_requests(), 0);
        log.push(NetworkRecord::discovered(
            1,
            ResourceType::Script,
            "a.js".into(),
            "https://a".into(),
            Initiator::Script,
        ));
        log.push(NetworkRecord::discovered(
            2,
            ResourceType::Stylesheet,
            "b.css".into(),
            "https://b".into(),
            Initiator::Link,
        ));
        assert_eq!(log.total_requests(), 2);
    }

    #[test]
    fn test_network_log_total_bytes() {
        let mut log = NetworkLog::new();
        assert_eq!(log.total_bytes(), 0);

        let mut r1 = NetworkRecord::fetched(
            1,
            "GET".into(),
            ResourceType::Document,
            "nav".into(),
            "https://a".into(),
            Initiator::Navigation,
        );
        r1.body_size = Some(1024);
        log.push(r1);

        let mut r2 = NetworkRecord::fetched(
            2,
            "GET".into(),
            ResourceType::Script,
            "app.js".into(),
            "https://b".into(),
            Initiator::Script,
        );
        r2.body_size = Some(4096);
        log.push(r2);

        let r3 = NetworkRecord::discovered(
            3,
            ResourceType::Image,
            "img.png".into(),
            "https://c".into(),
            Initiator::Img,
        );
        log.push(r3);

        assert_eq!(log.total_bytes(), 5120);
    }

    #[test]
    fn test_network_log_total_time_ms() {
        let mut log = NetworkLog::new();
        assert_eq!(log.total_time_ms(), 0);

        let mut r1 = NetworkRecord::fetched(
            1,
            "GET".into(),
            ResourceType::Document,
            "nav".into(),
            "https://a".into(),
            Initiator::Navigation,
        );
        r1.timing_ms = Some(100);
        log.push(r1);

        let mut r2 = NetworkRecord::fetched(
            2,
            "GET".into(),
            ResourceType::Script,
            "app.js".into(),
            "https://b".into(),
            Initiator::Script,
        );
        r2.timing_ms = Some(250);
        log.push(r2);

        assert_eq!(log.total_time_ms(), 250);
    }

    #[test]
    fn test_network_log_total_time_ms_empty() {
        let log = NetworkLog::new();
        assert_eq!(log.total_time_ms(), 0);
    }

    #[test]
    fn test_network_log_failed_count() {
        let mut log = NetworkLog::new();
        assert_eq!(log.failed_count(), 0);

        let mut r_ok = NetworkRecord::fetched(
            1,
            "GET".into(),
            ResourceType::Document,
            "nav".into(),
            "https://a".into(),
            Initiator::Navigation,
        );
        r_ok.status = Some(200);
        log.push(r_ok);

        let mut r404 = NetworkRecord::fetched(
            2,
            "GET".into(),
            ResourceType::Image,
            "img.png".into(),
            "https://b".into(),
            Initiator::Img,
        );
        r404.status = Some(404);
        log.push(r404);

        let mut r_err = NetworkRecord::fetched(
            3,
            "GET".into(),
            ResourceType::Script,
            "app.js".into(),
            "https://c".into(),
            Initiator::Script,
        );
        r_err.error = Some("connection refused".into());
        log.push(r_err);

        let mut r500 = NetworkRecord::fetched(
            4,
            "GET".into(),
            ResourceType::Fetch,
            "api".into(),
            "https://d".into(),
            Initiator::Fetch,
        );
        r500.status = Some(500);
        log.push(r500);

        assert_eq!(log.failed_count(), 3);
    }

    #[test]
    fn test_network_log_failed_count_3xx_not_failed() {
        let mut log = NetworkLog::new();
        let mut r = NetworkRecord::fetched(
            1,
            "GET".into(),
            ResourceType::Document,
            "nav".into(),
            "https://a".into(),
            Initiator::Navigation,
        );
        r.status = Some(301);
        log.push(r);
        assert_eq!(log.failed_count(), 0);
    }

    // -- Display for ResourceType --
    #[test]
    fn test_resource_type_display() {
        assert_eq!(format!("{}", ResourceType::Document), "document");
        assert_eq!(format!("{}", ResourceType::Stylesheet), "stylesheet");
        assert_eq!(format!("{}", ResourceType::Script), "script");
        assert_eq!(format!("{}", ResourceType::Image), "image");
        assert_eq!(format!("{}", ResourceType::Font), "font");
        assert_eq!(format!("{}", ResourceType::Media), "media");
        assert_eq!(format!("{}", ResourceType::Fetch), "fetch");
        assert_eq!(format!("{}", ResourceType::Xhr), "xhr");
        assert_eq!(format!("{}", ResourceType::Other), "other");
    }

    // -- Display for Initiator --
    #[test]
    fn test_initiator_display() {
        assert_eq!(format!("{}", Initiator::Navigation), "navigation");
        assert_eq!(format!("{}", Initiator::Link), "link");
        assert_eq!(format!("{}", Initiator::Script), "script");
        assert_eq!(format!("{}", Initiator::Img), "img");
        assert_eq!(format!("{}", Initiator::Fetch), "fetch");
        assert_eq!(format!("{}", Initiator::Parser), "parser");
        assert_eq!(format!("{}", Initiator::Other), "other");
    }

    // -- Serialize --
    #[test]
    fn test_serialize_record() {
        let mut r = NetworkRecord::fetched(
            1,
            "GET".into(),
            ResourceType::Document,
            "nav".into(),
            "https://example.com".into(),
            Initiator::Navigation,
        );
        r.status = Some(200);
        r.body_size = Some(4096);
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains(r#""type":"document""#));
        assert!(json.contains(r#""status":200"#));
        assert!(json.contains(r#""body_size":4096"#));
        assert!(!json.contains("status_text"));
        assert!(!json.contains("error"));
    }

    #[test]
    fn test_serialize_record_with_error() {
        let mut r = NetworkRecord::fetched(
            1,
            "GET".into(),
            ResourceType::Script,
            "app.js".into(),
            "https://example.com".into(),
            Initiator::Script,
        );
        r.error = Some("timeout".into());
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains(r#""error":"timeout""#));
    }

    #[test]
    fn test_serialize_record_with_headers() {
        let mut r = NetworkRecord::fetched(
            1,
            "GET".into(),
            ResourceType::Document,
            "nav".into(),
            "https://example.com".into(),
            Initiator::Navigation,
        );
        r.response_headers
            .push(("content-type".into(), "text/html".into()));
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("content-type"));
    }

    #[test]
    fn test_serialize_log() {
        let mut log = NetworkLog::new();
        log.push(NetworkRecord::discovered(
            1,
            ResourceType::Script,
            "a.js".into(),
            "https://a".into(),
            Initiator::Script,
        ));
        let json = serde_json::to_string(&log).unwrap();
        assert!(json.contains(r#""records""#));
    }

    #[test]
    fn test_serialize_resource_type_lowercase() {
        assert_eq!(
            serde_json::to_string(&ResourceType::Document).unwrap(),
            r#""document""#
        );
        assert_eq!(
            serde_json::to_string(&ResourceType::Stylesheet).unwrap(),
            r#""stylesheet""#
        );
        assert_eq!(
            serde_json::to_string(&ResourceType::Fetch).unwrap(),
            r#""fetch""#
        );
    }

    #[test]
    fn test_serialize_initiator_lowercase() {
        assert_eq!(
            serde_json::to_string(&Initiator::Navigation).unwrap(),
            r#""navigation""#
        );
        assert_eq!(serde_json::to_string(&Initiator::Img).unwrap(), r#""img""#);
    }
}
