//! Network probes, speed tests, and history storage for wirewolf.
//!
//! [`run`] is the live driver: it probes the path on an interval, publishes snapshots and samples
//! into a [`Hub`] with the same shapes the demo driver uses, runs scheduled and on-demand speed
//! tests, and keeps history in SQLite.
//!
//! This crate is part of [wirewolf](https://github.com/joshka/wirewolf), a self-hosted network
//! diagnostics service. Most people want the `wirewolf` crate, which installs the binary
//! (`cargo install wirewolf`). This crate's API is internal and may change in any release.

use std::sync::Arc;
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use wirewolf_core::config::Config;
use wirewolf_core::diagnose::{diagnose, with_targets};
use wirewolf_core::model::{API_VERSION, ActivityKind, Meta, Sample, Severity, Snapshot, Status};
use wirewolf_core::{Command, Hub};

pub mod assess;
pub mod icmp;
pub mod measure;
pub mod ports;
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
    hub.load_activity(history.activity);
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

/// [`validate`] per field, for the config editor: each bad target is reported under its own key.
pub fn field_errors(config: &Config) -> Vec<wirewolf_core::editor::FieldError> {
    use wirewolf_core::editor::FieldError;
    let mut errors: Vec<FieldError> = config
        .targets
        .iter()
        .enumerate()
        .filter_map(|(i, t)| targets::parse(&t.check, t.verify_tls).err().map(|e| FieldError::new(format!("targets.{i}.check"), e)))
        .collect();
    // The resolver filter doesn't see IP literals, so check those URLs here.
    let urls = std::iter::once(("web.captive_portal".to_owned(), &config.web.captive_portal)).chain(config.web.sites.iter().enumerate().map(|(i, s)| (format!("web.sites.{i}"), s)));
    for (key, url) in urls {
        let host = reqwest::Url::parse(url).ok().and_then(|u| u.host_str().map(|h| h.trim_start_matches('[').trim_end_matches(']').to_owned()));
        if host.and_then(|h| h.parse().ok()).is_some_and(targets::link_local) {
            errors.push(FieldError::new(key, "cloud metadata addresses (169.254.0.0/16) aren't probed"));
        }
    }
    if let Err(e) = config.speed.backend.parse::<Backend>() {
        errors.push(FieldError::new("speed.backend", e));
    }
    errors
}

fn probe_ticker(config: &Config) -> tokio::time::Interval {
    let mut ticker = tokio::time::interval(config.probe.interval.0.max(Duration::from_secs(5)));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker
}

fn speed_ticker_for(hub: &Hub, config: &Config) -> Option<tokio::time::Interval> {
    let every = Some(config.speed.interval.0).filter(|d| !d.is_zero())?;
    // First scheduled run after one interval, unless the history says one is overdue.
    let last = hub.speed_runs().first().map(|r| r.started);
    let since = last.map(|t| Timestamp::now().duration_since(t).unsigned_abs()).unwrap_or(Duration::MAX);
    let first = every.saturating_sub(since).max(Duration::from_secs(60));
    Some(tokio::time::interval_at(tokio::time::Instant::now() + first, every))
}

