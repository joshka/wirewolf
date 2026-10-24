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
use wirewolf_core::config::{self, Config};
use wirewolf_core::editor::FieldError;
use wirewolf_core::brief;
use wirewolf_core::model::{Activity, ActivityKind, Event, LogLine};
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
        .layer(Extension(hub.clone()))
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
        .route("/activity", get(activity))
        .route("/logs", get(logs))
        .route("/logs/events", get(log_events))
        .route("/logs/filter", axum::routing::put(put_log_filter))
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
    tokio::spawn(log_shutdown());
    axum::serve(listener, app).await
}

/// Logs the signal that stops the server, then exits as the signal would have.
async fn log_shutdown() {
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => drop(s.recv().await),
            Err(_) => std::future::pending().await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    let (name, code) = tokio::select! {
        _ = tokio::signal::ctrl_c() => ("interrupt", 130),
        () = term => ("terminate", 143),
    };
    tracing::info!("stopping on {name} signal");
    std::process::exit(code);
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
async fn put_config(State(hub): State<Hub>, access: Option<Extension<auth::Access>>, Json(body): Json<serde_json::Value>) -> Response {
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
            let before = hub.config().map(|v| v.config);
            let actor = access.and_then(|a| a.0.user);
            if let Some(before) = &before {
                let changed = changed_keys(before, &view.config);
                if !changed.is_empty() {
                    hub.record(ActivityKind::Config, format!("Settings saved: {}", changed.join(", ")), None, actor.clone());
                }
                if before.log.filter != config.log.filter {
                    apply_log_filter(&hub, &config.log.filter, actor);
                }
            }
            hub.set_config(view.clone());
            hub.send(Command::ApplyConfig(Arc::new(config)));
            Json(view).into_response()
        }
        Err(e) => {
            tracing::debug!("settings not saved: {}", e.iter().map(|e| format!("{}: {}", e.key, e.message)).collect::<Vec<_>>().join("; "));
            errors(StatusCode::UNPROCESSABLE_ENTITY, e)
        }
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

/// The dotted keys whose values differ, at section depth plus one (`probe.interval`,
/// `targets`), for the activity timeline. Values are never included.
fn changed_keys(before: &Config, after: &Config) -> Vec<String> {
    let (a, b) = (serde_json::to_value(before).unwrap_or_default(), serde_json::to_value(after).unwrap_or_default());
    let mut keys = Vec::new();
    let (Some(a), Some(b)) = (a.as_object(), b.as_object()) else { return keys };
    for (section, av) in a {
        let bv = &b[section];
        match (av.as_object(), bv.as_object()) {
            (Some(ao), Some(bo)) => keys.extend(ao.iter().filter(|(k, v)| bo.get(*k) != Some(v)).map(|(k, _)| format!("{section}.{k}"))),
            _ if av != bv => keys.push(section.clone()),
            _ => {}
        }
    }
    keys
}

/// Applies a log filter now and notes it in the activity timeline.
fn apply_log_filter(hub: &Hub, filter: &str, actor: Option<String>) -> Option<Result<(), String>> {
    let logs = hub.logs()?;
    let result = logs.set_filter(filter);
    match &result {
        Ok(()) => hub.record(ActivityKind::Config, format!("Log filter set to {filter}"), None, actor),
        Err(e) => tracing::warn!("log.filter {filter:?}: {e}"),
    }
    Some(result)
}

/// Activity entries, newest first. Who made a change is shown only to those who can write.
async fn activity(State(hub): State<Hub>, access: Option<Extension<auth::Access>>, Query(q): Query<ActivityQuery>) -> impl IntoResponse {
    let can_write = access.is_some_and(|a| a.can_write);
    let entries: Vec<Activity> = hub
        .activity()
        .into_iter()
        .filter(|a| q.kind.as_deref().is_none_or(|k| serde_json::to_value(a.kind).ok().and_then(|v| v.as_str().map(|s| s == k)).unwrap_or(false)))
        .take(q.limit.unwrap_or(500).min(1000))
        .map(|a| hide_actor(a, can_write))
        .collect();
    Json(entries)
}

#[derive(Debug, Deserialize)]
struct ActivityQuery {
    kind: Option<String>,
    limit: Option<usize>,
}

fn hide_actor(mut a: Activity, can_write: bool) -> Activity {
    if !can_write {
        a.actor = None;
    }
    a
}

const LOGS_NEED_WRITE: &str = "server logs need permission to change settings";

#[derive(Debug, Deserialize)]
struct LogsQuery {
    /// Only lines with a `seq` above this.
    after: Option<u64>,
    /// The least severe level to include: `error`, `warn`, `info`, `debug`, `trace`.
    level: Option<String>,
    /// Only lines whose target starts with this, for example `wirewolf_probe`.
    target: Option<String>,
    /// Only lines containing this text (case-insensitive).
    q: Option<String>,
    limit: Option<usize>,
}

fn level_rank(level: &str) -> u8 {
    match level {
        "error" => 0,
        "warn" => 1,
        "info" => 2,
        "debug" => 3,
        _ => 4,
    }
}

impl LogsQuery {
    fn keeps(&self, l: &LogLine) -> bool {
        self.level.as_deref().is_none_or(|lv| level_rank(&l.level) <= level_rank(&lv.to_ascii_lowercase()))
            && self.target.as_deref().is_none_or(|t| l.target.starts_with(t))
            && self.q.as_deref().is_none_or(|q| l.message.to_lowercase().contains(&q.to_lowercase()) || l.target.contains(q))
    }
}

/// `GET /logs`: recent server log lines, oldest first, with the filter in effect. Logs name hosts
/// and addresses, so only those who can write may read them.
async fn logs(State(hub): State<Hub>, access: Option<Extension<auth::Access>>, Query(q): Query<LogsQuery>) -> Response {
    if !access.is_some_and(|a| a.can_write) {
        return (StatusCode::FORBIDDEN, LOGS_NEED_WRITE).into_response();
    }
    let Some(logs) = hub.logs() else { return StatusCode::NOT_FOUND.into_response() };
    let mut lines: Vec<LogLine> = logs.lines(q.after.unwrap_or(0)).into_iter().filter(|l| q.keeps(l)).collect();
    let limit = q.limit.unwrap_or(logs.capacity());
    if lines.len() > limit {
        lines.drain(..lines.len() - limit);
    }
    Json(serde_json::json!({ "filter": logs.filter(), "capacity": logs.capacity(), "lines": lines })).into_response()
}

/// `GET /logs/events`: new log lines as `log` server-sent events, with the same filters as
/// `GET /logs`.
async fn log_events(State(hub): State<Hub>, access: Option<Extension<auth::Access>>, Query(q): Query<LogsQuery>) -> Response {
    if !access.is_some_and(|a| a.can_write) {
        return (StatusCode::FORBIDDEN, LOGS_NEED_WRITE).into_response();
    }
    let Some(logs) = hub.logs() else { return StatusCode::NOT_FOUND.into_response() };
    let Ok(permit) = EVENT_STREAMS.clone().try_acquire_owned() else {
        return (StatusCode::SERVICE_UNAVAILABLE, [(axum::http::header::RETRY_AFTER, "30")], "too many live connections").into_response();
    };
    let q = Arc::new(q);
    let stream = BroadcastStream::new(logs.subscribe()).filter_map(move |l| {
        let q = q.clone();
        async move { l.ok().filter(|l| q.keeps(l)) }
    });
    let stream = stream.map(move |line| {
        let _ = &permit;
        Ok::<_, Infallible>(SseEvent::default().event("log").data(serde_json::to_string(&line).expect("model types serialize")))
    });
    Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
}

#[derive(Debug, Deserialize)]
struct FilterBody {
    filter: String,
}

/// `PUT /logs/filter`: changes the log filter now, and saves it as `log.filter` when this
/// server can save settings and no env var pins it.
async fn put_log_filter(State(hub): State<Hub>, access: Option<Extension<auth::Access>>, Json(body): Json<FilterBody>) -> Response {
    let errors = |status, errors: Vec<FieldError>| (status, Json(serde_json::json!({ "errors": errors }))).into_response();
    if hub.logs().is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let filter = body.filter.trim().to_string();
    if let Err(e) = wirewolf_core::logs::check_filter(&filter) {
        return errors(StatusCode::UNPROCESSABLE_ENTITY, vec![FieldError::new("log.filter", e)]);
    }
    let actor = access.and_then(|a| a.0.user);
    let pinned = hub.config().is_some_and(|v| matches!(v.sources.get("log.filter"), Some(config::Source::Env | config::Source::Flag)));
    let mut saved = false;
    if let (Some(editor), Some(view)) = (hub.editor(), hub.config())
        && !editor.read_only()
        && !pinned
        && view.config.log.filter != filter
    {
        let mut submitted = view.config.clone();
        submitted.log.filter = filter.clone();
        match tokio::task::spawn_blocking(move || editor.apply(submitted).map(|l| (editor.view(&l), l.config))).await.expect("editor doesn't panic") {
            // Only the log filter changed, so the probes have nothing to reload.
            Ok((view, _)) => {
                hub.set_config(view);
                saved = true;
            }
            Err(e) => return errors(StatusCode::UNPROCESSABLE_ENTITY, e),
        }
    }
    match apply_log_filter(&hub, &filter, actor) {
        Some(Err(e)) => errors(StatusCode::UNPROCESSABLE_ENTITY, vec![FieldError::new("log.filter", e)]),
        _ => Json(serde_json::json!({ "filter": filter, "saved": saved })).into_response(),
    }
}

/// Open `/events` streams at once. Each is cheap, but anyone who can read could open thousands.
const MAX_EVENT_STREAMS: usize = 64;
static EVENT_STREAMS: std::sync::LazyLock<Arc<tokio::sync::Semaphore>> = std::sync::LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(MAX_EVENT_STREAMS)));

