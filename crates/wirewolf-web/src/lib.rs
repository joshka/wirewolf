//! Web API and dashboard for wirewolf.
//!
//! [`api_router`] is the JSON + SSE contract documented in `docs/api.md`. UIs mount alongside
//! it via [`router`].

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use futures_util::Stream;
use futures_util::StreamExt;
use jiff::Timestamp;
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use wirewolf_core::config::Config;
use wirewolf_core::editor::FieldError;
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
    router.layer(Extension(auth)).with_state(hub)
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

async fn config(State(hub): State<Hub>) -> Response {
    match hub.config() {
        Some(view) => Json(view).into_response(),
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

/// Server-sent events. The current snapshot is sent first so clients render immediately.
async fn events(State(hub): State<Hub>) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let initial = hub.snapshot().map(Event::Snapshot);
    let live = BroadcastStream::new(hub.subscribe()).filter_map(|e| async move { e.ok() });
    let stream = futures_util::stream::iter(initial).chain(live).map(|event| {
        let data = match &event {
            Event::Snapshot(x) => serde_json::to_string(x),
            Event::Sample(x) => serde_json::to_string(x),
            Event::Speed(x) => serde_json::to_string(x),
            Event::Incident(x) => serde_json::to_string(x),
        }
        .expect("model types serialize");
        Ok(SseEvent::default().event(event.name()).data(data))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
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
    async fn read_only_settings_refuse_writes() {
        let hub = live_hub(std::env::temp_dir().join("wirewolf-web-ro/wirewolf.toml"), true);
        assert!(!hub.config().unwrap().editable);
        let c = serde_json::to_value(hub.config().unwrap().config).unwrap();
        assert_eq!(put(&hub, c).await.0, StatusCode::FORBIDDEN);
    }
}
