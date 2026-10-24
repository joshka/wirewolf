//! Throughput tests. Cloudflare's speed endpoints by default; Ookla's `speedtest` CLI if asked
//! for and installed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use jiff::Timestamp;
use serde::Deserialize;
use wirewolf_core::Hub;
use wirewolf_core::model::{BloatGrade, SpeedRun, SpeedState};

use crate::assess::median;
use crate::measure::error_chain;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Cloudflare,
    Ookla,
}

impl std::str::FromStr for Backend {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "cloudflare" => Ok(Self::Cloudflare),
            "ookla" => Ok(Self::Ookla),
            _ => Err(format!("unknown speed backend {s:?} (expected cloudflare or ookla)")),
        }
    }
}

impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Self::Cloudflare => "cloudflare",
            Self::Ookla => "ookla",
        }
    }
}

/// Tunables for the Cloudflare test.
#[derive(Debug, Clone, Copy)]
pub struct CloudflareConfig {
    /// Parallel connections per direction. A single stream rarely fills a fiber link.
    pub streams: usize,
    /// How long each direction runs.
    pub duration: Duration,
}

impl Default for CloudflareConfig {
    fn default() -> Self {
        Self { streams: 8, duration: Duration::from_secs(10) }
    }
}

/// Runs one test and publishes progress and the result into the hub.
pub async fn run(hub: Hub, backend: Backend, cf: CloudflareConfig) {
    let started = Timestamp::now();
    let run = SpeedRun {
        id: format!("speed-{}", started.as_millisecond()),
        started,
        state: SpeedState::Running,
        backend: backend.name().into(),
        server: None,
        down_mbps: None,
        up_mbps: None,
        idle_latency_ms: None,
        loaded_latency_ms: None,
        bloat: None,
        bytes_used: Some(0),
        progress: Some(0.0),
        error: None,
    };
    hub.upsert_speed(run.clone());
    let result = match backend {
        Backend::Cloudflare => cloudflare(&hub, run.clone(), cf).await,
        Backend::Ookla => ookla(&hub, run.clone()).await,
    };
    let finished = match result {
        Ok(done) => done,
        Err(failure) => {
            let (partial, error) = *failure;
            tracing::warn!("speed test failed: {error}");
            SpeedRun { state: SpeedState::Failed, progress: None, bloat: None, error: Some(error), ..partial }
        }
    };
    hub.upsert_speed(finished);
}

/// The run so far, and why it failed.
type Failure = Box<(SpeedRun, String)>;

const CF: &str = "https://speed.cloudflare.com";
const CHUNK_DOWN: u64 = 25_000_000;
const CHUNK_UP: usize = 10_000_000;

