use crate::record::{NetworkLog, NetworkRecord};
use serde::Serialize;

const URL_DISPLAY_MAX: usize = 80;

pub fn format_table(log: &NetworkLog) -> String {
    if log.records.is_empty() {
        return String::new();
    }

    let total_bytes = log.total_bytes();
    let total_time = log.total_time_ms();
    let failed = log.failed_count();

    let mut out = String::new();

    let mut header = format!("# Network — {} requests", log.total_requests());
    if total_bytes > 0 {
        header.push_str(&format!(" — {}", format_bytes(total_bytes)));
    }
    if total_time > 0 {
        header.push_str(&format!(" — {}ms total", total_time));
    }
    if failed > 0 {
        header.push_str(&format!(" — {} failed", failed));
    }
    out.push_str(&header);
    out.push('\n');

    // Column headers
    out.push_str("\n");
    out.push_str(&format!(
        "  {:>2}  {:<7}  {:<11}  {:<24}  {:<40}  {:>6}  {:>8}  {:>6}\n",
        "#", "Method", "Type", "Resource", "URL", "Status", "Size", "Time"
    ));
    out.push_str(&format!(
        "  {:>2}  {:<7}  {:<11}  {:<24}  {:<40}  {:>6}  {:>8}  {:>6}\n",
        "—",
        "——————",
        "—————————",
        "————————————————",
        "———————————————————————————————",
        "——————",
        "———————",
        "——————"
    ));

    for record in &log.records {
        let method = record.method.to_uppercase();
        let type_str = record.resource_type.to_string();
        let desc = truncate(&record.description, 24);
        let url = truncate(&record.url, URL_DISPLAY_MAX);

        let status = match &record.status {
            Some(s) => s.to_string(),
            None => "—".to_string(),
        };

        let size = match record.body_size {
            Some(s) => format_bytes(s),
            None => "—".to_string(),
        };

        let time = match record.timing_ms {
            Some(t) => format!("{}ms", t),
            None => "—".to_string(),
        };

        out.push_str(&format!(
            "  {:>2}  {:<7}  {:<11}  {:<24}  {:<40}  {:>6}  {:>8}  {:>6}\n",
            record.id, method, type_str, desc, url, status, size, time
        ));
    }

    out
}

pub fn format_table_with_initiator(log: &NetworkLog) -> String {
    if log.records.is_empty() {
        return String::new();
    }

    let total_bytes = log.total_bytes();
    let total_time = log.total_time_ms();
    let failed = log.failed_count();

    let mut out = String::new();

    let mut header = format!("# Network — {} requests", log.total_requests());
    if total_bytes > 0 {
        header.push_str(&format!(" — {}", format_bytes(total_bytes)));
    }
    if total_time > 0 {
        header.push_str(&format!(" — {}ms total", total_time));
    }
    if failed > 0 {
        header.push_str(&format!(" — {} failed", failed));
    }
    out.push_str(&header);
    out.push('\n');

    out.push_str("\n");
    out.push_str(&format!(
        "  {:<7}  {:<10}  {:<44}\n",
        "Method", "Type", "Resource"
    ));
    out.push_str(&format!(
        "  {:<7}  {:<10}  {:<44}\n",
        "——————", "——————————", "——————————————————————————————"
    ));

    for record in &log.records {
        let type_str = record.initiator.to_string();
        let resource_desc = format!(
            "{} · {}",
            record.resource_type,
            truncate(&record.description, 30)
        );
        let url = truncate(&record.url, URL_DISPLAY_MAX + 10);

        out.push_str(&format!(
            "  {:<7}  {:<10}  {:<44}  {}\n",
            "GET", type_str, resource_desc, url
        ));
    }

    out
}

#[derive(Serialize)]
pub struct NetworkLogJson {
    pub total_requests: usize,
    pub total_bytes: usize,
    pub total_time_ms: u128,
    pub failed: usize,
    pub requests: Vec<NetworkRecord>,
}

impl NetworkLogJson {
    pub fn from_log(log: &NetworkLog) -> Self {
        Self {
            total_requests: log.total_requests(),
            total_bytes: log.total_bytes(),
            total_time_ms: log.total_time_ms(),
            failed: log.failed_count(),
            requests: log.records.clone(),
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}

pub fn format_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{Initiator, ResourceType};

    // -- format_bytes --
    #[test]
    fn test_format_bytes_zero() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn test_format_bytes_bytes_range() {
        assert_eq!(format_bytes(1), "1 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn test_format_bytes_kb_range() {
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(51200), "50.0 KB");
    }

    #[test]
    fn test_format_bytes_mb_range() {
        assert_eq!(format_bytes(1048576), "1.0 MB");
        assert_eq!(format_bytes(1572864), "1.5 MB");
    }

    // -- truncate --
    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hi", 10), "hi");
    }

