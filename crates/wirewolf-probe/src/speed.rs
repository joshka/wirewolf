//! Throughput tests against Cloudflare's speed endpoints (default) or Ookla's speedtest.net server
//! network. Both are native: one transfer engine with different endpoints, and no external CLI.

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
///
/// `server` pins the Ookla server (`host:port`) instead of picking the fastest nearby.
pub async fn run(hub: Hub, backend: Backend, cf: CloudflareConfig, server: Option<String>) {
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
    // Probing pauses while a test runs, so a test must always end: cap the whole run, and turn
    // a panic in the test into a failure instead of a run that stays "running" forever.
    let limit = cf.duration * 2 + Duration::from_secs(90);
    let test = {
        let (hub, run) = (hub.clone(), run.clone());
        tokio::spawn(async move {
            match backend {
                Backend::Cloudflare => http_test(&hub, run, cf, Target::cloudflare).await,
                Backend::Ookla => match server {
                    Some(host) => http_test(&hub, run, cf, |_| async move { Ok(Target::ookla_server(host)) }).await,
                    None => http_test(&hub, run, cf, Target::ookla).await,
                },
            }
        })
    };
    let abort = test.abort_handle();
    let result = match tokio::time::timeout(limit, test).await {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => Err(Box::new((hub.speed_runs().into_iter().find(|r| r.id == run.id).unwrap_or(run.clone()), format!("speed test crashed: {e}")))),
        Err(_) => {
            abort.abort();
            Err(Box::new((hub.speed_runs().into_iter().find(|r| r.id == run.id).unwrap_or(run.clone()), format!("timed out after {}s", limit.as_secs()))))
        }
    };
    let finished = match result {
        Ok(done) => {
            let mbps = |v: Option<f64>| v.map_or("?".into(), |v| format!("{v:.0}"));
            tracing::info!(
                server = done.server.as_deref().unwrap_or(""),
                "speed test: {} down, {} up Mbps; latency {} ms idle, {} ms loaded",
                mbps(done.down_mbps),
                mbps(done.up_mbps),
                mbps(done.idle_latency_ms),
                mbps(done.loaded_latency_ms)
            );
            done
        }
        Err(failure) => {
            let (partial, error) = *failure;
            tracing::warn!(backend = backend.name(), "speed test failed: {error}");
            SpeedRun { state: SpeedState::Failed, progress: None, bloat: None, error: Some(error), ..partial }
        }
    };
    if finished.state == SpeedState::Complete {
        tracing::info!(
            backend = backend.name(),
            down_mbps = finished.down_mbps,
            up_mbps = finished.up_mbps,
            "speed test finished"
        );
    }
    hub.upsert_speed(finished);
}

/// The run so far, and why it failed.
type Failure = Box<(SpeedRun, String)>;

const CF: &str = "https://speed.cloudflare.com";
const CHUNK_DOWN: u64 = 25_000_000;
const CHUNK_UP: usize = 10_000_000;

/// Where to run the test.
#[derive(Debug, Clone)]
struct Target {
    label: String,
    /// Download URL for `CHUNK_DOWN` bytes.
    download: String,
    upload: String,
    ping: PingVia,
}

#[derive(Debug, Clone)]
enum PingVia {
    /// Time to response headers of an empty Cloudflare download, minus server time.
    CloudflareHttp,
    /// Ookla's text protocol on the server port: `PING <ms>` → `PONG <ms>` on a held connection.
    OoklaTcp(String),
}

impl Target {
    async fn cloudflare(probe: reqwest::Client) -> Result<Self, String> {
        let colo = match probe.get(format!("{CF}/cdn-cgi/trace")).send().await {
            Ok(r) => r.text().await.ok().and_then(|b| b.lines().find_map(|l| l.strip_prefix("colo=")).map(str::to_string)),
            Err(e) => return Err(format!("could not reach speed.cloudflare.com: {}", error_chain(&e))),
        };
        Ok(Self {
            label: colo.map_or("Cloudflare".into(), |c| format!("Cloudflare {c}")),
            download: format!("{CF}/__down?bytes={CHUNK_DOWN}"),
            upload: format!("{CF}/__up"),
            ping: PingVia::CloudflareHttp,
        })
    }

    /// A specific speedtest.net server, `host:port`.
    fn ookla_server(host: String) -> Self {
        Self {
            label: crate::assess::sanitize(&host),
            download: format!("https://{host}/download?size={CHUNK_DOWN}"),
            upload: format!("https://{host}/upload"),
            ping: PingVia::OoklaTcp(host),
        }
    }