async fn cloudflare(hub: &Hub, mut run: SpeedRun, cfg: CloudflareConfig) -> Result<SpeedRun, Failure> {
    let client = reqwest::Client::builder()
        .timeout(cfg.duration + Duration::from_secs(20))
        .user_agent(concat!("wirewolf/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| Box::new((run.clone(), e.to_string())))?;
    // A separate client so latency probes don't queue behind the loaded connections.
    let probe = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().map_err(|e| Box::new((run.clone(), e.to_string())))?;

    // Server location.
    if let Ok(r) = probe.get(format!("{CF}/cdn-cgi/trace")).send().await
        && let Ok(body) = r.text().await
    {
        run.server = body.lines().find_map(|l| l.strip_prefix("colo=")).map(|c| format!("Cloudflare {c}"));
    }

    // Idle latency: time to response headers of empty downloads.
    let mut idle = Vec::new();
    for _ in 0..10 {
        if let Some(ms) = latency(&probe).await {
            idle.push(ms);
        }
    }
    let Some(idle_ms) = median(&idle) else {
        return Err(Box::new((run, "could not reach speed.cloudflare.com".into())));
    };
    run.idle_latency_ms = Some(round1(idle_ms));
    run.progress = Some(0.05);
    hub.upsert_speed(run.clone());

    let loaded = Arc::new(std::sync::Mutex::new(Vec::new()));
    let total_bytes = Arc::new(AtomicU64::new(0));

    // Download, then upload. Each phase is half the progress bar after the latency step.
    for (phase, upload) in [(0, false), (1, true)] {
        let counter = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let start = Instant::now();
        let mut tasks = Vec::new();
        for _ in 0..cfg.streams {
            let (client, counter, stop) = (client.clone(), counter.clone(), stop.clone());
            tasks.push(tokio::spawn(async move {
                while !stop.load(Ordering::Relaxed) {
                    let ok = if upload { upload_once(&client, &counter, &stop).await } else { download_once(&client, &counter, &stop).await };
                    if let Err(e) = ok {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        return Err(e);
                    }
                }
                Ok(())
            }));
        }
        // Latency under load, plus progress, until the phase ends.
        let latency_task = {
            let (probe, loaded, stop) = (probe.clone(), loaded.clone(), stop.clone());
            tokio::spawn(async move {
                // Let the connections ramp before sampling.
                tokio::time::sleep(Duration::from_secs(1)).await;
                while !stop.load(Ordering::Relaxed) {
                    if let Some(ms) = latency(&probe).await {
                        loaded.lock().unwrap().push(ms);
                    }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            })
        };
        // Skip the slow-start ramp when computing throughput.
        let warmup = Duration::from_secs(2).min(cfg.duration / 4);
        let mut warm_bytes = None;
        let mut ticker = tokio::time::interval(Duration::from_millis(500));
        loop {
            ticker.tick().await;
            let elapsed = start.elapsed();
            let bytes = counter.load(Ordering::Relaxed);
            if warm_bytes.is_none() && elapsed >= warmup {
                warm_bytes = Some((bytes, elapsed));
            }
            let mbps = match warm_bytes {
                Some((b0, t0)) if elapsed > t0 => (bytes - b0) as f64 * 8.0 / (elapsed - t0).as_secs_f64() / 1e6,
                _ => bytes as f64 * 8.0 / elapsed.as_secs_f64().max(0.001) / 1e6,
            };
            if upload {
                run.up_mbps = Some(round1(mbps));
            } else {
                run.down_mbps = Some(round1(mbps));
            }
            run.bytes_used = Some(total_bytes.load(Ordering::Relaxed) + bytes);
            run.loaded_latency_ms = median(&loaded.lock().unwrap()).map(round1);
            let frac = (elapsed.as_secs_f64() / cfg.duration.as_secs_f64()).min(1.0);
            run.progress = Some(round2(0.05 + 0.475 * (phase as f64 + frac)));
            if elapsed >= cfg.duration || tasks.iter().all(|t| t.is_finished()) {
                break;
            }
            hub.upsert_speed(run.clone());
        }
        stop.store(true, Ordering::Relaxed);
        let mut errors = Vec::new();
        for t in tasks {
            t.abort();
            if let Ok(Err(e)) = t.await {
                errors.push(e);
            }
        }
        latency_task.abort();
        total_bytes.fetch_add(counter.load(Ordering::Relaxed), Ordering::Relaxed);
        run.bytes_used = Some(total_bytes.load(Ordering::Relaxed));
        if counter.load(Ordering::Relaxed) == 0 {
            let what = if upload { "upload" } else { "download" };
            let why = errors.first().cloned().unwrap_or_else(|| "no data transferred".into());
            return Err(Box::new((run, format!("{what} failed: {why}"))));
        }
    }

    let loaded_ms = median(&loaded.lock().unwrap());
    run.loaded_latency_ms = loaded_ms.map(round1);
    run.bloat = loaded_ms.map(|l| BloatGrade::from_increase_ms((l - idle_ms).max(0.0)));
    run.state = SpeedState::Complete;
    run.progress = Some(1.0);
    Ok(run)
}

async fn latency(client: &reqwest::Client) -> Option<f64> {
    let start = Instant::now();
    let r = client.get(format!("{CF}/__down?bytes=0")).send().await.ok()?;
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    // Subtract Cloudflare's own processing time (`cfSpeedEdge;dur=3, cfSpeedWorker;dur=19`).
    let server: f64 = r
        .headers()
        .get_all("server-timing")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .filter(|entry| entry.trim().starts_with("cfSpeed") || entry.trim().starts_with("cfRequestDuration"))
        .filter_map(|entry| entry.split(';').find_map(|p| p.trim().strip_prefix("dur=")).and_then(|d| d.parse::<f64>().ok()))
        .sum();
    r.bytes().await.ok()?;
    Some((ms - server).max(0.1))
}

async fn download_once(client: &reqwest::Client, counter: &AtomicU64, stop: &AtomicBool) -> Result<(), String> {
    let r = client.get(format!("{CF}/__down?bytes={CHUNK_DOWN}")).send().await.map_err(|e| error_chain(&e))?;
    if !r.status().is_success() {
        return Err(format!("HTTP {}", r.status()));
    }
    let mut body = r.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|e| error_chain(&e))?;
        counter.fetch_add(chunk.len() as u64, Ordering::Relaxed);
        if stop.load(Ordering::Relaxed) {
            break;
        }
    }
    Ok(())
}

async fn upload_once(client: &reqwest::Client, counter: &Arc<AtomicU64>, stop: &Arc<AtomicBool>) -> Result<(), String> {
    const PIECE: usize = 64 * 1024;
    static ZEROS: [u8; PIECE] = [0; PIECE];
    let (counter, stop) = (counter.clone(), stop.clone());
    let pieces = futures_util::stream::iter(0..CHUNK_UP / PIECE).map(move |_| {
        if stop.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("stopped"));
        }
        counter.fetch_add(PIECE as u64, Ordering::Relaxed);
        Ok::<_, std::io::Error>(bytes::Bytes::from_static(&ZEROS))
    });
    let r = client
        .post(format!("{CF}/__up"))
        .header("content-type", "application/octet-stream")
        .header("content-length", (CHUNK_UP / PIECE * PIECE).to_string())
        .body(reqwest::Body::wrap_stream(pieces))
        .send()
        .await
        .map_err(|e| error_chain(&e))?;
    if !r.status().is_success() {
        return Err(format!("HTTP {}", r.status()));
    }
    Ok(())
}

