// src/web/mod.rs
use axum::{
    body::Body,
    http::{header, HeaderValue, Response, StatusCode, Uri},
    response::IntoResponse,
};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "web/dist/"]
#[prefix = ""]
struct Asset;

pub async fn static_handler(uri: Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();
    if path.is_empty() {
        path = "index.html".to_string();
    }

    match Asset::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, HeaderValue::from_str(mime.as_ref()).unwrap())
                .header(header::CACHE_CONTROL, cache_control_for(&path))
                .header("X-Content-Type-Options", "nosniff")
                .header("X-Frame-Options", "SAMEORIGIN")
                .header("Referrer-Policy", "strict-origin-when-cross-origin")
                .body(Body::from(content.data))
                .unwrap()
        }
        None => {
            // SPA fallback: return index.html for unknown HTML/client routes
            if let Some(index) = Asset::get("index.html") {
                Response::builder()
                    .header(header::CONTENT_TYPE, HeaderValue::from_static("text/html"))
                    .header(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"))
                    .header("X-Content-Type-Options", "nosniff")
                    .header("X-Frame-Options", "SAMEORIGIN")
                    .header("Referrer-Policy", "strict-origin-when-cross-origin")
                    .body(Body::from(index.data))
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("404 Not Found"))
                    .unwrap()
            }
        }
    }
}

/// Vite emits content-hashed filenames under `assets/` (e.g. `assets/index-a1b2c3d4.js`) — those
/// can never go stale under a given name, so they're safe to cache for a year as immutable.
/// `index.html` (and anything else unhashed, like `public/`-copied files) must always be
/// revalidated since a redeploy can change its content without changing its URL.
fn cache_control_for(path: &str) -> HeaderValue {
    if path == "index.html" {
        HeaderValue::from_static("no-cache")
    } else if path.starts_with("assets/") {
        HeaderValue::from_static("public, max-age=31536000, immutable")
    } else {
        HeaderValue::from_static("public, max-age=3600")
    }
}
