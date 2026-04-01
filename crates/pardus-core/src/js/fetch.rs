//! Fetch operation for deno_core.
//!
//! Provides JavaScript fetch API via reqwest.

use deno_core::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ==================== Fetch Op ====================

#[op2]
#[serde]
pub async fn op_fetch(#[serde] args: FetchArgs) -> FetchResult {
    let client = reqwest::Client::new();
    let mut req = match args.method.as_str() {
        "POST" => client.post(&args.url),
        "PUT" => client.put(&args.url),
        "DELETE" => client.delete(&args.url),
        "PATCH" => client.patch(&args.url),
        _ => client.get(&args.url),
    };

    for (k, v) in &args.headers {
        req = req.header(k, v);
    }

    if let Some(body) = &args.body {
        req = req.body(body.clone());
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let status_text = resp.status().canonical_reason().unwrap_or("").to_string();
            let headers: HashMap<String, String> = resp
                .headers()
                .iter()
                .filter_map(|(k, v)| Some((k.to_string(), v.to_str().ok()?.to_string())))
                .collect();
            let body = resp.text().await.unwrap_or_default();

            FetchResult {
                ok: status >= 200 && status < 300,
                status,
                status_text,
                headers,
                body,
            }
        }
        Err(_) => FetchResult {
            ok: false,
            status: 0,
            status_text: "Network Error".to_string(),
            headers: HashMap::new(),
            body: String::new(),
        },
    }
}

// ==================== Types ====================

#[derive(Deserialize)]
pub struct FetchArgs {
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}

#[derive(Serialize)]
pub struct FetchResult {
    pub ok: bool,
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

// ==================== Legacy Types (for external use) ====================

#[derive(Debug, Serialize, Deserialize)]
pub struct FetchRequest {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
}

fn default_method() -> String {
    "GET".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FetchResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub ok: bool,
}

/// Execute a fetch request via reqwest (legacy helper for non-op usage).
pub async fn execute_fetch(
    client: reqwest::Client,
    request: FetchRequest,
) -> anyhow::Result<FetchResponse> {
    let mut builder = client.request(
        reqwest::Method::from_bytes(request.method.as_bytes()).unwrap_or(reqwest::Method::GET),
        &request.url,
    );

    for (k, v) in &request.headers {
        builder = builder.header(k.as_str(), v.as_str());
    }
    if let Some(body) = &request.body {
        builder = builder.body(body.clone());
    }

    let response = builder.send().await?;
    let status = response.status().as_u16();
    let status_text = response
        .status()
        .canonical_reason()
        .unwrap_or("")
        .to_string();
    let ok = response.status().is_success();

    let mut headers = HashMap::new();
    for (k, v) in response.headers() {
        if let Ok(v_str) = v.to_str() {
            headers.insert(k.to_string(), v_str.to_string());
        }
    }

    let body = response.text().await.unwrap_or_default();

    Ok(FetchResponse {
        status,
        status_text,
        headers,
        body,
        ok,
    })
}
