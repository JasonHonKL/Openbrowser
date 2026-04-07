use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use base64::Engine;
use parking_lot::Mutex;
use rquest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredHeader {
    name: String,
    value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredCookie {
    url: String,
    header: String,
}

/// Error type for session operations.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Cookie error: {0}")]
    Cookie(String),
}

/// A structured cookie entry for programmatic access.
#[derive(Debug, Clone, Serialize)]
pub struct CookieEntry {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub http_only: bool,
    pub secure: bool,
    pub same_site: Option<String>,
}

/// Result type alias for session operations.
pub type SessionResult<T> = Result<T, SessionError>;

// Session size limits
const MAX_COOKIES: usize = 500;
const MAX_HEADERS: usize = 50;
const MAX_LOCAL_STORAGE_KEYS_PER_ORIGIN: usize = 200;
const MAX_LOCAL_STORAGE_ORIGINS: usize = 50;

pub struct SessionStore {
    session_dir: PathBuf,
    cookies_path: PathBuf,
    headers_path: PathBuf,
    local_storage_path: PathBuf,
    jar: Mutex<cookie_store::CookieStore>,
    raw_cookies: Mutex<Vec<StoredCookie>>,
    headers: Mutex<Vec<StoredHeader>>,
    local_storage: Mutex<HashMap<String, HashMap<String, String>>>,
    /// If true, session data is kept in memory only and never written to disk.
    ephemeral: bool,
    /// If true, localStorage operations are disabled.
    no_local_storage: bool,
}

impl SessionStore {
    pub fn load(name: &str, cache_dir: &Path) -> SessionResult<Self> {
        let session_dir = cache_dir.join("sessions").join(name);
        std::fs::create_dir_all(&session_dir)?;

        let cookies_path = session_dir.join("cookies.jsonl");
        let headers_path = session_dir.join("headers.json");
        let local_storage_path = session_dir.join("local_storage.json");

        let mut jar = cookie_store::CookieStore::default();

        let raw_cookies = if cookies_path.exists() {
            let data = std::fs::read_to_string(&cookies_path)?;
            let mut cookies = Vec::new();
            for line in data.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                match serde_json::from_str::<StoredCookie>(line) {
                    Ok(c) => {
                        if let Ok(url) = c.url.parse::<Url>() {
                            if let Err(e) = jar.parse(&c.header, &url) {
                                tracing::debug!("failed to reload cookie: {}", e);
                            }
                        }
                        cookies.push(c);
                    }
                    Err(e) => {
                        tracing::debug!("failed to parse cookie line: {}", e);
                    }
                }
            }
            cookies
        } else {
            Vec::new()
        };

        let headers = if headers_path.exists() {
            let data = std::fs::read_to_string(&headers_path)?;
            match serde_json::from_str::<Vec<StoredHeader>>(&data) {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!(
                        "[session] Warning: failed to parse headers for session '{}': {}",
                        name,
                        e
                    );
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        let local_storage = if local_storage_path.exists() {
            let data = std::fs::read_to_string(&local_storage_path)?;
            match serde_json::from_str::<HashMap<String, HashMap<String, String>>>(&data) {
                Ok(ls) => ls,
                Err(e) => {
                    tracing::warn!(
                        "[session] Warning: failed to parse localStorage for session '{}': {}",
                        name,
                        e
                    );
                    HashMap::new()
                }
            }
        } else {
            HashMap::new()
        };

        Ok(Self {
            session_dir,
            cookies_path,
            headers_path,
            local_storage_path,
            jar: Mutex::new(jar),
            raw_cookies: Mutex::new(raw_cookies),
            headers: Mutex::new(headers),
            local_storage: Mutex::new(local_storage),
            ephemeral: false,
            no_local_storage: false,
        })
    }

    /// Create an ephemeral (in-memory only) session store.
    /// Data is kept in memory but never persisted to disk.
    pub fn ephemeral(name: &str, cache_dir: &Path) -> SessionResult<Self> {
        let session_dir = cache_dir.join("sessions").join(name);
        // Don't create directories — we won't write to disk
        Ok(Self {
            session_dir,
            cookies_path: PathBuf::new(),
            headers_path: PathBuf::new(),
            local_storage_path: PathBuf::new(),
            jar: Mutex::new(cookie_store::CookieStore::default()),
            raw_cookies: Mutex::new(Vec::new()),
            headers: Mutex::new(Vec::new()),
            local_storage: Mutex::new(HashMap::new()),
            ephemeral: true,
            no_local_storage: true,
        })
    }

    pub fn save(&self) -> SessionResult<()> {
        // Ephemeral sessions skip disk persistence
        if self.ephemeral {
            return Ok(());
        }

        {
            let raw = self.raw_cookies.lock();
            let mut lines = Vec::new();
            for c in raw.iter() {
                lines.push(serde_json::to_string(c)?);
            }
            let cookie_data = lines.join("\n") + "\n";
            Self::atomic_write(&self.cookies_path, &cookie_data)?;
        }
        {
            let headers = self.headers.lock();
            let json = serde_json::to_string_pretty(&*headers)?;
            Self::atomic_write(&self.headers_path, &json)?;
        }
        {
            let ls = self.local_storage.lock();
            let json = serde_json::to_string_pretty(&*ls)?;
            Self::atomic_write(&self.local_storage_path, &json)?;
        }
        Ok(())
    }

    fn atomic_write(path: &Path, data: &str) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp_path = path.with_extension("tmp");
        std::fs::write(&tmp_path, data)?;
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    }

