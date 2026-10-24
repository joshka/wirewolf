//! Web API and dashboard for wirewolf.
//!
//! [`api_router`] is the JSON + SSE contract documented in `docs/api.md`, served under
//! `/api/v1`. [`router_with_auth`] adds sign-in and the read/write policy ([`auth`]), the
//! host-name check against DNS rebinding, security headers, and, with the `ui-shadcn` feature
//! (on by default), the embedded dashboard. [`serve`] and [`serve_with_auth`] bind and run it.
//!
//! Building with `ui-shadcn` compiles the dashboard with Node and pnpm. Set
//! `WIREWOLF_UI_SHADCN_PREBUILT=1` to embed an existing `ui-shadcn/dist` instead.
//!
//! This crate is part of [wirewolf](https://github.com/joshka/wirewolf), a self-hosted network
//! diagnostics service. Most people want the `wirewolf` crate, which installs the binary
//! (`cargo install wirewolf`). This crate's API is internal and may change in any release.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use futures_util::StreamExt;
use jiff::Timestamp;
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use wirewolf_core::config::Config;
use wirewolf_core::editor::FieldError;
use wirewolf_core::brief;
use wirewolf_core::model::Event;
use wirewolf_core::{Command, Hub};

pub mod auth;
#[cfg(feature = "ui-shadcn")]
mod ui_shadcn;

use auth::AuthState;

/// The full app with no sign-in and open writes (the demo). See [`router_with_auth`].
pub fn router(hub: Hub) -> Router {
    router_with_auth(hub, Arc::new(AuthState::open()))
}

/// The full app: the API under `/api/v1`, guarded by [`auth::policy`], plus sign-in and the UI.
pub fn router_with_auth(hub: Hub, auth: Arc<AuthState>) -> Router {
    let api = api_router().merge(auth::api_routes()).layer(axum::middleware::from_fn_with_state(auth.clone(), auth::policy));
    let router = Router::new().nest("/api/v1", api).merge(auth::routes());
    #[cfg(feature = "ui-shadcn")]
    let router = router.merge(ui_shadcn::router());
    router
        .layer(Extension(auth.clone()))
        .layer(axum::middleware::from_fn_with_state(auth, auth::host_guard))
        .layer(axum::middleware::map_response(security_headers))
        .with_state(hub)
}

/// Headers for every response: no MIME sniffing, no framing (clickjacking the settings page), and
/// no referrer leaking a dashboard URL to other sites.
async fn security_headers(mut res: Response) -> Response {
    use axum::http::{HeaderValue, header};
    let h = res.headers_mut();
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("same-origin"));
    res
}

/// The JSON + SSE API. Mount under `/api/v1`.
pub fn api_router() -> Router<Hub> {
    Router::new()
        .route("/meta", get(meta))
        .route("/now", get(now))
        .route("/config", get(config).put(put_config))
        .route("/history", get(history))
        .route("/speed", get(speed))
        .route("/speed/run", post(run_speed))
        .route("/incidents", get(incidents))
        .route("/incidents/{id}", get(incident))
        .route("/events", get(events))
        .route("/brief", get(brief))
        .route("/brief/prompt", get(brief_prompt))
        .route("/demo/scenario", post(set_scenario))
}

/// Serves the demo: no sign-in, open writes.
pub async fn serve(hub: Hub, addr: SocketAddr) -> std::io::Result<()> {
    serve_with_auth(hub, addr, AuthState::open()).await
}

pub async fn serve_with_auth(hub: Hub, addr: SocketAddr, auth: AuthState) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on http://{}", listener.local_addr()?);
    // The peer address decides whether trusted-header sign-in applies.
    let app = router_with_auth(hub, Arc::new(auth)).into_make_service_with_connect_info::<SocketAddr>();
    axum::serve(listener, app).await
}

async fn meta(State(hub): State<Hub>) -> impl IntoResponse {
    Json(hub.meta())
}

