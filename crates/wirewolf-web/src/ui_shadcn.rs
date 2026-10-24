//! The React + shadcn/ui variant, built by Vite (see `build.rs`) and embedded in the binary.
//!
//! Served from the root. Unknown paths fall back to `index.html` so client-side routes such as
//! `/incidents/inc-3` survive a reload; unknown `/api/` paths stay 404.

use std::sync::LazyLock;

use axum::Router;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use base64::Engine;
use rust_embed::RustEmbed;
use sha2::{Digest, Sha256};

#[derive(RustEmbed)]
#[folder = "ui-shadcn/dist"]
struct Assets;

pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/", get(|| asset(String::new())))
        .fallback(|uri: Uri| asset(uri.path().trim_start_matches('/').to_owned()))
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
    if path.starts_with("assets/") || path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    match Assets::get("index.html") {
        Some(index) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache"),
                (header::CONTENT_SECURITY_POLICY, CSP.as_str()),
            ],
            index.data,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// The page's content security policy. Scripts only come from this server, plus the inline
/// theme script in `index.html`, allowed by its hash. Styles allow `'unsafe-inline'` because
/// React and the chart components set inline styles. Probe results are attacker-influenced text
/// (hop names, captive portal and DNS answers), so this backs up React's escaping.
static CSP: LazyLock<String> = LazyLock::new(|| {
    let index = Assets::get("index.html").map(|f| String::from_utf8_lossy(&f.data).into_owned()).unwrap_or_default();
    let hashes: String = inline_scripts(&index)
        .map(|s| format!(" 'sha256-{}'", base64::engine::general_purpose::STANDARD.encode(Sha256::digest(s.as_bytes()))))
        .collect();
    format!(
        "default-src 'self'; script-src 'self'{hashes}; style-src 'self' 'unsafe-inline'; img-src 'self' data:; \
         connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"
    )
});

/// The bodies of `<script>` tags with no attributes (inline scripts).
fn inline_scripts(html: &str) -> impl Iterator<Item = &str> {
    html.split("<script>").skip(1).filter_map(|rest| rest.split_once("</script>").map(|(body, _)| body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csp_allows_the_inline_theme_script_by_hash() {
        let html = "<head><script>let a = 1</script><script type=\"module\" src=\"/x.js\"></script></head>";
        assert_eq!(inline_scripts(html).collect::<Vec<_>>(), ["let a = 1"]);
        let index = String::from_utf8(Assets::get("index.html").unwrap().data.into_owned()).unwrap();
        assert!(index.contains("<script>"), "index.html no longer has an inline script; update CSP");
        assert!(CSP.contains("script-src 'self' 'sha256-"), "{}", *CSP);
    }
}
