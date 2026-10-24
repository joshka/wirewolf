//! The read/write policy for each sign-in mode, and the OIDC flow against a mock issuer.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;
use wirewolf_core::config;
use wirewolf_core::{Hub, demo};
use wirewolf_web::auth::AuthState;

#[path = "../examples/mock_issuer.rs"]
#[allow(dead_code)]
mod mock_issuer;
use mock_issuer::{Issuer, start_issuer};

const PROXY: &str = "10.0.0.2:5000";
const ELSEWHERE: &str = "192.168.1.50:5000";

fn app(auth: AuthState, peer: &str) -> Router {
    let hub = Hub::new(demo::meta("healthy"));
    wirewolf_web::router_with_auth(hub, Arc::new(auth)).layer(MockConnectInfo(peer.parse::<SocketAddr>().unwrap()))
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, String) {
    let res = app.clone().oneshot(req).await.unwrap();
    let (parts, body) = res.into_parts();
    let body = String::from_utf8(body.collect().await.unwrap().to_bytes().to_vec()).unwrap();
    (parts.status, parts.headers, body)
}

fn read() -> axum::http::request::Builder {
    Request::get("/api/v1/speed")
}

fn write() -> axum::http::request::Builder {
    Request::post("/api/v1/speed/run")
}

fn header_auth() -> config::Auth {
    let mut c = config::Auth::default();
    c.trusted_header.user = "Remote-User".into();
    c.trusted_header.email = "Remote-Email".into();
    c.trusted_header.groups = "Remote-Groups".into();
    c.trusted_header.proxies = vec!["10.0.0.0/24".into()];
    c
}

