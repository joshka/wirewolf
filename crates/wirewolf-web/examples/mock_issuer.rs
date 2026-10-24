//! A mock OIDC issuer for trying sign-in locally, and for the tests in `tests/auth.rs`.
//!
//! ```sh
//! cargo run -p wirewolf-web --example mock_issuer -- 127.0.0.1:7499 alice@example.com
//! ```
//!
//! It signs in everyone as the given email, with client id `wirewolf` (any secret). The signing
//! key is a throwaway test key; never trust this issuer for anything real.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Form, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use base64::Engine;
use openidconnect::core::{CoreIdToken, CoreIdTokenClaims, CoreJwsSigningAlgorithm, CoreRsaPrivateSigningKey};
use openidconnect::{Audience, EmptyAdditionalClaims, EndUserEmail, IssuerUrl, JsonWebKeyId, Nonce, PrivateSigningKey, StandardClaims, SubjectIdentifier};
use sha2::Digest;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let addr = args.next().unwrap_or_else(|| "127.0.0.1:7499".into());
    let email = args.next().unwrap_or_else(|| "alice@example.com".into());
    let issuer = start_issuer(&addr, &email).await;
    println!("mock issuer at {} signing in {email}", issuer.url);
    std::future::pending::<()>().await;
}

/// A tiny OIDC issuer: discovery, JWKS, and a token endpoint that checks PKCE.
#[derive(Clone)]
pub struct Issuer {
    pub url: String,
    key: Arc<CoreRsaPrivateSigningKey>,
    /// The nonce and PKCE challenge from the authorize URL wirewolf built.
    pub expect: Arc<Mutex<Option<(String, String)>>>,
    pub email: String,
}

pub async fn start_issuer(addr: &str, email: &str) -> Issuer {
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let pem = include_str!("../tests/fixtures/test-issuer-key.pem");
    let key = CoreRsaPrivateSigningKey::from_pem(pem, Some(JsonWebKeyId::new("k1".into()))).unwrap();
    let issuer = Issuer { url, key: Arc::new(key), expect: Arc::default(), email: email.into() };
    let app = Router::new()
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/jwks", get(|State(i): State<Issuer>| async move { axum::Json(serde_json::json!({ "keys": [i.key.as_verification_key()] })) }))
        .route("/authorize", get(authorize))
        .route("/token", post(token))
        .with_state(issuer.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    issuer
}

async fn discovery(State(i): State<Issuer>) -> axum::Json<serde_json::Value> {
    let u = &i.url;
    axum::Json(serde_json::json!({
        "issuer": u,
        "authorization_endpoint": format!("{u}/authorize"),
        "token_endpoint": format!("{u}/token"),
        "jwks_uri": format!("{u}/jwks"),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
    }))
}

/// Approves everyone at once: remembers the nonce and PKCE challenge, then sends the browser back.
async fn authorize(State(i): State<Issuer>, Query(q): Query<HashMap<String, String>>) -> axum::response::Redirect {
    let get = |k: &str| q.get(k).cloned().unwrap_or_default();
    *i.expect.lock().unwrap() = Some((get("nonce"), get("code_challenge")));
    let back = format!("{}?code=good-code&state={}", get("redirect_uri"), get("state"));
    axum::response::Redirect::to(&back)
}

async fn token(State(i): State<Issuer>, Form(form): Form<HashMap<String, String>>) -> axum::response::Response {
    use axum::response::IntoResponse;
    let (nonce, challenge) = i.expect.lock().unwrap().clone().expect("login first");
    let verifier = form.get("code_verifier").cloned().unwrap_or_default();
    let computed = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier.as_bytes()));
    if computed != challenge || form.get("code").map(String::as_str) != Some("good-code") {
        return (StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({ "error": "invalid_grant" }))).into_response();
    }
    let now = chrono::Utc::now();
    let claims = CoreIdTokenClaims::new(
        IssuerUrl::new(i.url.clone()).unwrap(),
        vec![Audience::new("wirewolf".into())],
        now + chrono::Duration::minutes(5),
        now,
        StandardClaims::new(SubjectIdentifier::new("alice-sub".into())).set_email(Some(EndUserEmail::new(i.email.clone()))).set_email_verified(Some(true)),
        EmptyAdditionalClaims {},
    )
    .set_nonce(Some(Nonce::new(nonce)));
    let id_token = CoreIdToken::new(claims, i.key.as_ref(), CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256, None, None).unwrap();
    axum::Json(serde_json::json!({ "access_token": "at", "token_type": "Bearer", "expires_in": 300, "id_token": id_token.to_string() })).into_response()
}

