//! The wire model shared by the engine, the API, and every client.
//!
//! These types are serialized as-is by the JSON API (`docs/api.md`). Field names are
//! `snake_case`, enums are lowercase strings, and timestamps are RFC 3339 strings. Treat any
//! change here as an API change.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

/// The version of the API contract. Bump the minor for additive changes, the major for breaking.
pub const API_VERSION: &str = "1.2";

/// The outcome of a single check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Working as expected.
    Pass,
    /// Reachable but degraded (high latency, loss, slow DNS). Never counts as a failure.
    Warn,
    /// Broken.
    Fail,
    /// Not run because a prerequisite failed.
    Skip,
    /// Does not apply to this environment (for example, no IPv6, no proxy).
    NotApplicable,
    /// Not measured yet.
    Pending,
}

/// Where a hop sits on the path from wirewolf to the internet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HopKind {
    /// The machine wirewolf runs on: interface, address, MTU, CGNAT.
    Host,
    /// The default gateway (home router).
    Gateway,
    /// The ISP edge, beyond the gateway. Expanded into path hops when a fault is upstream.
    Isp,
    /// Internet reachability: TCP handshakes to anycast targets.
    Internet,
    /// Name resolution.
    Dns,
    /// HTTP/TLS reachability and captive-portal detection.
    Web,
}

/// One step in the path chain, with the evidence behind its status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hop {
    /// Stable identifier, used to query history (`host`, `gateway`, `isp`, `internet`, `dns`,
    /// `web`).
    pub id: String,
    pub kind: HopKind,
    /// Short display name, for example "Gateway" or "DNS".
    pub label: String,
    /// The address or name of the thing probed, for example `192.168.1.1`.
    pub target: Option<String>,
    pub status: Status,
    /// One-line human summary, for example "2.1 ms, no loss".
    pub summary: String,
    pub metrics: HopMetrics,
    /// Raw evidence for the detail drawer.
    pub evidence: Vec<Evidence>,
    /// Individual sub-checks (for example each anycast target, or each resolver).
    pub checks: Vec<Check>,
}

/// Latency and loss for a hop. `None` means not measured.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct HopMetrics {
    pub rtt_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    /// Packet or handshake loss, 0.0 to 100.0.
    pub loss_pct: Option<f64>,
}

/// A key/value fact shown in the detail drawer, for example `mtu = 1500`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub key: String,
    pub value: String,
}

/// A sub-check within a hop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub status: Status,
    pub detail: String,
    pub rtt_ms: Option<f64>,
}

/// The overall health classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Healthy,
    Degraded,
    Down,
    /// Nothing could be measured (for example, the probes themselves failed to run).
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

/// The plain-English diagnosis of a snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verdict {
    pub severity: Severity,
    /// One line, for example "DNS is failing — the internet is reachable by IP".
    pub headline: String,
    /// A sentence or two of explanation.
    pub detail: String,
    /// The hop blamed for the problem, if any. Downstream failures are collateral.
    pub blame: Option<String>,
    pub confidence: Confidence,
    /// What to try next.
    pub fix_hint: Option<String>,
    /// How the verdict was reached, for example "12 probes over 4s to 3 targets".
    pub method: String,
}

/// The complete current state of the path at one moment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub at: Timestamp,
    pub verdict: Verdict,
    /// In path order: host, gateway, isp, internet, dns, web.
    pub hops: Vec<Hop>,
    /// The incident open right now, if any.
    pub open_incident: Option<String>,
    /// User-configured targets (`[[targets]]` in the config), in config order. Added in 1.1;
    /// empty when none are configured.
    #[serde(default)]
    pub targets: Vec<Target>,
}

/// The latest result for one user-configured target, for example a NAS or a Pi-hole.
///
/// Its history is in `/history?hop=<id>`. Ids start with `target:` so they never collide with
/// hop ids.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Target {
    /// Stable identifier, for example `target:nas`.
    pub id: String,
    /// Display name from the config, for example "NAS".
    pub name: String,
    /// The configured check, for example `tcp://nas.local:445` (credentials removed).
    pub check: String,
    /// `tcp`, `http`, `https`, `ping`, or `dns`.
    pub kind: String,
    /// A failing critical target makes the verdict degraded and can open an incident.
    pub critical: bool,
    pub status: Status,
    /// One line, for example "TCP :445 in 1.8 ms".
    pub detail: String,
    pub rtt_ms: Option<f64>,
    pub loss_pct: Option<f64>,
}

