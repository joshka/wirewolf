//! Network probes, speed tests, and history storage for wirewolf.
//!
//! [`run`] is the live driver: it probes the path on an interval, publishes snapshots and samples
//! into a [`Hub`] with the same shapes the demo driver uses, runs scheduled and on-demand speed
//! tests, and keeps history in SQLite.

use std::sync::Arc;
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use wirewolf_core::config::Config;
use wirewolf_core::diagnose::{diagnose, with_targets};
use wirewolf_core::model::{API_VERSION, Meta, Sample, Severity, Snapshot, Status};
use wirewolf_core::{Command, Hub};

pub mod assess;
pub mod icmp;
pub mod measure;
pub mod speed;
pub mod store;
pub mod targets;

pub use speed::Backend;

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
///
/// An empty `storage.db` keeps history in memory only.
pub fn attach_store(hub: &Hub, config: &Config) -> anyhow::Result<()> {
    let db = &config.storage.db;
    let store = if db.as_os_str().is_empty() { store::Store::in_memory()? } else { store::Store::open(db)? };
    let history = store.load(Timestamp::now() - LOAD_WINDOW, 500)?;
    tracing::info!(
        "loaded {} samples, {} speed runs, {} incidents from {}",
        history.samples.len(),
        history.speed.len(),
        history.incidents.len(),
        if db.as_os_str().is_empty() { "memory".into() } else { db.display().to_string() }
    );
    hub.load_history(history.samples, history.speed, history.incidents);
    let retention = SignedDuration::try_from(config.storage.retention.0)?;
    tokio::spawn(store::follow(hub.clone(), store, retention));
    Ok(())
}

/// Checks the parts of a config that serde can't, such as target URLs and the speed backend.
pub fn validate(config: &Config) -> anyhow::Result<()> {
    targets::parse_all(&config.targets).map_err(anyhow::Error::msg)?;
    config.speed.backend.parse::<Backend>().map_err(anyhow::Error::msg)?;
    Ok(())
}

/// Probes forever, publishing into the hub.
pub async fn run(hub: Hub, config: Arc<Config>) {
    let mut commands = hub.take_commands().expect("the live driver needs the command receiver");
    let mut prober = match measure::Prober::new(config.clone()) {
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
    let mut ticker = tokio::time::interval(config.probe.interval.0.max(Duration::from_secs(5)));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let speed_interval = Some(config.speed.interval.0).filter(|d| !d.is_zero());
    let mut speed_ticker = speed_interval.map(|every| {
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
                let (measurements, targets) = prober.cycle(upstream_fault).await;
                let at = Timestamp::now();
                let hops = assess::hops(&measurements);
                // Sweep the path next cycle when the break is past the gateway.
                upstream_fault = hops.iter().find(|h| h.id == "gateway").is_some_and(|h| h.status == Status::Pass)
                    && hops.iter().any(|h| matches!(h.id.as_str(), "isp" | "internet") && matches!(h.status, Status::Fail | Status::Warn));
                for hop in &hops {
                    hub.push_sample(Sample { at, hop: hop.id.clone(), status: hop.status, rtt_ms: hop.metrics.rtt_ms, jitter_ms: hop.metrics.jitter_ms, loss_pct: hop.metrics.loss_pct });
                }
                for t in &targets {
                    hub.push_sample(Sample { at, hop: t.id.clone(), status: t.status, rtt_ms: t.rtt_ms, jitter_ms: None, loss_pct: t.loss_pct });
                }
                let verdict = with_targets(diagnose(&hops), &targets);
                if verdict.severity != Severity::Healthy {
                    tracing::info!("{:?}: {}", verdict.severity, verdict.headline);
                }
                hub.publish_snapshot(Snapshot { at, verdict, hops, open_incident: None, targets });
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
    let backend = config.speed.backend.parse().unwrap_or(Backend::Cloudflare);
    let cf = speed::CloudflareConfig { streams: config.speed.streams.max(1), duration: config.speed.duration.0.max(Duration::from_secs(2)) };
    let server = Some(config.speed.server.clone()).filter(|s| !s.is_empty());
    tracing::info!("starting {} speed test", backend.name());
    tokio::spawn(speed::run(hub.clone(), backend, cf, server));
}