    pub fn session_name(&self) -> &str {
        self.session_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
    }

    pub fn headers(&self) -> HeaderMap {
        let headers = self.headers.lock();
        let mut map = HeaderMap::new();
        for h in headers.iter() {
            if let (Ok(name), Ok(value)) = (
                h.name.parse::<http::header::HeaderName>(),
                HeaderValue::from_str(&h.value),
            ) {
                map.append(name, value);
            }
        }
        map
    }

    pub fn set_headers(&self, headers: HeaderMap) {
        let mut stored = self.headers.lock();
        stored.clear();
        for (name, value) in headers.iter() {
            if let Ok(v) = value.to_str() {
                stored.push(StoredHeader {
                    name: name.as_str().to_string(),
                    value: v.to_string(),
                });
            }
        }
    }

    pub fn add_header(&self, name: &str, value: &str) {
        let mut stored = self.headers.lock();
        if stored.len() >= MAX_HEADERS {
            tracing::warn!(
                "session header limit ({}) reached, dropping header: {}",
                MAX_HEADERS,
                name
            );
            return;
        }
        stored.push(StoredHeader {
            name: name.to_string(),
            value: value.to_string(),
        });
    }

    pub fn cookie_count(&self) -> usize {
        let jar = self.jar.lock();
        jar.iter_unexpired().count()
    }

    pub fn header_count(&self) -> usize { self.headers.lock().len() }

    pub fn local_storage_origins(&self) -> Vec<String> {
        let ls = self.local_storage.lock();
        ls.keys().cloned().collect()
    }

    pub fn local_storage_get(&self, origin: &str, key: &str) -> Option<String> {
        let ls = self.local_storage.lock();
        ls.get(origin).and_then(|map| map.get(key).cloned())
    }

    pub fn local_storage_set(&self, origin: &str, key: &str, value: &str) {
        if self.no_local_storage {
            return;
        }
        let mut ls = self.local_storage.lock();
        if ls.len() >= MAX_LOCAL_STORAGE_ORIGINS && !ls.contains_key(origin) {
            tracing::warn!(
                "session localStorage origin limit ({}) reached, dropping set for {}",
                MAX_LOCAL_STORAGE_ORIGINS,
                origin
            );
            return;
        }
        let map = ls.entry(origin.to_string()).or_default();
        if map.len() >= MAX_LOCAL_STORAGE_KEYS_PER_ORIGIN && !map.contains_key(key) {
            tracing::warn!(
                "session localStorage key limit ({}) reached for origin {}, dropping key: {}",
                MAX_LOCAL_STORAGE_KEYS_PER_ORIGIN,
                origin,
                key
            );
            return;
        }
        map.insert(key.to_string(), value.to_string());
    }

    pub fn local_storage_keys(&self, origin: &str) -> Vec<String> {
        let ls = self.local_storage.lock();
        ls.get(origin)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn local_storage_remove(&self, origin: &str, key: &str) {
        let mut ls = self.local_storage.lock();
        if let Some(map) = ls.get_mut(origin) {
            map.remove(key);
        }
    }

    pub fn local_storage_clear(&self, origin: &str) {
        let mut ls = self.local_storage.lock();
        ls.remove(origin);
    }

    pub fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        let jar = self.jar.lock();
        let cookies: Vec<String> = jar
            .get_request_values(url)
            .map(|(name, value)| format!("{}={}", name, value))
            .collect();
        if cookies.is_empty() {
            None
        } else {
            HeaderValue::from_str(&cookies.join("; ")).ok()
        }
    }