/// Probes forever, publishing into the hub.
pub async fn run(hub: Hub, mut config: Arc<Config>) {
    let mut commands = hub.take_commands().expect("the live driver needs the command receiver");
    let mut prober = match measure::Prober::new(config.clone()) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("can't start probes: {e}");
            return;
        }
    };
    let capability = match prober.icmp() {
        Some(i) if i.can_trace() => format!("ICMP available ({i:?}); gateway and ISP probes use ICMP"),
        Some(i) => format!("ICMP available ({i:?}) without TTL replies; the ISP hop is inferred"),
        None => "ICMP unavailable; the gateway uses TCP connects and the ISP hop is inferred".into(),
    };
    tracing::info!("{capability}");
    log_settings(&config);
    // Only when it differs from the last run, so restarts don't fill the timeline.
    if !hub.activity().iter().find(|a| a.kind == ActivityKind::Probe).is_some_and(|a| a.message == capability) {
        hub.record(ActivityKind::Probe, capability, None, None);
    }
    let mut ticker = probe_ticker(&config);
    let mut speed_ticker = speed_ticker_for(&hub, &config);
    let mut upstream_fault = false;
    // Logged when they change, not every cycle: a fault that lasts an hour is two lines.
    let mut last_verdict: Option<(Severity, String)> = None;
    let mut last_status: std::collections::HashMap<String, Status> = std::collections::HashMap::new();
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                // A running speed test saturates the link; probing now would blame the network
                // for our own load.
                if hub.speed_running() {
                    tracing::debug!("skipping probe cycle during speed test");
                    continue;
                }
                let started = std::time::Instant::now();
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
                if tracing::enabled!(tracing::Level::DEBUG) {
                    let ms = |v: Option<f64>| v.map(|v| format!(" {v:.1}ms")).unwrap_or_default();
                    let path: Vec<String> = hops.iter().map(|h| format!("{} {:?}{}", h.id, h.status, ms(h.metrics.rtt_ms)).to_lowercase()).chain(targets.iter().map(|t| format!("{} {:?}{}", t.id, t.status, ms(t.rtt_ms)).to_lowercase())).collect();
                    tracing::debug!(took_ms = started.elapsed().as_millis() as u64, sweep_next = upstream_fault, "cycle: {}", path.join(", "));
                }
                for (id, status, detail) in hops.iter().map(|h| (&h.id, h.status, &h.summary)).chain(targets.iter().map(|t| (&t.id, t.status, &t.detail))) {
                    if status != Status::Pass {
                        tracing::trace!(check = %id, ?status, "{detail}");
                    }
                    if last_status.insert(id.clone(), status).is_some_and(|was| was != status) {
                        tracing::debug!(check = %id, "{id} {status:?}: {detail}");
                    }
                }
                let verdict = with_targets(diagnose(&hops), &targets);
                let key = (verdict.severity, verdict.headline.clone());
                if last_verdict.as_ref() != Some(&key) {
                    match verdict.severity {
                        Severity::Healthy if last_verdict.is_none() => tracing::info!("first check: {}", verdict.headline),
                        Severity::Healthy => tracing::info!("recovered: {}", verdict.headline),
                        severity => tracing::warn!(?severity, "{}", verdict.headline),
                    }
                    last_verdict = Some(key);
                }
                hub.publish_snapshot(Snapshot { at, verdict, hops, open_incident: None, targets });
            }
            _ = async { speed_ticker.as_mut().unwrap().tick().await }, if speed_ticker.is_some() => {
                start_speed(&hub, &config);
            }
            Some(command) = commands.recv() => match command {
                Command::RunSpeedTest => start_speed(&hub, &config),
                Command::SetScenario(_) => {}
                Command::ApplyConfig(new) => match measure::Prober::new(new.clone()) {
                    Ok(p) => {
                        tracing::info!("applying saved settings from the next cycle");
                        log_settings(&new);
                        if new.probe.interval != config.probe.interval {
                            ticker = probe_ticker(&new);
                        }
                        if new.speed.interval != config.speed.interval {
                            speed_ticker = speed_ticker_for(&hub, &new);
                        }
                        prober = p;
                        config = new;
                    }
                    Err(e) => tracing::error!("can't apply saved settings: {e}"),
                },
            }
        }
    }
}

/// One line with the settings that shape the probes and how much they log, never secrets.
fn log_settings(config: &Config) {
    tracing::info!(
        "probing every {:#}, {} targets, {} sites; speed test {}; history {} for {:#}",
        jiff::SignedDuration::try_from(config.probe.interval.0).unwrap_or_default(),
        config.targets.len(),
        config.web.sites.len(),
        if config.speed.interval.0.is_zero() { "off".to_owned() } else { format!("{} every {:#}", config.speed.backend, jiff::SignedDuration::try_from(config.speed.interval.0).unwrap_or_default()) },
        if config.storage.db.as_os_str().is_empty() { "in memory".to_owned() } else { config.storage.db.display().to_string() },
        jiff::SignedDuration::try_from(config.storage.retention.0).unwrap_or_default(),
    );
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