/// The settings. Readers who can't write don't see `[auth]` (issuer, client ID, allowlists,
/// proxy addresses) or where the file is saved.
async fn config(State(hub): State<Hub>, access: Option<Extension<auth::Access>>) -> Response {
    match hub.config() {
        Some(mut view) => {
            if !access.is_some_and(|a| a.can_write) {
                view.config.auth = Default::default();
                view.save_path = None;
            }
            Json(view).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Saves settings to the config file and applies them from the next probe cycle. Whether the
/// caller may write is checked by the auth layer before this runs.
async fn put_config(State(hub): State<Hub>, Json(body): Json<serde_json::Value>) -> Response {
    let Some(editor) = hub.editor() else { return StatusCode::NOT_FOUND.into_response() };
    let errors = |status, errors: Vec<FieldError>| (status, Json(serde_json::json!({ "errors": errors }))).into_response();
    if editor.read_only() {
        return errors(StatusCode::FORBIDDEN, vec![FieldError::new("", "settings are read-only on this server")]);
    }
    let submitted: Config = match serde_json::from_value(body) {
        Ok(c) => c,
        Err(e) => return errors(StatusCode::UNPROCESSABLE_ENTITY, vec![FieldError::new("", e.to_string())]),
    };
    match tokio::task::spawn_blocking(move || editor.apply(submitted).map(|l| (editor.view(&l), l.config))).await.expect("editor doesn't panic") {
        Ok((view, config)) => {
            hub.set_config(view.clone());
            hub.send(Command::ApplyConfig(Arc::new(config)));
            Json(view).into_response()
        }
        Err(e) => errors(StatusCode::UNPROCESSABLE_ENTITY, e),
    }
}

async fn now(State(hub): State<Hub>) -> Response {
    match hub.snapshot() {
        Some(snapshot) => Json(snapshot).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    hop: Option<String>,
    since: Option<Timestamp>,
    until: Option<Timestamp>,
}

async fn history(State(hub): State<Hub>, Query(q): Query<HistoryQuery>) -> impl IntoResponse {
    Json(hub.samples(q.hop.as_deref(), q.since, q.until))
}

async fn speed(State(hub): State<Hub>) -> impl IntoResponse {
    Json(hub.speed_runs())
}

async fn run_speed(State(hub): State<Hub>) -> StatusCode {
    if hub.speed_running() {
        return StatusCode::CONFLICT;
    }
    hub.send(Command::RunSpeedTest);
    StatusCode::ACCEPTED
}

async fn incidents(State(hub): State<Hub>) -> impl IntoResponse {
    Json(hub.incidents())
}

async fn incident(State(hub): State<Hub>, Path(id): Path<String>) -> Response {
    match hub.incident(&id) {
        Some(incident) => Json(incident).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct BriefQuery {
    /// `text` (the default) or `json`.
    format: Option<String>,
    /// Defaults to true.
    redact: Option<bool>,
    /// How far back the history summary looks, for example `6h`. Defaults to 24h.
    window: Option<String>,
    /// Describe this incident's evidence instead of the current state.
    incident: Option<String>,
}

/// Builds the brief for a query, or the error response.
fn build_brief(hub: &Hub, q: &BriefQuery) -> Result<brief::Brief, (StatusCode, String)> {
    let bad = |m: String| (StatusCode::UNPROCESSABLE_ENTITY, m);
    let window = match &q.window {
        Some(w) => w.parse::<wirewolf_core::config::Dur>().map_err(bad)?.0,
        None => brief::Options::default().window,
    };
    let base = hub.config().map_or_else(brief::Options::default, |v| brief::Options::for_config(&v.config));
    let options = brief::Options { redact: q.redact.unwrap_or(true), window, ..base };
    let meta = hub.meta();
    let (incidents, samples, speed) = (hub.incidents(), hub.samples(None, None, None), hub.speed_runs());
    let incident = match &q.incident {
        Some(id) => Some(hub.incident(id).ok_or((StatusCode::NOT_FOUND, String::new()))?),
        None => None,
    };
    let snapshot = match &incident {
        Some(i) => i.evidence.clone(),
        None => hub.snapshot().ok_or((StatusCode::NO_CONTENT, String::new()))?,
    };
    // An incident's brief lists the incidents before it, not the ones that came later.
    let start = incident.as_ref().map_or(0, |i| incidents.iter().position(|x| x.id == i.id).unwrap_or(0));
    let input = brief::Input { meta: &meta, snapshot: &snapshot, incident: incident.as_ref(), incidents: &incidents[start..], samples: &samples, speed: &speed };
    Ok(brief::build(input, &options))
}

/// `redact=false` shows public IPs and MAC addresses, so only those who can write may ask for it.
fn refuse_unredacted(q: &BriefQuery, access: Option<Extension<auth::Access>>) -> Option<Response> {
    (q.redact == Some(false) && !access.is_some_and(|a| a.can_write)).then(|| (StatusCode::FORBIDDEN, "redact=false needs permission to change settings").into_response())
}

fn plain_text(text: String) -> Response {
    ([(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")], text).into_response()
}

/// `GET /brief`: a compact, redacted summary for an AI assistant (see `wirewolf_core::brief`).
async fn brief(State(hub): State<Hub>, access: Option<Extension<auth::Access>>, Query(q): Query<BriefQuery>) -> Response {
    if let Some(e) = refuse_unredacted(&q, access) {
        return e;
    }
    match build_brief(&hub, &q) {
        Ok(b) if q.format.as_deref() == Some("json") => Json(b).into_response(),
        Ok(b) => plain_text(b.text()),
        Err(e) => e.into_response(),
    }
}

/// `GET /brief/prompt`: the brief wrapped in a request to explain it, ready to paste.
async fn brief_prompt(State(hub): State<Hub>, access: Option<Extension<auth::Access>>, Query(q): Query<BriefQuery>) -> Response {
    if let Some(e) = refuse_unredacted(&q, access) {
        return e;
    }
    match build_brief(&hub, &q) {
        Ok(b) => plain_text(brief::prompt(&b)),
        Err(e) => e.into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct ScenarioBody {
    id: String,
}

async fn set_scenario(State(hub): State<Hub>, Json(body): Json<ScenarioBody>) -> StatusCode {
    let meta = hub.meta();
    if meta.mode != "demo" {
        return StatusCode::NOT_FOUND;
    }
    if !meta.scenarios.iter().any(|s| s.id == body.id) {
        return StatusCode::UNPROCESSABLE_ENTITY;
    }
    hub.send(Command::SetScenario(body.id));
    StatusCode::ACCEPTED
}

/// Open `/events` streams at once. Each is cheap, but anyone who can read could open thousands.
const MAX_EVENT_STREAMS: usize = 64;
static EVENT_STREAMS: std::sync::LazyLock<Arc<tokio::sync::Semaphore>> = std::sync::LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(MAX_EVENT_STREAMS)));

/// Server-sent events. The current snapshot is sent first so clients render immediately.
async fn events(State(hub): State<Hub>) -> Response {
    let Ok(permit) = EVENT_STREAMS.clone().try_acquire_owned() else {
        return (StatusCode::SERVICE_UNAVAILABLE, [(axum::http::header::RETRY_AFTER, "30")], "too many live connections").into_response();
    };
    let initial = hub.snapshot().map(Event::Snapshot);
    let live = BroadcastStream::new(hub.subscribe()).filter_map(|e| async move { e.ok() });
    let stream = futures_util::stream::iter(initial).chain(live).map(move |event| {
        let data = match &event {
            Event::Snapshot(x) => serde_json::to_string(x),
            Event::Sample(x) => serde_json::to_string(x),
            Event::Speed(x) => serde_json::to_string(x),
            Event::Incident(x) => serde_json::to_string(x),
        }
        .expect("model types serialize");
        // The permit lives as long as the stream, so a closed connection frees its slot.
        let _ = &permit;
        Ok::<_, Infallible>(SseEvent::default().event(event.name()).data(data))
    });
    Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use tower::ServiceExt;
    use wirewolf_core::config;
    use wirewolf_core::editor::Editor;
    use wirewolf_core::model::{ConfigView, Meta};

    fn live_hub(path: std::path::PathBuf, read_only: bool) -> Hub {
        let hub = Hub::new(Meta { version: "0".into(), api_version: "1".into(), mode: "live".into(), host: "h".into(), scenarios: vec![], scenario: None });
        let editor = Arc::new(Editor::new(path.clone(), vec![], |_| vec![], read_only));
        hub.set_config(editor.view(&config::load(Some(&path), false, &[]).unwrap()));
        hub.set_editor(editor);
        hub
    }

    async fn put(hub: &Hub, body: serde_json::Value) -> (StatusCode, serde_json::Value) {
        let req = Request::put("/api/v1/config").header("content-type", "application/json").body(Body::from(body.to_string())).unwrap();
        let res = router(hub.clone()).oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
    }

    #[tokio::test]
    async fn put_config_saves_and_reloads_live() {
        let dir = std::env::temp_dir().join(format!("wirewolf-web-put-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("wirewolf.toml");
        let hub = live_hub(path.clone(), false);
        let mut commands = hub.take_commands().unwrap();
        let mut c = serde_json::to_value(hub.config().unwrap().config).unwrap();
        c["incidents"]["open_after"] = 4.into();
        let (status, body) = put(&hub, c.clone()).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let view: ConfigView = serde_json::from_value(body).unwrap();
        assert_eq!(view.config.incidents.open_after, 4);
        assert_eq!(view.defaults, Config::default());
        assert_eq!(hub.config().unwrap().config.incidents.open_after, 4);
        assert!(std::fs::read_to_string(&path).unwrap().contains("open_after = 4"));
        match commands.try_recv().unwrap() {
            Command::ApplyConfig(cfg) => assert_eq!(cfg.incidents.open_after, 4),
            other => panic!("unexpected {other:?}"),
        }

        c["probe"]["interval"] = "1s".into();
        let (status, body) = put(&hub, c).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["errors"][0]["key"], "probe.interval");
        assert!(commands.try_recv().is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn brief_is_text_json_or_prompt() {
        let hub = Hub::new(wirewolf_core::demo::meta("dns_down"));
        let get = |uri: &'static str| {
            let hub = hub.clone();
            async move {
                let res = router(hub).oneshot(Request::get(uri).body(Body::empty()).unwrap()).await.unwrap();
                let status = res.status();
                (status, String::from_utf8(to_bytes(res.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap())
            }
        };
        assert_eq!(get("/api/v1/brief").await.0, StatusCode::NO_CONTENT);
        hub.publish_snapshot(wirewolf_core::demo::snapshot("dns_down", jiff::Timestamp::now()));
        let (status, text) = get("/api/v1/brief").await;
        assert_eq!(status, StatusCode::OK);
        assert!(text.starts_with("# wirewolf brief v1 · demo") && text.contains("- dns [fail]"), "{text}");
        let (_, json) = get("/api/v1/brief?format=json&window=1h").await;
        assert_eq!(serde_json::from_str::<brief::Brief>(&json).unwrap().blame.as_deref(), Some("dns"));
        let (_, prompt) = get("/api/v1/brief/prompt").await;
        assert!(prompt.starts_with(brief::PROMPT) && prompt.contains("<brief>\n# wirewolf brief"), "{prompt}");
        assert_eq!(get("/api/v1/brief?incident=nope").await.0, StatusCode::NOT_FOUND);
        assert_eq!(get("/api/v1/brief?window=soon").await.0, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn responses_carry_security_headers() {
        let hub = Hub::new(wirewolf_core::demo::meta("dns_down"));
        let res = router(hub).oneshot(Request::get("/api/v1/meta").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.headers()["x-content-type-options"], "nosniff");
        assert_eq!(res.headers()["x-frame-options"], "DENY");
    }

    #[tokio::test]
    async fn readers_who_cannot_write_see_less() {
        let path = std::env::temp_dir().join("wirewolf-web-readers/wirewolf.toml");
        let hub = live_hub(path, false);
        let mut view = hub.config().unwrap();
        view.config.auth.allowed_emails = vec!["a@example.com".into()];
        hub.set_config(view);
        hub.publish_snapshot(wirewolf_core::demo::snapshot("healthy", jiff::Timestamp::now()));
        let closed = router_with_auth(hub.clone(), Arc::new(auth::AuthState::from_config(&Default::default()).unwrap()));
        let get = |app: &Router, uri: &'static str| app.clone().oneshot(Request::get(uri).body(Body::empty()).unwrap());
        let body = to_bytes(get(&closed, "/api/v1/config").await.unwrap().into_body(), 1 << 20).await.unwrap();
        let view: ConfigView = serde_json::from_slice(&body).unwrap();
        assert!(view.config.auth.allowed_emails.is_empty() && view.save_path.is_none());
        assert_eq!(get(&closed, "/api/v1/brief?redact=false").await.unwrap().status(), StatusCode::FORBIDDEN);
        assert_eq!(get(&closed, "/api/v1/brief").await.unwrap().status(), StatusCode::OK);
        let open = router(hub);
        let body = to_bytes(get(&open, "/api/v1/config").await.unwrap().into_body(), 1 << 20).await.unwrap();
        assert_eq!(serde_json::from_slice::<ConfigView>(&body).unwrap().config.auth.allowed_emails, ["a@example.com"]);
        assert_eq!(get(&open, "/api/v1/brief?redact=false").await.unwrap().status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn read_only_settings_refuse_writes() {
        let hub = live_hub(std::env::temp_dir().join("wirewolf-web-ro/wirewolf.toml"), true);
        assert!(!hub.config().unwrap().editable);
        let c = serde_json::to_value(hub.config().unwrap().config).unwrap();
        assert_eq!(put(&hub, c).await.0, StatusCode::FORBIDDEN);
    }
}
