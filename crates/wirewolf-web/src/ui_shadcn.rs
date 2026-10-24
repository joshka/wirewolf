//! The React + shadcn/ui variant, built by Vite (see `build.rs`) and embedded in the binary.
//!
//! Mounted at `/ui/shadcn/`. Unknown paths fall back to `index.html` so client-side routes
//! such as `/ui/shadcn/incidents/inc-3` survive a reload.

use axum::Router;
use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "ui-shadcn/dist"]
struct Assets;

pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/ui/shadcn", get(|| async { Redirect::permanent("/ui/shadcn/") }))
        .route("/ui/shadcn/", get(|| asset(String::new())))
        .route("/ui/shadcn/{*path}", get(|Path(path): Path<String>| asset(path)))
}

async fn asset(path: String) -> Response {
    if let Some(file) = Assets::get(&path).filter(|_| !path.is_empty()) {
        // Vite fingerprints everything under assets/, so it can be cached forever.
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        let mime = file.metadata.mimetype().to_owned();
        return ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, cache.into())], file.data)
            .into_response();
    }
    if path.starts_with("assets/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    match Assets::get("index.html") {
        Some(index) => (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")],
            index.data,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