    #[test]
    fn test_truncate_exact() {
        assert_eq!(truncate("exactly", 7), "exactly");
    }

    #[test]
    fn test_truncate_over() {
        assert_eq!(truncate("a very long string here", 10), "a very ...");
    }

    #[test]
    fn test_truncate_empty() {
        assert_eq!(truncate("", 10), "");
    }

    // -- format_table --
    #[test]
    fn test_format_table_empty() {
        let log = NetworkLog::new();
        assert!(format_table(&log).is_empty());
    }

    #[test]
    fn test_format_table_single_record() {
        let mut log = NetworkLog::new();
        let mut r = NetworkRecord::fetched(
            1,
            "GET".into(),
            ResourceType::Document,
            "document · navigation".into(),
            "https://example.com".into(),
            Initiator::Navigation,
        );
        r.status = Some(200);
        r.body_size = Some(4096);
        r.timing_ms = Some(142);
        log.push(r);
        let out = format_table(&log);
        assert!(out.contains("1 requests"));
        assert!(out.contains("4.0 KB"));
        assert!(out.contains("142ms total"));
        assert!(out.contains("200"));
        assert!(out.contains("example.com"));
    }

    #[test]
    fn test_format_table_discovered_shows_dashes() {
        let mut log = NetworkLog::new();
        log.push(NetworkRecord::discovered(
            1,
            ResourceType::Stylesheet,
            "styles.css".into(),
            "https://example.com/styles.css".into(),
            Initiator::Link,
        ));
        let out = format_table(&log);
        assert!(out.contains("1 requests"));
        assert!(out.contains("document") == false);
        assert!(out.contains("stylesheet"));
    }

    #[test]
    fn test_format_table_multiple_records() {
        let mut log = NetworkLog::new();
        let mut r1 = NetworkRecord::fetched(
            1,
            "GET".into(),
            ResourceType::Document,
            "nav".into(),
            "https://example.com".into(),
            Initiator::Navigation,
        );
        r1.status = Some(200);
        r1.body_size = Some(1024);
        r1.timing_ms = Some(100);
        log.push(r1);

        let mut r2 = NetworkRecord::fetched(
            2,
            "GET".into(),
            ResourceType::Script,
            "app.js".into(),
            "https://example.com/app.js".into(),
            Initiator::Script,
        );
        r2.status = Some(404);
        r2.error = Some("not found".into());
        log.push(r2);

        let out = format_table(&log);
        assert!(out.contains("2 requests"));
        assert!(out.contains("1 failed"));
        assert!(out.contains("1.0 KB"));
        assert!(out.contains("100ms total"));
    }

    #[test]
    fn test_format_table_with_no_bytes() {
        let mut log = NetworkLog::new();
        log.push(NetworkRecord::discovered(
            1,
            ResourceType::Image,
            "img.png".into(),
            "https://example.com/img.png".into(),
            Initiator::Img,
        ));
        let out = format_table(&log);
        assert!(!out.contains(" B"));
        assert!(!out.contains(" KB"));
    }

    #[test]
    fn test_format_table_with_error_record() {
        let mut log = NetworkLog::new();
        let mut r = NetworkRecord::fetched(
            1,
            "GET".into(),
            ResourceType::Script,
            "app.js".into(),
            "https://example.com/app.js".into(),
            Initiator::Script,
        );
        r.error = Some("connection refused".into());
        log.push(r);
        let out = format_table(&log);
        assert!(out.contains("1 failed"));
    }

    #[test]
    fn test_format_table_long_url_truncated() {
        let long_url = "https://example.com/a/very/long/path/that/exceeds/the/display/max/limit/for/url/column.js";
        let mut log = NetworkLog::new();
        log.push(NetworkRecord::discovered(
            1,
            ResourceType::Script,
            "script.js".into(),
            long_url.into(),
            Initiator::Script,
        ));
        let out = format_table(&log);
        assert!(out.contains("..."));
        assert!(!out.contains(long_url));
    }

    // -- format_table_with_initiator --
    #[test]
    fn test_format_table_with_initiator_empty() {
        let log = NetworkLog::new();
        assert!(format_table_with_initiator(&log).is_empty());
    }