// ---- Ookla ----

#[derive(Debug, Deserialize)]
struct OoklaResult {
    ping: OoklaPing,
    download: OoklaTransfer,
    upload: OoklaTransfer,
    server: Option<OoklaServer>,
}

#[derive(Debug, Deserialize)]
struct OoklaPing {
    latency: f64,
}

#[derive(Debug, Deserialize)]
struct OoklaTransfer {
    /// Bytes per second.
    bandwidth: f64,
    bytes: u64,
    latency: Option<OoklaLatency>,
}

#[derive(Debug, Deserialize)]
struct OoklaLatency {
    iqm: f64,
}

#[derive(Debug, Deserialize)]
struct OoklaServer {
    name: Option<String>,
    location: Option<String>,
}

async fn ookla(hub: &Hub, mut run: SpeedRun) -> Result<SpeedRun, Failure> {
    let child = tokio::process::Command::new("speedtest")
        .args(["--format=json", "--accept-license", "--accept-gdpr"])
        .kill_on_drop(true)
        .output();
    run.progress = Some(0.1);
    hub.upsert_speed(run.clone());
    let output = match tokio::time::timeout(Duration::from_secs(120), child).await {
        Err(_) => return Err(Box::new((run, "speedtest timed out after 120s".into()))),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Box::new((run, "the Ookla `speedtest` CLI is not installed (https://www.speedtest.net/apps/cli)".into())));
        }
        Ok(Err(e)) => return Err(Box::new((run, format!("running speedtest: {e}")))),
        Ok(Ok(o)) => o,
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Box::new((run, format!("speedtest exited with {}: {}", output.status, crate::assess::sanitize(stderr.trim())))));
    }
    parse_ookla(&output.stdout, run.clone()).map_err(|e| Box::new((run, e)))
}

fn parse_ookla(json: &[u8], run: SpeedRun) -> Result<SpeedRun, String> {
    let r: OoklaResult = serde_json::from_slice(json).map_err(|e| format!("unexpected speedtest output: {e}"))?;
    let loaded = [&r.download, &r.upload].iter().filter_map(|t| t.latency.as_ref().map(|l| l.iqm)).fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x))));
    Ok(SpeedRun {
        state: SpeedState::Complete,
        server: r.server.map(|s| match (s.name, s.location) {
            (Some(n), Some(l)) => format!("{n} ({l})"),
            (n, l) => n.or(l).unwrap_or_else(|| "Ookla".into()),
        }),
        down_mbps: Some(round1(r.download.bandwidth * 8.0 / 1e6)),
        up_mbps: Some(round1(r.upload.bandwidth * 8.0 / 1e6)),
        idle_latency_ms: Some(round1(r.ping.latency)),
        loaded_latency_ms: loaded.map(round1),
        bloat: loaded.map(|l| BloatGrade::from_increase_ms((l - r.ping.latency).max(0.0))),
        bytes_used: Some(r.download.bytes + r.upload.bytes),
        progress: Some(1.0),
        error: None,
        ..run
    })
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ookla_json() {
        let json = br#"{"type":"result","timestamp":"2026-10-23T10:00:00Z",
            "ping":{"jitter":0.3,"latency":4.2,"low":3.9,"high":4.6},
            "download":{"bandwidth":115000000,"bytes":1200000000,"elapsed":10000,"latency":{"iqm":11.5,"low":4,"high":40,"jitter":2}},
            "upload":{"bandwidth":110000000,"bytes":1100000000,"elapsed":10000,"latency":{"iqm":9.0,"low":4,"high":30,"jitter":2}},
            "packetLoss":0,"isp":"Example Fiber",
            "server":{"id":1,"host":"x","port":8080,"name":"Example ISP","location":"Seattle, WA","country":"United States","ip":"1.2.3.4"},
            "result":{"id":"abc","url":"https://www.speedtest.net/result/c/abc","persisted":true}}"#;
        let run = SpeedRun {
            id: "s".into(),
            started: Timestamp::UNIX_EPOCH,
            state: SpeedState::Running,
            backend: "ookla".into(),
            server: None,
            down_mbps: None,
            up_mbps: None,
            idle_latency_ms: None,
            loaded_latency_ms: None,
            bloat: None,
            bytes_used: None,
            progress: None,
            error: None,
        };
        let r = parse_ookla(json, run).unwrap();
        assert_eq!(r.down_mbps, Some(920.0));
        assert_eq!(r.up_mbps, Some(880.0));
        assert_eq!(r.loaded_latency_ms, Some(11.5));
        assert_eq!(r.bloat, Some(BloatGrade::A));
        assert_eq!(r.server.as_deref(), Some("Example ISP (Seattle, WA)"));
        assert_eq!(r.state, SpeedState::Complete);
    }
}
