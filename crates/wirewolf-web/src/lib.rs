//! Web API and dashboard for wirewolf.
//!
//! [`api_router`] is the JSON + SSE contract documented in `docs/api.md`. UIs mount alongside
//! it via [`router`].

use std::convert::Infallible;
use std::net::SocketAddr;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::Stream;
use futures_util::StreamExt;
use jiff::Timestamp;
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use wirewolf_core::model::Event;
use wirewolf_core::{Command, Hub};

#[cfg(feature = "ui-shadcn")]
mod ui_shadcn;

/// The full app: the API under `/api/v1` plus the UI.
pub fn router(hub: Hub) -> Router {
    let router = Router::new().nest("/api/v1", api_router());
    #[cfg(feature = "ui-shadcn")]
    let router = router.merge(ui_shadcn::router());
    router.with_state(hub)
}

/// The JSON + SSE API. Mount under `/api/v1`.
pub fn api_router() -> Router<Hub> {
    Router::new()
        .route("/meta", get(meta))
        .route("/now", get(now))
        .route("/config", get(config))
        .route("/history", get(history))
        .route("/speed", get(speed))
        .route("/speed/run", post(run_speed))
        .route("/incidents", get(incidents))
        .route("/incidents/{id}", get(incident))
        .route("/events", get(events))
        .route("/demo/scenario", post(set_scenario))
}

pub async fn serve(hub: Hub, addr: SocketAddr) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on http://{}", listener.local_addr()?);
    axum::serve(listener, router(hub)).await
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