    pub fn set_cookies<'a>(
        &self,
        cookie_headers: &mut dyn Iterator<Item = &'a HeaderValue>,
        url: &Url,
    ) {
        let mut jar = self.jar.lock();
        let mut raw = self.raw_cookies.lock();
        for hv in cookie_headers {
            if raw.len() >= MAX_COOKIES {
                tracing::warn!(
                    "session cookie limit ({}) reached, dropping further cookies for {}",
                    MAX_COOKIES,
                    url
                );
                break;
            }
            if let Ok(s) = hv.to_str() {
                let cookie_str = s.trim();
                if !cookie_str.is_empty() {
                    if let Err(e) = jar.parse(cookie_str, url) {
                        tracing::debug!("failed to parse cookie: {}", e);
                    } else {
                        raw.push(StoredCookie {
                            url: url.to_string(),
                            header: cookie_str.to_string(),
                        });
                    }
                }
            }
        }
    }

    pub fn clear_cookies(&self) {
        let mut jar = self.jar.lock();
        let mut raw = self.raw_cookies.lock();
        jar.clear();
        raw.clear();
    }

    pub fn delete_cookie(&self, name: &str, domain: &str, path: &str) -> bool {
        let mut jar = self.jar.lock();
        let removed = jar.remove(domain, path, name).is_some();
        if removed {
            let mut raw = self.raw_cookies.lock();
            raw.retain(|c| {
                let header_name = c
                    .header
                    .split(';')
                    .next()
                    .and_then(|s| s.splitn(2, '=').next())
                    .map(|n| n.trim())
                    .unwrap_or("");
                header_name != name
            });
        }
        removed
    }

    /// List all unexpired cookies as structured entries.
    pub fn all_cookies(&self) -> Vec<CookieEntry> {
        let jar = self.jar.lock();
        jar.iter_unexpired()
            .map(|cookie| {
                let name = cookie.name().to_string();
                let value = cookie.value().to_string();
                let domain = cookie.domain().unwrap_or("").to_string();
                let path = cookie.path().unwrap_or("/").to_string();
                let http_only = cookie.http_only().unwrap_or(false);
                let secure = cookie.secure().unwrap_or(false);
                let same_site = cookie.same_site().map(|ss| {
                    match ss {
                        rquest::cookie::SameSite::Strict => "Strict".to_string(),
                        rquest::cookie::SameSite::Lax => "Lax".to_string(),
                        rquest::cookie::SameSite::None => "None".to_string(),
                    }
                });
                CookieEntry {
                    name,
                    value,
                    domain,
                    path,
                    http_only,
                    secure,
                    same_site,
                }
            })
            .collect()
    }

    /// Set a cookie programmatically by name, value, domain, and path.
    pub fn set_cookie(&self, name: &str, value: &str, domain: &str, path: &str) {
        let header = format!("{}={}; Domain={}; Path={}", name, value, domain, path);
        let url_str = if domain.starts_with('.') {
            format!("https://{}", &domain[1..])
        } else {
            format!("https://{}", domain)
        };
        if let Ok(url) = url_str.parse::<Url>() {
            let mut jar = self.jar.lock();
            let mut raw = self.raw_cookies.lock();
            if let Err(e) = jar.parse(&header, &url) {
                tracing::debug!("failed to parse cookie: {}", e);
            } else if raw.len() < MAX_COOKIES {
                raw.push(StoredCookie {
                    url: url.to_string(),
                    header,
                });
            }
        }
    }

    /// Get a reference to the inner cookie store for rquest integration.
    pub fn jar(&self) -> &Mutex<cookie_store::CookieStore> { &self.jar }

    pub fn session_dir(&self) -> &Path { &self.session_dir }

    pub fn list_sessions(cache_dir: &Path) -> SessionResult<Vec<String>> {
        let sessions_dir = cache_dir.join("sessions");
        if !sessions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        for entry in std::fs::read_dir(sessions_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    sessions.push(name.to_string());
                }
            }
        }
        sessions.sort();
        Ok(sessions)
    }

    pub fn destroy(name: &str, cache_dir: &Path) -> SessionResult<()> {
        let session_dir = cache_dir.join("sessions").join(name);
        if session_dir.exists() {
            std::fs::remove_dir_all(&session_dir)?;
        }
        Ok(())
    }

    pub fn parse_auth_header(auth: &str) -> Option<(String, String)> {
        let parts: Vec<&str> = auth.splitn(2, ':').collect();
        if parts.len() != 2 {
            return None;
        }
        let scheme = parts[0].to_lowercase();
        let creds = parts[1];

        match scheme.as_str() {
            "bearer" => {
                if creds.is_empty() {
                    return None;
                }
                Some(("Authorization".to_string(), format!("Bearer {}", creds)))
            }
            "basic" => {
                let user_pass: Vec<&str> = creds.splitn(2, ':').collect();
                if user_pass.len() != 2 {
                    return None;
                }
                let encoded = base64::engine::general_purpose::STANDARD
                    .encode(format!("{}:{}", user_pass[0], user_pass[1]));
                Some(("Authorization".to_string(), format!("Basic {}", encoded)))
            }
            _ => None,
        }
    }

    pub fn parse_custom_header(header: &str) -> Option<(String, String)> {
        let parts: Vec<&str> = header.splitn(2, ':').collect();
        if parts.len() != 2 {
            return None;
        }
        let name = parts[0].trim();
        let value = parts[1].trim();
        if name.is_empty() {
            return None;
        }
        Some((name.to_string(), value.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_load_save() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let store = SessionStore::load("test", dir.path()).expect("failed to load session");

        store.add_header("X-Custom", "value");
        store.save().expect("failed to save session");

        drop(store);

        let store2 = SessionStore::load("test", dir.path()).expect("failed to reload session");
        assert_eq!(store2.header_count(), 1);
        let headers = store2.headers();
        assert_eq!(
            headers.get("X-Custom").and_then(|v| v.to_str().ok()),
            Some("value")
        );
    }

    #[test]
    fn test_cookies() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let store = SessionStore::load("test", dir.path()).expect("failed to load session");

        assert_eq!(store.cookie_count(), 0);

        let url: Url = "https://example.com".parse().expect("failed to parse url");
        let header = HeaderValue::from_static("session=abc123; Path=/; HttpOnly");
        let mut iter = std::iter::once(&header);
        store.set_cookies(&mut iter, &url);

        assert_eq!(store.cookie_count(), 1);

        let cookies = store.cookies(&url).expect("expected cookies");
        assert!(cookies.to_str().unwrap_or("").contains("session"));
    }

    #[test]
    fn test_auth_header_parsing() {
        let (name, value) = SessionStore::parse_auth_header("bearer:my-token")
            .expect("failed to parse bearer auth");
        assert_eq!(name, "Authorization");
        assert_eq!(value, "Bearer my-token");

        let (name, value) =
            SessionStore::parse_auth_header("basic:user:pass").expect("failed to parse basic auth");
        assert_eq!(name, "Authorization");
        assert!(value.starts_with("Basic "));
    }

    #[test]
    fn test_custom_header_parsing() {
        let (name, value) = SessionStore::parse_custom_header("Content-Type: application/json")
            .expect("failed to parse custom header");
        assert_eq!(name, "Content-Type");
        assert_eq!(value, "application/json");

        assert!(SessionStore::parse_custom_header("no-colon").is_none());
    }

    #[test]
    fn test_local_storage() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let store = SessionStore::load("test", dir.path()).expect("failed to load session");

        assert_eq!(store.local_storage_get("https://example.com", "key1"), None);

        store.local_storage_set("https://example.com", "key1", "value1");
        store.local_storage_set("https://example.com", "key2", "value2");
        store.local_storage_set("https://other.com", "key3", "value3");

        assert_eq!(
            store.local_storage_get("https://example.com", "key1"),
            Some("value1".to_string())
        );
        assert_eq!(
            store.local_storage_get("https://example.com", "key2"),
            Some("value2".to_string())
        );
        assert_eq!(
            store.local_storage_get("https://other.com", "key3"),
            Some("value3".to_string())
        );
        assert_eq!(store.local_storage_get("https://example.com", "key3"), None);

        let keys = store.local_storage_keys("https://example.com");
        assert_eq!(keys.len(), 2);

        store.local_storage_remove("https://example.com", "key1");
        assert_eq!(store.local_storage_get("https://example.com", "key1"), None);
        assert_eq!(
            store.local_storage_get("https://example.com", "key2"),
            Some("value2".to_string())
        );

        store.local_storage_clear("https://other.com");
        assert_eq!(store.local_storage_get("https://other.com", "key3"), None);

        store.save().expect("failed to save");
        drop(store);

        let store2 = SessionStore::load("test", dir.path()).expect("failed to reload session");
        assert_eq!(
            store2.local_storage_get("https://example.com", "key2"),
            Some("value2".to_string())
        );
        assert_eq!(store2.local_storage_get("https://other.com", "key3"), None);
    }

    #[test]
    fn test_local_storage_persistence() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let store = SessionStore::load("persist-test", dir.path()).expect("failed to load session");

        store.local_storage_set("https://example.com", "token", "abc123");
        store.local_storage_set("https://example.com", "theme", "dark");
        store.save().expect("failed to save");
        drop(store);

        let store2 =
            SessionStore::load("persist-test", dir.path()).expect("failed to reload session");
        assert_eq!(
            store2.local_storage_get("https://example.com", "token"),
            Some("abc123".to_string())
        );
        assert_eq!(
            store2.local_storage_get("https://example.com", "theme"),
            Some("dark".to_string())
        );
        assert_eq!(store2.local_storage_keys("https://example.com").len(), 2);
    }

    #[test]
    fn test_list_sessions() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let sessions = SessionStore::list_sessions(dir.path()).expect("failed to list sessions");
        assert!(sessions.is_empty());

        SessionStore::load("alpha", dir.path()).expect("failed to load session");
        SessionStore::load("beta", dir.path()).expect("failed to load session");
        SessionStore::load("gamma", dir.path()).expect("failed to load session");

        let sessions = SessionStore::list_sessions(dir.path()).expect("failed to list sessions");
        assert_eq!(sessions, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn test_destroy_session() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        SessionStore::load("to-delete", dir.path()).expect("failed to load session");

        assert!(dir.path().join("sessions/to-delete").exists());
        SessionStore::destroy("to-delete", dir.path()).expect("failed to destroy session");
        assert!(!dir.path().join("sessions/to-delete").exists());
    }

    #[test]
    fn test_parse_auth_bearer() {
        let result = SessionStore::parse_auth_header("bearer:my-secret-token");
        assert_eq!(result.expect("expected result").1, "Bearer my-secret-token");
    }

    #[test]
    fn test_parse_auth_basic() {
        let result = SessionStore::parse_auth_header("basic:myuser:mypass");
        let (name, value) = result.expect("expected result");
        assert_eq!(name, "Authorization");
        assert!(value.starts_with("Basic "));
    }

    #[test]
    fn test_parse_auth_invalid() {
        assert!(SessionStore::parse_auth_header("invalid").is_none());
        assert!(SessionStore::parse_auth_header("bearer:").is_none());
        assert!(SessionStore::parse_auth_header("basic:").is_none());
    }

    #[test]
    fn test_parse_custom_header() {
        let result = SessionStore::parse_custom_header("Content-Type: application/json");
        let (name, value) = result.expect("expected result");
        assert_eq!(name, "Content-Type");
        assert_eq!(value, "application/json");
    }

    #[test]
    fn test_parse_custom_header_invalid() {
        assert!(SessionStore::parse_custom_header("no-colon").is_none());
        assert!(SessionStore::parse_custom_header(":no-name").is_none());
        assert!(SessionStore::parse_custom_header(":").is_none());
    }

    #[test]
    fn test_session_cookies_multiple_domains() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let store = SessionStore::load("multi", dir.path()).expect("failed to load session");

        let url1: Url = "https://site1.com".parse().expect("failed to parse url");
        let url2: Url = "https://site2.com".parse().expect("failed to parse url");

        let h1 = HeaderValue::from_static("a=1; Domain=site1.com");
        let mut iter1 = std::iter::once(&h1);
        store.set_cookies(&mut iter1, &url1);

        let h2 = HeaderValue::from_static("b=2; Domain=site2.com");
        let mut iter2 = std::iter::once(&h2);
        store.set_cookies(&mut iter2, &url2);

        assert_eq!(store.cookie_count(), 2);

        let c1 = store.cookies(&url1).expect("expected cookies");
        assert_eq!(c1.to_str().unwrap(), "a=1");

        let c2 = store.cookies(&url2).expect("expected cookies");
        assert_eq!(c2.to_str().unwrap(), "b=2");

        store.save().expect("failed to save");
        drop(store);

        let store2 = SessionStore::load("multi", dir.path()).expect("failed to reload session");
        assert_eq!(store2.cookie_count(), 2);
    }

    #[test]
    fn test_base64_encode() {
        use base64::{Engine, engine::general_purpose::STANDARD as B64};
        assert_eq!(B64.encode("user:pass"), "dXNlcjpwYXNz");
        assert_eq!(B64.encode(""), "");
        assert_eq!(B64.encode("a"), "YQ==");
    }
}
