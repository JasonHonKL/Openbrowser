use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

/// Send cookies extracted from the visual webview to the headless CDP server.
///
/// Connects to the CDP WebSocket endpoint and sends `Network.setCookie` commands
/// for each cookie in the string. The CDP server writes to the shared cookie jar
/// on the `Arc<App>`, so the agent's existing session will see the new cookies.
pub async fn send_cookies_to_headless(
    port: u16,
    cookie_string: &str,
    url: &str,
) -> Result<usize, String> {
    let ws_url = format!("ws://127.0.0.1:{}", port);

    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .map_err(|e| format!("failed to connect to CDP at {}: {}", ws_url, e))?;

    // Extract domain from URL for cookie scoping
    let domain = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| "example.com".to_string());

    let cookies = parse_cookie_string(cookie_string);
    let count = cookies.len();

    for (i, (name, value)) in cookies.iter().enumerate() {
        let msg = serde_json::json!({
            "id": i + 1,
            "method": "Network.setCookie",
            "params": {
                "name": name,
                "value": value,
                "domain": domain,
                "path": "/"
            }
        });

        ws_stream
            .send(Message::Text(msg.to_string().into()))
            .await
            .map_err(|e| format!("failed to send setCookie: {}", e))?;

        // Read response (discard)
        if let Some(Ok(_response)) = ws_stream.next().await {
            // Response received, cookie set
        }
    }

    let _ = ws_stream.close(None).await;
    Ok(count)
}

/// Parse a cookie string like "name1=value1; name2=value2" into pairs.
fn parse_cookie_string(cookie_string: &str) -> Vec<(String, String)> {
    cookie_string
        .split(';')
        .filter_map(|pair| {
            let pair = pair.trim();
            if pair.is_empty() {
                return None;
            }
            let mut parts = pair.splitn(2, '=');
            let name = parts.next()?.trim().to_string();
            let value = parts.next()?.trim().to_string();
            if name.is_empty() {
                return None;
            }
            Some((name, value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cookie_string() {
        let cookies = parse_cookie_string("cf_clearance=abc123; _ga=GA1.2.123");
        assert_eq!(cookies.len(), 2);
        assert_eq!(
            cookies[0],
            ("cf_clearance".to_string(), "abc123".to_string())
        );
        assert_eq!(cookies[1], ("_ga".to_string(), "GA1.2.123".to_string()));
    }

    #[test]
    fn test_parse_empty() {
        assert!(parse_cookie_string("").is_empty());
    }

    #[test]
    fn test_parse_single() {
        let cookies = parse_cookie_string("token=xyz");
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0], ("token".to_string(), "xyz".to_string()));
    }
}