#[tokio::test]
async fn no_auth_reads_open_writes_refused() {
    let app = app(AuthState::from_config(&config::Auth::default()).unwrap(), ELSEWHERE);
    assert_eq!(send(&app, read().body(Body::empty()).unwrap()).await.0, StatusCode::OK);
    assert_eq!(send(&app, write().body(Body::empty()).unwrap()).await.0, StatusCode::FORBIDDEN);
    let put = Request::put("/api/v1/config").header("content-type", "application/json").body(Body::from("{}")).unwrap();
    assert_eq!(send(&app, put).await.0, StatusCode::FORBIDDEN, "settings writes are gated too");
    let (_, _, body) = send(&app, Request::get("/api/v1/session").body(Body::empty()).unwrap()).await;
    assert!(body.contains(r#""can_write":false"#), "{body}");
}

#[tokio::test]
async fn no_auth_writes_when_allowed_but_not_cross_site() {
    let c = config::Auth { allow_unauthenticated_writes: true, ..config::Auth::default() };
    let app = app(AuthState::from_config(&c).unwrap(), ELSEWHERE);
    assert_eq!(send(&app, write().body(Body::empty()).unwrap()).await.0, StatusCode::ACCEPTED);
    let cross = write().header("host", "wirewolf.lan:7465").header("origin", "https://evil.example").body(Body::empty()).unwrap();
    assert_eq!(send(&app, cross).await.0, StatusCode::FORBIDDEN);
    let same = write().header("host", "wirewolf.lan:7465").header("origin", "http://wirewolf.lan:7465").body(Body::empty()).unwrap();
    assert_eq!(send(&app, same).await.0, StatusCode::ACCEPTED);
    let fetch = write().header("sec-fetch-site", "cross-site").body(Body::empty()).unwrap();
    assert_eq!(send(&app, fetch).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn trusted_header_only_from_proxies() {
    let from_proxy = app(AuthState::from_config(&header_auth()).unwrap(), PROXY);
    let elsewhere = app(AuthState::from_config(&header_auth()).unwrap(), ELSEWHERE);
    let as_alice = || write().header("remote-user", "alice").body(Body::empty()).unwrap();
    assert_eq!(send(&from_proxy, as_alice()).await.0, StatusCode::ACCEPTED);
    assert_eq!(send(&elsewhere, as_alice()).await.0, StatusCode::UNAUTHORIZED, "spoofed header must not count");
    assert_eq!(send(&from_proxy, write().body(Body::empty()).unwrap()).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(send(&elsewhere, read().body(Body::empty()).unwrap()).await.0, StatusCode::OK);
    let session = Request::get("/api/v1/session").header("remote-user", "alice").header("remote-email", "a@x.org").body(Body::empty()).unwrap();
    let (_, _, body) = send(&from_proxy, session).await;
    assert!(body.contains(r#""via":"header""#) && body.contains(r#""can_write":true"#), "{body}");
}

#[tokio::test]
async fn trusted_header_allowlists_and_read_gate() {
    let mut c = header_auth();
    c.allowed_groups = vec!["admins".into()];
    c.require_for_reads = true;
    let app = app(AuthState::from_config(&c).unwrap(), PROXY);
    let with = |groups: &str| write().header("remote-user", "bob").header("remote-groups", groups).body(Body::empty()).unwrap();
    assert_eq!(send(&app, with("users, admins")).await.0, StatusCode::ACCEPTED);
    assert_eq!(send(&app, with("users")).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(send(&app, read().body(Body::empty()).unwrap()).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(send(&app, Request::get("/api/v1/session").body(Body::empty()).unwrap()).await.0, StatusCode::OK, "session stays readable");
    let read_as = read().header("remote-user", "bob").header("remote-groups", "admins").body(Body::empty()).unwrap();
    assert_eq!(send(&app, read_as).await.0, StatusCode::OK);
}

#[test]
fn header_mode_needs_proxies() {
    let mut c = header_auth();
    c.trusted_header.proxies.clear();
    assert!(AuthState::from_config(&c).is_err());
}

fn oidc_config(issuer: &Issuer, dir: &std::path::Path) -> config::Auth {
    config::Auth {
        issuer: issuer.url.clone(),
        client_id: "wirewolf".into(),
        client_secret: "secret".into(),
        redirect_url: "http://wirewolf.lan/auth/callback".into(),
        allowed_emails: vec!["Alice@Example.com".into()],
        session_key_file: dir.join("session.key"),
        ..config::Auth::default()
    }
}

fn cookies(headers: &axum::http::HeaderMap) -> String {
    headers.get_all(header::SET_COOKIE).iter().map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned()).collect::<Vec<_>>().join("; ")
}

fn query_param(url: &str, name: &str) -> String {
    let query = url.split_once('?').unwrap().1;
    query.split('&').find_map(|kv| kv.strip_prefix(&format!("{name}="))).unwrap().to_owned()
}

/// Signs in through `/auth/login` and `/auth/callback`; returns the callback response.
async fn sign_in(app: &Router, issuer: &Issuer, code: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let (status, headers, _) = send(app, Request::get("/auth/login?return_to=/settings").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let location = headers[header::LOCATION].to_str().unwrap().to_owned();
    assert!(location.starts_with(&format!("{}/authorize?", issuer.url)), "{location}");
    assert_eq!(query_param(&location, "code_challenge_method"), "S256");
    *issuer.expect.lock().unwrap() = Some((query_param(&location, "nonce"), query_param(&location, "code_challenge")));
    let state = query_param(&location, "state");
    let callback = Request::get(format!("/auth/callback?code={code}&state={state}")).header(header::COOKIE, cookies(&headers)).body(Body::empty()).unwrap();
    send(app, callback).await
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("wirewolf-auth-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test]
async fn oidc_sign_in_then_write_with_csrf() {
    let issuer = start_issuer("127.0.0.1:0", "alice@example.com").await;
    let dir = temp_dir("flow");
    let config = oidc_config(&issuer, &dir);
    let app = app(AuthState::from_config(&config).unwrap(), ELSEWHERE);

    assert_eq!(send(&app, write().body(Body::empty()).unwrap()).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(send(&app, read().body(Body::empty()).unwrap()).await.0, StatusCode::OK);

    let (status, headers, body) = sign_in(&app, &issuer, "good-code").await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert_eq!(headers[header::LOCATION], "/settings");
    let set_cookie = headers.get_all(header::SET_COOKIE).iter().map(|v| v.to_str().unwrap()).find(|c| c.starts_with("wirewolf_session=")).unwrap();
    assert!(set_cookie.contains("HttpOnly") && set_cookie.contains("SameSite=Lax"), "{set_cookie}");
    let session_cookie = set_cookie.split(';').next().unwrap().to_owned();

    let (_, _, body) = send(&app, Request::get("/api/v1/session").header(header::COOKIE, &session_cookie).body(Body::empty()).unwrap()).await;
    let session: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(session["user"]["email"], "alice@example.com");
    assert_eq!(session["can_write"], true);
    let csrf = session["csrf"].as_str().unwrap().to_owned();

    let no_csrf = write().header(header::COOKIE, &session_cookie).body(Body::empty()).unwrap();
    assert_eq!(send(&app, no_csrf).await.0, StatusCode::FORBIDDEN);
    let with_csrf = || write().header(header::COOKIE, &session_cookie).header("x-wirewolf-csrf", &csrf).body(Body::empty()).unwrap();
    assert_eq!(send(&app, with_csrf()).await.0, StatusCode::ACCEPTED);

    // The key file survives a restart, so the session does too.
    let restarted = self::app(AuthState::from_config(&config).unwrap(), ELSEWHERE);
    let req = write().header(header::COOKIE, &session_cookie).header("x-wirewolf-csrf", &csrf).body(Body::empty()).unwrap();
    assert_eq!(send(&restarted, req).await.0, StatusCode::ACCEPTED);
    // Taken off the allowlist: the same cookie no longer signs in after a restart.
    let removed = config::Auth { allowed_emails: vec!["bob@example.com".into()], ..config.clone() };
    let removed = self::app(AuthState::from_config(&removed).unwrap(), ELSEWHERE);
    let req = write().header(header::COOKIE, &session_cookie).header("x-wirewolf-csrf", &csrf).body(Body::empty()).unwrap();
    assert_eq!(send(&removed, req).await.0, StatusCode::UNAUTHORIZED);

    let logout = Request::post("/auth/logout").header(header::COOKIE, &session_cookie).header("x-wirewolf-csrf", &csrf).body(Body::empty()).unwrap();
    let (status, headers, _) = send(&app, logout).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(cookies(&headers).starts_with("wirewolf_session="));
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn oidc_refuses_bad_code_wrong_user_and_forged_state() {
    let issuer = start_issuer("127.0.0.1:0", "mallory@example.com").await;
    let dir = temp_dir("refuse");
    let app = app(AuthState::from_config(&oidc_config(&issuer, &dir)).unwrap(), ELSEWHERE);
    assert_eq!(sign_in(&app, &issuer, "good-code").await.0, StatusCode::FORBIDDEN, "not in allowed_emails");
    let unverified = start_issuer("127.0.0.1:0", "unverified:alice@example.com").await;
    let app2 = self::app(AuthState::from_config(&oidc_config(&unverified, &dir)).unwrap(), ELSEWHERE);
    assert_eq!(sign_in(&app2, &unverified, "good-code").await.0, StatusCode::FORBIDDEN, "an email without email_verified doesn't count");
    assert_eq!(sign_in(&app, &issuer, "bad-code").await.0, StatusCode::BAD_GATEWAY);
    let forged = Request::get("/auth/callback?code=good-code&state=x").body(Body::empty()).unwrap();
    assert_eq!(send(&app, forged).await.0, StatusCode::BAD_REQUEST);
    let open_redirect = Request::get("/auth/login?return_to=//evil.example").body(Body::empty()).unwrap();
    assert_eq!(send(&app, open_redirect).await.0, StatusCode::SEE_OTHER);
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn oidc_issuer_down_does_not_stop_startup() {
    let dir = temp_dir("down");
    let config = config::Auth {
        issuer: "http://127.0.0.1:9".into(),
        client_id: "wirewolf".into(),
        redirect_url: "http://wirewolf.lan/auth/callback".into(),
        session_key_file: dir.join("k"),
        ..config::Auth::default()
    };
    let err = AuthState::from_config(&config).err().expect("no allowlist is refused");
    assert!(err.contains("allow_any_user"), "{err}");
    let config = config::Auth { allow_any_user: true, ..config };
    let app = app(AuthState::from_config(&config).unwrap(), ELSEWHERE);
    assert_eq!(send(&app, Request::get("/auth/login").body(Body::empty()).unwrap()).await.0, StatusCode::BAD_GATEWAY);
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn unknown_host_names_are_refused() {
    let auth = AuthState::from_config(&config::Auth { redirect_url: "https://ww.example.org/auth/callback".into(), ..header_auth() })
        .unwrap()
        .with_allowed_hosts(&["dash.example.com".into(), "*.example.net".into()]);
    let app = app(auth, ELSEWHERE);
    let status = |host: &'static str| {
        let app = app.clone();
        async move { send(&app, read().header("host", host).body(Body::empty()).unwrap()).await.0 }
    };
    for ok in ["192.0.2.7:7465", "[2001:db8::1]:7465", "localhost:7465", "nas", "wirewolf.local", "pi.home.arpa", "box.tail1234.ts.net", "dash.example.com", "a.example.net", "ww.example.org"] {
        assert_eq!(status(ok).await, StatusCode::OK, "{ok}");
    }
    // A rebinding page sends its own public name.
    for bad in ["evil.example:7465", "example.net", "dash.example.com.evil.example"] {
        assert_eq!(status(bad).await, StatusCode::MISDIRECTED_REQUEST, "{bad}");
    }
}