/// Holds a live-connection slot and logs when the connection opens and closes.
struct StreamPermit(#[allow(dead_code)] tokio::sync::OwnedSemaphorePermit);

impl StreamPermit {
    fn new(permit: tokio::sync::OwnedSemaphorePermit) -> Self {
        tracing::debug!(open = MAX_EVENT_STREAMS - EVENT_STREAMS.available_permits(), "live connection opened");
        Self(permit)
    }
}

impl Drop for StreamPermit {
    fn drop(&mut self) {
        // The slot is freed after this runs.
        tracing::debug!(open = MAX_EVENT_STREAMS - EVENT_STREAMS.available_permits() - 1, "live connection closed");
    }
}

/// Server-sent events. The current snapshot is sent first so clients render immediately.
async fn events(State(hub): State<Hub>, access: Option<Extension<auth::Access>>) -> Response {
    let can_write = access.is_some_and(|a| a.can_write);
    let Ok(permit) = EVENT_STREAMS.clone().try_acquire_owned() else {
        tracing::warn!("refused a live connection: all {MAX_EVENT_STREAMS} are in use");
        return (StatusCode::SERVICE_UNAVAILABLE, [(axum::http::header::RETRY_AFTER, "30")], "too many live connections").into_response();
    };
    let permit = StreamPermit::new(permit);
    let initial = hub.snapshot().map(Event::Snapshot);
    let live = BroadcastStream::new(hub.subscribe()).filter_map(|e| async move { e.ok() });
    let stream = futures_util::stream::iter(initial).chain(live).map(move |event| {
        let data = match &event {
            Event::Snapshot(x) => serde_json::to_string(x),
            Event::Sample(x) => serde_json::to_string(x),
            Event::Speed(x) => serde_json::to_string(x),
            Event::Incident(x) => serde_json::to_string(x),
            Event::Activity(x) => serde_json::to_string(&hide_actor(x.clone(), can_write)),
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

    async fn call(app: &Router, req: Request<Body>) -> (StatusCode, serde_json::Value) {
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
    }

    fn filter_req(filter: &str) -> Request<Body> {
        Request::put("/api/v1/logs/filter").header("content-type", "application/json").body(Body::from(serde_json::json!({ "filter": filter }).to_string())).unwrap()
    }

    #[tokio::test]
    async fn logs_need_write_access() {
        let hub = Hub::new(wirewolf_core::demo::meta("healthy"));
        let logs = wirewolf_core::logs::Logs::new();
        hub.set_logs(logs.clone());
        logs.push("INFO", "wirewolf_probe::measure", "gateway 192.0.2.1 ok");
        logs.push("DEBUG", "wirewolf_web", "request served");
        hub.record(ActivityKind::Config, "Settings saved: probe.interval", None, Some("a@example.com".into()));
        // No sign-in configured and writes off: a reader.
        let closed = router_with_auth(hub.clone(), Arc::new(auth::AuthState::from_config(&Default::default()).unwrap()));
        let get = |uri: &str| Request::get(uri).body(Body::empty()).unwrap();
        assert_eq!(call(&closed, get("/api/v1/logs")).await.0, StatusCode::FORBIDDEN);
        assert_eq!(call(&closed, get("/api/v1/logs/events")).await.0, StatusCode::FORBIDDEN);
        assert_eq!(call(&closed, filter_req("debug")).await.0, StatusCode::FORBIDDEN);
        let (status, body) = call(&closed, get("/api/v1/activity")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body[0]["message"], "Settings saved: probe.interval");
        assert!(body[0]["actor"].is_null());

        let open = router(hub.clone());
        let (status, body) = call(&open, get("/api/v1/logs?level=info")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["filter"], "info");
        assert_eq!(body["lines"].as_array().unwrap().len(), 1);
        let (_, body) = call(&open, get("/api/v1/logs?target=wirewolf_web&after=0")).await;
        assert_eq!(body["lines"][0]["message"], "request served");
        assert_eq!(call(&open, get("/api/v1/activity")).await.1[0]["actor"], "a@example.com");
    }

    #[tokio::test]
    async fn log_filter_changes_live_and_saves() {
        let dir = std::env::temp_dir().join(format!("wirewolf-web-logfilter-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("wirewolf.toml");
        let hub = live_hub(path.clone(), false);
        let logs = wirewolf_core::logs::Logs::new();
        hub.set_logs(logs.clone());
        let app = router(hub.clone());
        let (status, body) = call(&app, filter_req("info,wirewolf_probe=loud")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["errors"][0]["key"], "log.filter");
        let (status, body) = call(&app, filter_req("info,wirewolf_probe=debug")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["saved"], true);
        assert_eq!(logs.filter(), "info,wirewolf_probe=debug");
        assert_eq!(hub.config().unwrap().config.log.filter, "info,wirewolf_probe=debug");
        assert!(std::fs::read_to_string(&path).unwrap().contains("wirewolf_probe=debug"));
        assert!(hub.activity().iter().any(|a| a.message == "Log filter set to info,wirewolf_probe=debug"));

        // Through the settings form too, with the changed keys noted.
        let mut c = serde_json::to_value(hub.config().unwrap().config).unwrap();
        c["log"]["filter"] = "warn".into();
        assert_eq!(put(&hub, c).await.0, StatusCode::OK);
        assert_eq!(logs.filter(), "warn");
        assert!(hub.activity().iter().any(|a| a.message == "Settings saved: log.filter"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
