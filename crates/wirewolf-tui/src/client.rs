//! A small async client for the wirewolf HTTP API (`docs/api.md`).

use anyhow::{Context, bail};
use jiff::Timestamp;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use wirewolf_core::model::{Event, Incident, Meta, Sample, Snapshot, SpeedRun};

/// A client for one wirewolf server.
#[derive(Debug, Clone)]
pub struct Client {
    base: String,
    http: reqwest::Client,
}

/// The outcome of asking for a speed test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeedRequest {
    Started,
    AlreadyRunning,
}

impl Client {
    /// Creates a client for a server such as `http://127.0.0.1:7465`.
    pub fn new(base: &str) -> anyhow::Result<Self> {
        let base = base.trim_end_matches('/');
        let base = if base.contains("://") {
            base.to_string()
        } else {
            format!("http://{base}")
        };
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()?;
        Ok(Self { base, http })
    }

    /// The server's base URL.
    pub fn base(&self) -> &str {
        &self.base
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{path}", self.base)
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        let url = self.url(path);
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        let response = response.error_for_status()?;
        Ok(response.json().await?)
    }

    pub async fn meta(&self) -> anyhow::Result<Meta> {
        self.get("/meta").await
    }

    /// The current snapshot, or `None` before the first measurement.
    pub async fn now(&self) -> anyhow::Result<Option<Snapshot>> {
        let url = self.url("/now");
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        if response.status() == StatusCode::NO_CONTENT {
            return Ok(None);
        }
        Ok(Some(response.error_for_status()?.json().await?))
    }

    /// Samples for every hop since `since`, in time order.
    pub async fn history(&self, since: Timestamp) -> anyhow::Result<Vec<Sample>> {
        self.get(&format!("/history?since={since}")).await
    }

    /// Speed runs, newest first.
    pub async fn speed(&self) -> anyhow::Result<Vec<SpeedRun>> {
        self.get("/speed").await
    }

    /// Incidents, newest first.
    pub async fn incidents(&self) -> anyhow::Result<Vec<Incident>> {
        self.get("/incidents").await
    }

    pub async fn run_speed(&self) -> anyhow::Result<SpeedRequest> {
        let response = self.http.post(self.url("/speed/run")).send().await?;
        match response.status() {
            StatusCode::CONFLICT => Ok(SpeedRequest::AlreadyRunning),
            status if status.is_success() => Ok(SpeedRequest::Started),
            status => bail!("speed test request failed: {status}"),
        }
    }

    pub async fn set_scenario(&self, id: &str) -> anyhow::Result<()> {
        let response = self
            .http
            .post(self.url("/demo/scenario"))
            .json(&serde_json::json!({ "id": id }))
            .send()
            .await?;
        match response.status() {
            status if status.is_success() => Ok(()),
            StatusCode::NOT_FOUND => bail!("scenarios are only available in demo mode"),
            StatusCode::UNPROCESSABLE_ENTITY => bail!("unknown scenario {id:?}"),
            status => bail!("scenario request failed: {status}"),
        }
    }

    /// Streams live events into `on_event` until the connection ends.
    ///
    /// Returns `Ok(())` when the server closes the stream cleanly.
    pub async fn events(&self, mut on_event: impl FnMut(Event)) -> anyhow::Result<()> {
        let mut response = self
            .http
            .get(self.url("/events"))
            .header("accept", "text/event-stream")
            .send()
            .await?
            .error_for_status()?;
        let mut parser = SseParser::default();
        while let Some(chunk) = response.chunk().await? {
            parser.push(&chunk, &mut on_event);
        }
        Ok(())
    }
}

/// An incremental parser for the subset of Server-Sent Events that wirewolf emits.
#[derive(Debug, Default)]
struct SseParser {
    buffer: String,
}

impl SseParser {
    fn push(&mut self, bytes: &[u8], on_event: &mut impl FnMut(Event)) {
        self.buffer.push_str(&String::from_utf8_lossy(bytes));
        if self.buffer.contains('\r') {
            self.buffer = self.buffer.replace("\r\n", "\n");
        }
        while let Some(end) = self.buffer.find("\n\n") {
            let block: String = self.buffer.drain(..end + 2).collect();
            if let Some(event) = parse_block(&block) {
                on_event(event);
            }
        }
    }
}

fn parse_block(block: &str) -> Option<Event> {
    let mut name = "message";
    let mut data = String::new();
    for line in block.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            name = value.trim();
        } else if let Some(value) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    let data = data.as_str();
    match name {
        "snapshot" => serde_json::from_str(data).ok().map(Event::Snapshot),
        "sample" => serde_json::from_str(data).ok().map(Event::Sample),
        "speed" => serde_json::from_str(data).ok().map(Event::Speed),
        "incident" => serde_json::from_str(data).ok().map(Event::Incident),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_split_events() {
        let sample = r#"{"at":"2026-10-24T04:40:40Z","hop":"dns","status":"pass","rtt_ms":4.2,"jitter_ms":null,"loss_pct":0.0}"#;
        let stream =
            format!(": keepalive\n\nevent: sample\ndata: {sample}\n\nevent: bogus\ndata: 1\n\n");
        let (a, b) = stream.split_at(20);
        let mut events = Vec::new();
        let mut parser = SseParser::default();
        parser.push(a.as_bytes(), &mut |e| events.push(e));
        parser.push(b.as_bytes(), &mut |e| events.push(e));
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], Event::Sample(s) if s.hop == "dns"));
    }
}
