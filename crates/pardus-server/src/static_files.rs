use axum::body::Body;
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use include_dir::{include_dir, Dir};

static WEB_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../web/dist");

/// Serve an embedded static file by path.
pub async fn serve_embedded(path: &str) -> Response {
    let path = path.trim_start_matches('/');

    let file_path = if path.is_empty() || path == "/" {
        "index.html"
    } else {
        path
    };

    match WEB_DIR.get_file(file_path) {
        Some(file) => {
            let content_type = guess_content_type(file_path);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(file.contents().to_vec()))
                .unwrap()
        }
        None => {
            // SPA fallback
            match WEB_DIR.get_file("index.html") {
                Some(index) => {
                    Html(index.contents_utf8().unwrap_or_default()).into_response()
                }
                None => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("Not Found"))
                    .unwrap(),
            }
        }
    }
}

/// Serve a static file from the filesystem (dev mode).
pub async fn serve_filesystem(path: &str, web_dir: &str) -> Response {
    use tokio::fs;

    let path = path.trim_start_matches('/');
    let file_path = if path.is_empty() || path == "/" {
        "index.html"
    } else {
        path
    };

    let full_path = format!("{}/{}", web_dir, file_path);

    match fs::read(&full_path).await {
        Ok(bytes) => {
            let content_type = guess_content_type(file_path);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(bytes))
                .unwrap()
        }
        Err(_) => {
            let index_path = format!("{}/index.html", web_dir);
            match fs::read(&index_path).await {
                Ok(bytes) => {
                    Html(String::from_utf8_lossy(&bytes).to_string()).into_response()
                }
                Err(_) => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("Not Found"))
                    .unwrap(),
            }
        }
    }
}

fn guess_content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}
