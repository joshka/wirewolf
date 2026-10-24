//! Network probes, speed tests, and history storage for wirewolf.
//!
//! [`run`] is the live driver: it probes the path on an interval, publishes snapshots and samples
//! into a [`Hub`] with the same shapes the demo driver uses, runs scheduled and on-demand speed
//! tests, and keeps history in SQLite.

use std::path::PathBuf;
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use wirewolf_core::diagnose::diagnose;
use wirewolf_core::model::{API_VERSION, Meta, Sample, Severity, Snapshot, Status};
use wirewolf_core::{Command, Hub};

pub mod assess;
pub mod icmp;
pub mod measure;
pub mod speed;
pub mod store;

pub use speed::Backend;

/// How the live driver runs.
#[derive(Debug, Clone)]
pub struct Config {
    /// Time between probe cycles.
    pub interval: Duration,
    /// Time between scheduled speed tests. `None` disables them.
    pub speed_interval: Option<Duration>,
    pub speed_backend: Backend,
    pub cloudflare: speed::CloudflareConfig,
    /// SQLite file. `None` keeps history in memory only.
    pub db: Option<PathBuf>,
    pub retention: SignedDuration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            speed_interval: Some(Duration::from_secs(60 * 60)),
            speed_backend: Backend::Cloudflare,
            cloudflare: speed::CloudflareConfig::default(),
            db: None,
            retention: SignedDuration::from_hours(24 * 30),
        }
    }
}

pub fn meta() -> Meta {
    Meta {
        version: env!("CARGO_PKG_VERSION").into(),
        api_version: API_VERSION.into(),
        mode: "live".into(),
        host: hostname(),
        scenarios: vec![],
        scenario: None,
    }
}

fn hostname() -> String {
    std::env::var("WIREWOLF_HOSTNAME")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok().map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let mut buf = [0u8; 256];
            // SAFETY: the buffer is valid for its length; gethostname NUL-terminates on success.
            (unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0)
                .then(|| String::from_utf8_lossy(buf.split(|b| *b == 0).next().unwrap_or_default()).into_owned())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "wirewolf".into())
}

/// Hub history only covers this much; older rows stay in SQLite.
const LOAD_WINDOW: SignedDuration = SignedDuration::from_hours(48);

/// Opens the store, loads history into the hub, and starts writing new history. Call before
/// [`run`] so the API serves history from the first request.
pub fn attach_store(hub: &Hub, config: &Config) -> anyhow::Result<()> {
    let store = match &config.db {
        Some(path) => store::Store::open(path)?,
        None => store::Store::in_memory()?,
    };
    let history = store.load(Timestamp::now() - LOAD_WINDOW, 500)?;
    tracing::info!(
        "loaded {} samples, {} speed runs, {} incidents from {}",
        history.samples.len(),
        history.speed.len(),
        history.incidents.len(),
        config.db.as_ref().map_or("memory".into(), |p| p.display().to_string())
    );
    hub.load_history(history.samples, history.speed, history.incidents);
    tokio::spawn(store::follow(hub.clone(), store, config.retention));
    Ok(())
}

/// Probes forever, publishing into the hub.
pub async fn run(hub: Hub, config: Config) {
    let mut commands = hub.take_commands().expect("the live driver needs the command receiver");
    let mut prober = match measure::Prober::new() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("can't start probes: {e}");
            return;
        }
    };
    match prober.icmp() {
        Some(i) if i.can_trace() => tracing::info!("ICMP available ({i:?}); gateway and ISP probes use ICMP"),
        Some(i) => tracing::info!("ICMP available ({i:?}) without TTL replies; the ISP hop is inferred"),
        None => tracing::info!("ICMP unavailable; the gateway uses TCP connects and the ISP hop is inferred"),
    }
    let mut ticker = tokio::time::interval(config.interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut speed_ticker = config.speed_interval.map(|every| {
        // First scheduled run after one interval, unless the history says one is overdue.
        let last = hub.speed_runs().first().map(|r| r.started);
        let since = last.map(|t| Timestamp::now().duration_since(t).unsigned_abs()).unwrap_or(Duration::MAX);
        let first = every.saturating_sub(since).max(Duration::from_secs(60));
        tokio::time::interval_at(tokio::time::Instant::now() + first, every)
    });
    let mut upstream_fault = false;
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                // A running speed test saturates the link; probing now would blame the network
                // for our own load.
                if hub.speed_running() {
                    tracing::debug!("skipping probe cycle during speed test");
                    continue;
                }
                let measurements = prober.cycle(upstream_fault).await;
                let at = Timestamp::now();
                let hops = assess::hops(&measurements);
                // Sweep the path next cycle when the break is past the gateway.
                upstream_fault = hops.iter().find(|h| h.id == "gateway").is_some_and(|h| h.status == Status::Pass)
                    && hops.iter().any(|h| matches!(h.id.as_str(), "isp" | "internet") && matches!(h.status, Status::Fail | Status::Warn));
                for hop in &hops {
                    hub.push_sample(Sample { at, hop: hop.id.clone(), status: hop.status, rtt_ms: hop.metrics.rtt_ms, jitter_ms: hop.metrics.jitter_ms, loss_pct: hop.metrics.loss_pct });
                }
                let verdict = diagnose(&hops);
                if verdict.severity != Severity::Healthy {
                    tracing::info!("{:?}: {}", verdict.severity, verdict.headline);
                }
                hub.publish_snapshot(Snapshot { at, verdict, hops, open_incident: None });
            }
            _ = async { speed_ticker.as_mut().unwrap().tick().await }, if speed_ticker.is_some() => {
                start_speed(&hub, &config);
            }
            Some(command) = commands.recv() => match command {
                Command::RunSpeedTest => start_speed(&hub, &config),
                Command::SetScenario(_) => {}
            }
        }
    }
}

fn start_speed(hub: &Hub, config: &Config) {
    if hub.speed_running() {
        return;
    }
    tracing::info!("starting {} speed test", config.speed_backend.name());
    tokio::spawn(speed::run(hub.clone(), config.speed_backend, config.cloudflare));
}