    #[test]
    fn test_format_table_with_initiator_single() {
        let mut log = NetworkLog::new();
        log.push(NetworkRecord::discovered(
            1,
            ResourceType::Stylesheet,
            "styles.css".into(),
            "https://example.com/styles.css".into(),
            Initiator::Link,
        ));
        let out = format_table_with_initiator(&log);
        assert!(out.contains("link"));
        assert!(out.contains("stylesheet"));
        assert!(out.contains("styles.css"));
        assert!(out.contains("example.com"));
    }

    #[test]
    fn test_format_table_with_initiator_multiple() {
        let mut log = NetworkLog::new();
        log.push(NetworkRecord::discovered(
            1,
            ResourceType::Document,
            "navigation".into(),
            "https://example.com".into(),
            Initiator::Navigation,
        ));
        log.push(NetworkRecord::discovered(
            2,
            ResourceType::Stylesheet,
            "styles.css".into(),
            "https://example.com/styles.css".into(),
            Initiator::Link,
        ));
        log.push(NetworkRecord::discovered(
            3,
            ResourceType::Script,
            "script.js".into(),
            "https://example.com/script.js".into(),
            Initiator::Script,
        ));
        let out = format_table_with_initiator(&log);
        assert!(out.contains("3 requests"));
        assert!(out.contains("navigation"));
        assert!(out.contains("link"));
        assert!(out.contains("script"));
    }

    #[test]
    fn test_format_table_with_initiator_shows_resource_type() {
        let mut log = NetworkLog::new();
        log.push(NetworkRecord::discovered(
            1,
            ResourceType::Script,
            "script.js".into(),
            "https://example.com/script.js".into(),
            Initiator::Script,
        ));
        let out = format_table_with_initiator(&log);
        assert!(out.contains("script · script.js"));
    }

    #[test]
    fn test_format_table_with_initiator_failed() {
        let mut log = NetworkLog::new();
        let mut r = NetworkRecord::fetched(
            1,
            "GET".into(),
            ResourceType::Document,
            "nav".into(),
            "https://example.com".into(),
            Initiator::Navigation,
        );
        r.status = Some(500);
        log.push(r);
        let out = format_table_with_initiator(&log);
        assert!(out.contains("1 failed"));
    }

    // -- NetworkLogJson::from_log --
    #[test]
    fn test_network_log_json_from_log_empty() {
        let log = NetworkLog::new();
        let json = NetworkLogJson::from_log(&log);
        assert_eq!(json.total_requests, 0);
        assert_eq!(json.total_bytes, 0);
        assert_eq!(json.total_time_ms, 0);
        assert_eq!(json.failed, 0);
        assert!(json.requests.is_empty());
    }

    #[test]
    fn test_network_log_json_from_log_with_records() {
        let mut log = NetworkLog::new();
        let mut r1 = NetworkRecord::fetched(
            1,
            "GET".into(),
            ResourceType::Document,
            "nav".into(),
            "https://example.com".into(),
            Initiator::Navigation,
        );
        r1.status = Some(200);
        r1.body_size = Some(4096);
        r1.timing_ms = Some(142);
        log.push(r1);

        let mut r2 = NetworkRecord::fetched(
            2,
            "GET".into(),
            ResourceType::Script,
            "app.js".into(),
            "https://example.com/app.js".into(),
            Initiator::Script,
        );
        r2.status = Some(404);
        log.push(r2);

        let json = NetworkLogJson::from_log(&log);
        assert_eq!(json.total_requests, 2);
        assert_eq!(json.total_bytes, 4096);
        assert_eq!(json.total_time_ms, 142);
        assert_eq!(json.failed, 1);
        assert_eq!(json.requests.len(), 2);
    }

    #[test]
    fn test_network_log_json_serialize() {
        let mut log = NetworkLog::new();
        let mut r = NetworkRecord::fetched(
            1,
            "GET".into(),
            ResourceType::Document,
            "nav".into(),
            "https://example.com".into(),
            Initiator::Navigation,
        );
        r.status = Some(200);
        r.body_size = Some(1024);
        r.timing_ms = Some(50);
        log.push(r);
        let json = NetworkLogJson::from_log(&log);
        let serialized = serde_json::to_string(&json).unwrap();
        assert!(serialized.contains(r#""total_requests":1"#));
        assert!(serialized.contains(r#""total_bytes":1024"#));
        assert!(serialized.contains(r#""total_time_ms":50"#));
        assert!(serialized.contains(r#""failed":0"#));
        assert!(serialized.contains(r#""requests"#));
    }
}