/// One time-series point for a hop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub at: Timestamp,
    pub hop: String,
    pub status: Status,
    pub rtt_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub loss_pct: Option<f64>,
}

/// A bufferbloat grade, from the latency increase under load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BloatGrade {
    #[serde(rename = "A+")]
    APlus,
    A,
    B,
    C,
    D,
    F,
}

impl BloatGrade {
    /// Grades by the increase of loaded over idle latency, using the common Waveform thresholds.
    pub fn from_increase_ms(increase: f64) -> Self {
        match increase {
            x if x < 5.0 => Self::APlus,
            x if x < 30.0 => Self::A,
            x if x < 60.0 => Self::B,
            x if x < 200.0 => Self::C,
            x if x < 400.0 => Self::D,
            _ => Self::F,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeedState {
    Running,
    Complete,
    Failed,
}

/// One throughput test run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeedRun {
    pub id: String,
    pub started: Timestamp,
    pub state: SpeedState,
    /// `cloudflare`, `ookla`, or `demo`.
    pub backend: String,
    /// Where the test ran against, for example "Cloudflare SYD".
    pub server: Option<String>,
    pub down_mbps: Option<f64>,
    pub up_mbps: Option<f64>,
    pub idle_latency_ms: Option<f64>,
    pub loaded_latency_ms: Option<f64>,
    pub bloat: Option<BloatGrade>,
    pub bytes_used: Option<u64>,
    /// 0.0 to 1.0 while running.
    pub progress: Option<f64>,
    pub error: Option<String>,
}

/// A period during which the verdict was not healthy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Incident {
    pub id: String,
    pub started: Timestamp,
    /// `None` while ongoing.
    pub ended: Option<Timestamp>,
    /// The worst severity seen during the incident.
    pub severity: Severity,
    pub headline: String,
    pub blame: Option<String>,
    /// The snapshot at the worst point, for the copyable report.
    pub evidence: Snapshot,
}

/// Static information about this wirewolf instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub version: String,
    pub api_version: String,
    /// `demo` or `live`.
    pub mode: String,
    /// Hostname of the machine wirewolf runs on.
    pub host: String,
    /// Available demo scenarios (empty in live mode).
    pub scenarios: Vec<Scenario>,
    /// The active demo scenario, if in demo mode.
    pub scenario: Option<String>,
}

/// The effective configuration, from `GET /api/v1/config`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigView {
    /// The config file that was read, if any.
    pub path: Option<String>,
    /// Every setting, with defaults filled in and credentials removed. Same shape as the TOML
    /// file (`wirewolf config default`).
    pub config: crate::config::Config,
    /// Where each non-default setting came from, keyed by dotted path (for example
    /// `probe.interval`): `file`, `env`, or `flag`. Anything absent is a default.
    pub sources: std::collections::BTreeMap<String, crate::config::Source>,
    /// Whether `PUT /config` accepts edits (1.2). False with `--read-only-settings`. The auth
    /// layer may still refuse a write; see `GET /session`.
    #[serde(default)]
    pub editable: bool,
    /// The file edits are written to, created on first save (1.2).
    #[serde(default)]
    pub save_path: Option<String>,
    /// Settings that only change on restart, so the API won't edit them (1.2).
    #[serde(default)]
    pub restart_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scenario {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// A live update, delivered over SSE with the variant name as the event name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Event {
    Snapshot(Snapshot),
    Sample(Sample),
    Speed(SpeedRun),
    Incident(Incident),
}

impl Event {
    /// The SSE event name.
    pub fn name(&self) -> &'static str {
        match self {
            Event::Snapshot(_) => "snapshot",
            Event::Sample(_) => "sample",
            Event::Speed(_) => "speed",
            Event::Incident(_) => "incident",
        }
    }
}