    /// Picks the lowest-latency server among the nearest few that speedtest.net suggests.
    async fn ookla(probe: reqwest::Client) -> Result<Self, String> {
        let servers: Vec<OoklaServer> = probe
            .get("https://www.speedtest.net/api/js/servers?engine=js&https_functional=true&limit=8")
            .send()
            .await
            .map_err(|e| format!("could not fetch the speedtest.net server list: {}", error_chain(&e)))?
            .json()
            .await
            .map_err(|e| format!("unexpected speedtest.net server list: {}", error_chain(&e)))?;
        let pings = servers.iter().map(|s| async move {
            let mut pinger = Pinger::connect(&PingVia::OoklaTcp(s.host.clone()), None).await?;
            let mut rtts = Vec::new();
            for _ in 0..3 {
                rtts.extend(pinger.ping().await);
            }
            median(&rtts)
        });
        let rtts = futures_util::future::join_all(pings).await;
        let best = servers
            .iter()
            .zip(rtts)
            .filter_map(|(s, r)| r.map(|r| (s, r)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .ok_or("no speedtest.net server answered")?
            .0;
        Ok(Self {
            label: format!("{} — {}", crate::assess::sanitize(&best.sponsor), crate::assess::sanitize(&best.name)),
            download: format!("https://{}/download?size={CHUNK_DOWN}", best.host),
            upload: format!("https://{}/upload", best.host),
            ping: PingVia::OoklaTcp(best.host.clone()),
        })
    }
}

#[derive(Debug, Deserialize)]
struct OoklaServer {
    /// `host:port`.
    host: String,
    name: String,
    sponsor: String,
}

enum Pinger {
    Http(reqwest::Client),
    Tcp(tokio::io::BufStream<tokio::net::TcpStream>),
}

impl Pinger {
    async fn connect(via: &PingVia, client: Option<reqwest::Client>) -> Option<Self> {
        match via {
            PingVia::CloudflareHttp => Some(Self::Http(client?)),
            PingVia::OoklaTcp(host) => {
                use tokio::io::AsyncWriteExt;
                let stream = tokio::time::timeout(Duration::from_secs(3), tokio::net::TcpStream::connect(host.as_str())).await.ok()?.ok()?;
                stream.set_nodelay(true).ok()?;
                let mut stream = tokio::io::BufStream::new(stream);
                stream.write_all(b"HI\n").await.ok()?;
                stream.flush().await.ok()?;
                let line = tokio::time::timeout(Duration::from_secs(3), read_reply(&mut stream)).await.ok()??;
                line.starts_with("HELLO").then_some(Self::Tcp(stream))
            }
        }
    }

    async fn ping(&mut self) -> Option<f64> {
        match self {
            Self::Http(client) => latency(client).await,
            Self::Tcp(stream) => {
                use tokio::io::AsyncWriteExt;
                let start = Instant::now();
                stream.write_all(format!("PING {}\n", Timestamp::now().as_millisecond()).as_bytes()).await.ok()?;
                stream.flush().await.ok()?;
                let line = tokio::time::timeout(Duration::from_secs(5), read_reply(stream)).await.ok()??;
                line.starts_with("PONG").then(|| start.elapsed().as_secs_f64() * 1000.0)
            }
        }
    }
}

/// One line from an Ookla server, at most 256 bytes. Servers are run by third parties, so a
/// reply with no newline mustn't grow without bound until the timeout.
async fn read_reply(stream: &mut tokio::io::BufStream<tokio::net::TcpStream>) -> Option<String> {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt};
    let mut line = String::new();
    stream.take(256).read_line(&mut line).await.ok()?;
    Some(line)
}

async fn http_test<F, Fut>(hub: &Hub, mut run: SpeedRun, cfg: CloudflareConfig, target: F) -> Result<SpeedRun, Failure>
where
    F: FnOnce(reqwest::Client) -> Fut,
    Fut: Future<Output = Result<Target, String>>,
{
    // Ookla servers are run by third parties: don't let one redirect the test to a LAN or
    // metadata address.
    let builder = || reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).dns_resolver(crate::targets::NoLinkLocal);
    let client = builder()
        .timeout(cfg.duration + Duration::from_secs(20))
        .user_agent(concat!("wirewolf/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| Box::new((run.clone(), e.to_string())))?;
    // A separate client so latency probes don't queue behind the loaded connections.
    let probe = builder().timeout(Duration::from_secs(5)).build().map_err(|e| Box::new((run.clone(), e.to_string())))?;

    let target = match target(probe.clone()).await {
        Ok(t) => t,
        Err(e) => return Err(Box::new((run, e))),
    };
    run.server = Some(target.label.clone());

    let mut idle = Vec::new();
    if let Some(mut pinger) = Pinger::connect(&target.ping, Some(probe.clone())).await {
        for _ in 0..10 {
            idle.extend(pinger.ping().await);
        }
    }
    let Some(idle_ms) = median(&idle) else {
        return Err(Box::new((run, format!("could not reach {}", target.label))));
    };
    run.idle_latency_ms = Some(round1(idle_ms));
    tracing::debug!(server = %target.label, "speed test idle latency {idle_ms:.1} ms");
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
            let url = if upload { target.upload.clone() } else { target.download.clone() };
            tasks.push(tokio::spawn(async move {
                while !stop.load(Ordering::Relaxed) {
                    let ok = if upload { upload_once(&client, &url, &counter, &stop).await } else { download_once(&client, &url, &counter, &stop).await };
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
            let (probe, loaded, stop, via) = (probe.clone(), loaded.clone(), stop.clone(), target.ping.clone());
            tokio::spawn(async move {
                let pinger = Pinger::connect(&via, Some(probe)).await;
                // Let the connections ramp before sampling.
                tokio::time::sleep(Duration::from_secs(1)).await;
                let Some(mut pinger) = pinger else { return };
                while !stop.load(Ordering::Relaxed) {
                    if let Some(ms) = pinger.ping().await {
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
        let what = if upload { "upload" } else { "download" };
        tracing::debug!(bytes = counter.load(Ordering::Relaxed), errors = errors.len(), "speed test {what} done in {:.1}s", start.elapsed().as_secs_f64());
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

async fn download_once(client: &reqwest::Client, url: &str, counter: &AtomicU64, stop: &AtomicBool) -> Result<(), String> {
    let r = client.get(url).send().await.map_err(|e| error_chain(&e))?;
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

async fn upload_once(client: &reqwest::Client, url: &str, counter: &Arc<AtomicU64>, stop: &Arc<AtomicBool>) -> Result<(), String> {
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
        .post(url)
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

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}
