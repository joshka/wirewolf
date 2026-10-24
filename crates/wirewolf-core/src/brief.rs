//! The brief: a compact, redacted summary of the network for an AI assistant (or a human).
//!
//! [`build`] turns a snapshot plus recent incidents, history, and speed runs into a [`Brief`].
//! [`Brief::text`] renders it as a few dozen lines of plain text, which is about 25 times
//! smaller than the raw API JSON for a day of history. [`prompt`] wraps that text in a request
//! to explain the problem, for pasting into any chat assistant.
//!
//! Redaction hides what identifies the user and keeps what explains the network: public IP
//! addresses become `<public-ip-N>`, while private, CGNAT, link-local, documentation, and
//! well-known resolver addresses stay, because they are often the diagnosis.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::net::IpAddr;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::model::{BloatGrade, Confidence, HopMetrics, Incident, Meta, Sample, Severity, Snapshot, SpeedRun, SpeedState, Status};

/// The brief's format version, printed in its first line.
pub const VERSION: u32 = 1;

const INCIDENTS: usize = 5;
const SPEED_RUNS: usize = 3;
/// Evidence that is long and says little (the DNS query list) or identifies hardware.
const SKIP_EVIDENCE: &[&str] = &["queries", "query", "address", "interface", "mac", "mac address"];
/// Public addresses everyone probes; naming them gives nothing away.
const WELL_KNOWN: &[&str] = &[
    "1.1.1.1", "1.0.0.1", "8.8.8.8", "8.8.4.4", "9.9.9.9", "149.112.112.112", "208.67.222.222", "208.67.220.220",
    "2606:4700:4700::1111", "2606:4700:4700::1001", "2001:4860:4860::8888", "2001:4860:4860::8844", "2620:fe::fe", "2620:fe::9",
];

/// What to put in a brief.
#[derive(Debug, Clone)]
pub struct Options {
    /// Replace public IP addresses with stable placeholders.
    pub redact: bool,
    /// How far back the history summary looks.
    pub window: std::time::Duration,
    /// Addresses to keep even when redacting, such as the configured probe targets.
    pub keep: Vec<String>,
}

impl Options {
    /// The defaults, keeping the addresses `config` probes: they're public, but naming them
    /// gives nothing away.
    pub fn for_config(config: &crate::config::Config) -> Self {
        let internet = config.internet.targets.iter().flat_map(|t| [t.v4.map(|a| a.to_string()), t.v6.map(|a| a.to_string())]).flatten();
        let keep = internet.chain(config.dns.resolvers.iter().map(|r| r.address.to_string())).collect();
        Self { keep, ..Self::default() }
    }
}

impl Default for Options {
    fn default() -> Self {
        Self { redact: true, window: std::time::Duration::from_secs(24 * 60 * 60), keep: vec![] }
    }
}

/// Everything a brief is built from. Empty slices are fine: `wirewolf check` has no history.
#[derive(Debug, Clone, Copy)]
pub struct Input<'a> {
    pub meta: &'a Meta,
    /// The state to describe: the current snapshot, or an incident's evidence.
    pub snapshot: &'a Snapshot,
    /// Set when the brief describes this incident rather than now.
    pub incident: Option<&'a Incident>,
    /// Newest first.
    pub incidents: &'a [Incident],
    /// In time order.
    pub samples: &'a [Sample],
    /// Newest first.
    pub speed: &'a [SpeedRun],
}

/// `GET /api/v1/brief?format=json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Brief {
    pub version: u32,
    pub mode: String,
    pub at: Timestamp,
    /// The incident this brief describes, if it isn't about now.
    pub incident: Option<BriefIncident>,
    pub severity: Severity,
    pub confidence: Confidence,
    pub headline: String,
    pub detail: String,
    pub blame: Option<String>,
    pub fix_hint: Option<String>,
    pub method: String,
    /// In path order.
    pub hops: Vec<BriefHop>,
    pub targets: Vec<BriefTarget>,
    /// The most recent few, newest first.
    pub incidents: Vec<BriefIncident>,
    pub incident_count: usize,
    pub history: Option<HistorySummary>,
    /// The most recent few completed runs, newest first.
    pub speed: Vec<BriefSpeed>,
    pub speed_count: usize,
    /// How many distinct addresses were replaced with placeholders.
    pub redacted: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BriefHop {
    pub id: String,
    pub status: Status,
    pub target: Option<String>,
    pub summary: String,
    pub metrics: HopMetrics,
    /// `key=value` facts, minus the noisy ones.
    pub evidence: Vec<(String, String)>,
    /// Checks that didn't pass and do apply: `(status, name, detail)`.
    pub problems: Vec<(Status, String, String)>,
    /// Checks that don't apply, grouped by reason: `(reason, check names)`.
    pub not_applicable: Vec<(String, Vec<String>)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BriefTarget {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub critical: bool,
    pub status: Status,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BriefIncident {
    pub id: String,
    pub severity: Severity,
    pub started: Timestamp,
    pub ended: Option<Timestamp>,
    pub blame: Option<String>,
    pub headline: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistorySummary {
    pub since: Timestamp,
    pub until: Timestamp,
    pub samples: usize,
    /// One row per hop or target with any measurements, in first-seen order.
    pub hops: Vec<HopSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HopSummary {
    pub hop: String,
    pub rtt_p50_ms: Option<f64>,
    pub rtt_p95_ms: Option<f64>,
    pub rtt_max_ms: Option<f64>,
    pub loss_max_pct: Option<f64>,
    /// Samples that weren't `pass`.
    pub not_pass: usize,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BriefSpeed {
    pub started: Timestamp,
    pub down_mbps: Option<f64>,
    pub up_mbps: Option<f64>,
    pub idle_latency_ms: Option<f64>,
    pub loaded_latency_ms: Option<f64>,
    pub bloat: Option<BloatGrade>,
}

/// Builds the brief for `input`.
pub fn build(input: Input<'_>, options: &Options) -> Brief {
    let mut r = Redactor::new(options);
    let snap = input.snapshot;
    let v = &snap.verdict;
    let brief_incident = |i: &Incident, r: &mut Redactor| BriefIncident {
        id: i.id.clone(),
        severity: i.severity,
        started: i.started,
        ended: i.ended,
        blame: i.blame.clone(),
        headline: r.text(&i.headline),
    };
    let hops = snap
        .hops
        .iter()
        .map(|h| {
            let mut not_applicable: Vec<(String, Vec<String>)> = vec![];
            let mut problems = vec![];
            for c in &h.checks {
                match c.status {
                    Status::Pass => {}
                    Status::NotApplicable => match not_applicable.iter_mut().find(|(reason, _)| *reason == c.detail) {
                        Some((_, names)) => names.push(c.name.clone()),
                        None => not_applicable.push((c.detail.clone(), vec![c.name.clone()])),
                    },
                    status => problems.push((status, c.name.clone(), r.text(&c.detail))),
                }
            }
            BriefHop {
                id: h.id.clone(),
                status: h.status,
                target: h.target.as_deref().map(|t| r.text(t)),
                summary: r.text(&h.summary),
                metrics: h.metrics,
                evidence: h.evidence.iter().filter(|e| !SKIP_EVIDENCE.contains(&e.key.as_str())).map(|e| (e.key.clone(), r.text(&e.value))).collect(),
                problems,
                not_applicable,
            }
        })
        .collect();
    let targets = snap
        .targets
        .iter()
        .map(|t| BriefTarget { id: t.id.clone(), name: t.name.clone(), kind: t.kind.clone(), critical: t.critical, status: t.status, detail: r.text(&t.detail) })
        .collect();
    let incidents = input.incidents.iter().take(INCIDENTS).map(|i| brief_incident(i, &mut r)).collect();
    let speed: Vec<_> = input.speed.iter().filter(|s| s.state == SpeedState::Complete).collect();
    Brief {
        version: VERSION,
        mode: input.meta.mode.clone(),
        at: snap.at,
        incident: input.incident.map(|i| brief_incident(i, &mut r)),
        severity: v.severity,
        confidence: v.confidence,
        headline: r.text(&v.headline),
        detail: r.text(&v.detail),
        blame: v.blame.clone(),
        fix_hint: v.fix_hint.as_deref().map(|f| r.text(f)),
        method: v.method.clone(),
        hops,
        targets,
        incidents,
        incident_count: input.incidents.len(),
        history: summarize(input.samples, snap.at, options.window),
        speed_count: speed.len(),
        speed: speed
            .iter()
            .take(SPEED_RUNS)
            .map(|s| BriefSpeed { started: s.started, down_mbps: s.down_mbps, up_mbps: s.up_mbps, idle_latency_ms: s.idle_latency_ms, loaded_latency_ms: s.loaded_latency_ms, bloat: s.bloat })
            .collect(),
        redacted: r.tokens.len(),
    }
}

/// Per-hop statistics over the `window` ending at `until`.
fn summarize(samples: &[Sample], until: Timestamp, window: std::time::Duration) -> Option<HistorySummary> {
    let since = until.checked_sub(jiff::SignedDuration::try_from(window).ok()?).ok()?;
    let in_window: Vec<&Sample> = samples.iter().filter(|s| s.at >= since && s.at <= until).collect();
    let (first, last) = (in_window.first()?, in_window.last()?);
    let mut order = vec![];
    let mut by_hop: BTreeMap<&str, Vec<&Sample>> = BTreeMap::new();
    for s in &in_window {
        by_hop.entry(&s.hop).or_insert_with(|| {
            order.push(s.hop.as_str());
            vec![]
        });
        by_hop.get_mut(s.hop.as_str()).expect("just inserted").push(s);
    }
    let hops = order
        .into_iter()
        .filter_map(|hop| {
            let ss = &by_hop[hop];
            let mut rtt: Vec<f64> = ss.iter().filter_map(|s| s.rtt_ms).collect();
            rtt.sort_by(f64::total_cmp);
            let not_pass = ss.iter().filter(|s| s.status != Status::Pass).count();
            if rtt.is_empty() && not_pass == 0 {
                return None;
            }
            let pct = |p: f64| (!rtt.is_empty()).then(|| rtt[((rtt.len() - 1) as f64 * p).round() as usize]);
            Some(HopSummary {
                hop: hop.to_owned(),
                rtt_p50_ms: pct(0.5),
                rtt_p95_ms: pct(0.95),
                rtt_max_ms: rtt.last().copied(),
                loss_max_pct: ss.iter().filter_map(|s| s.loss_pct).reduce(f64::max),
                not_pass,
                total: ss.len(),
            })
        })
        .collect();
    Some(HistorySummary { since: first.at, until: last.at, samples: in_window.len(), hops })
}

/// The request to an assistant that goes in front of the brief.
pub const PROMPT: &str = "I run wirewolf, a network monitor on my home network. It has already \
    diagnosed the problem below. Explain in plain language what is wrong and what I should do, in \
    order, starting with the cheapest thing to try, and say what each step would rule out. If the \
    data points to my internet provider, tell me what to say to them. Don't invent measurements \
    that aren't in the brief, and ask me for anything else you need.";

/// [`PROMPT`] plus the brief, ready to paste into a chat assistant.
pub fn prompt(brief: &Brief) -> String {
    format!("{PROMPT}\n\n<brief>\n{}</brief>\n", brief.text())
}

impl Brief {
    /// The plain-text form. Each line stands alone, so it survives being cut short.
    pub fn text(&self) -> String {
        let mut o = String::new();
        let mut line = |s: String| {
            o.push_str(&s);
            o.push('\n');
        };
        line(format!("# wirewolf brief v{} · {} · {}", self.version, self.mode, minute(self.at)));
        if let Some(i) = &self.incident {
            line(format!("about: incident {} ({}), not the current state", i.id, span(i)));
        }
        line(format!("verdict: {} ({} confidence) — {}", upper(self.severity), lower(self.confidence), self.headline));
        line(format!("detail: {}", self.detail));
        if let Some(b) = &self.blame {
            line(format!("blame: {b}"));
        }
        if let Some(f) = &self.fix_hint {
            line(format!("fix_hint: {f}"));
        }
        line(format!("method: {}", self.method));
        line(String::new());
        line("## path (in order; the first hop that isn't pass is the break, later skips are collateral)".into());
        for h in &self.hops {
            let m = metrics(&h.metrics);
            let target = h.target.as_deref().map(|t| format!(" {t}")).unwrap_or_default();
            line(format!("- {} [{}]{target} — {}{}", h.id, lower(h.status), h.summary, if m.is_empty() { String::new() } else { format!(" · {m}") }));
            if !h.evidence.is_empty() {
                line(format!("  evidence: {}", h.evidence.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("; ")));
            }
            for (status, name, detail) in &h.problems {
                line(format!("  check {}: {name} — {detail}", lower(*status)));
            }
            for (reason, names) in &h.not_applicable {
                line(format!("  n/a: {} — {reason}", names.join(", ")));
            }
        }
        if !self.targets.is_empty() {
            line(String::new());
            line("## your targets".into());
            for t in &self.targets {
                line(format!("- {} ({}{}) [{}] {}", t.name, t.kind, if t.critical { ", critical" } else { "" }, lower(t.status), t.detail));
            }
        }
        line(String::new());
        line(format!("## incidents (latest {} of {})", self.incidents.len(), self.incident_count));
        for i in &self.incidents {
            line(format!("- {} {} {} blame={} — {}", i.id, lower(i.severity), span(i), i.blame.as_deref().unwrap_or("none"), i.headline));
        }
        if let Some(h) = &self.history {
            line(String::new());
            line(format!("## history ({} → {}, {} samples)", minute(h.since), minute(h.until), h.samples));
            for s in &h.hops {
                let mut parts = vec![];
                if let (Some(p50), Some(p95), Some(max)) = (s.rtt_p50_ms, s.rtt_p95_ms, s.rtt_max_ms) {
                    parts.push(format!("rtt p50={p50:.1} p95={p95:.1} max={max:.1}ms"));
                }
                if let Some(l) = s.loss_max_pct {
                    parts.push(format!("loss max={l:.0}%"));
                }
                parts.push(format!("not pass {}/{}", s.not_pass, s.total));
                line(format!("- {}: {}", s.hop, parts.join("; ")));
            }
        }
        if !self.speed.is_empty() {
            line(String::new());
            line(format!("## speed (latest {} of {})", self.speed.len(), self.speed_count));
            for s in &self.speed {
                let num = |x: Option<f64>| x.map_or("?".into(), |x| format!("{x:.0}"));
                let bloat = s.bloat.map(|b| serde_json::to_value(b).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()).unwrap_or_else(|| "?".into());
                line(format!(
                    "- {} down={} up={} Mbps, latency idle={} loaded={} ms, bufferbloat {bloat}",
                    minute(s.started),
                    num(s.down_mbps),
                    num(s.up_mbps),
                    num(s.idle_latency_ms),
                    num(s.loaded_latency_ms)
                ));
            }
        }
        if self.redacted > 0 {
            line(String::new());
            line(format!("redacted: {} public address(es) shown as <public-ip-N>", self.redacted));
        }
        o
    }
}

fn metrics(m: &HopMetrics) -> String {
    let mut s = String::new();
    for (name, value, unit) in [("rtt", m.rtt_ms, "ms"), ("jitter", m.jitter_ms, "ms"), ("loss", m.loss_pct, "%")] {
        if let Some(v) = value {
            let _ = write!(s, "{}{name}={v:.1}{unit}", if s.is_empty() { "" } else { " " });
        }
    }
    s
}

fn span(i: &BriefIncident) -> String {
    format!("{} → {}", minute(i.started), i.ended.map_or("ongoing".into(), minute))
}

/// `2026-10-24 10:56Z`: minutes are enough, and seconds cost tokens.
fn minute(t: Timestamp) -> String {
    t.strftime("%Y-%m-%d %H:%MZ").to_string()
}

fn lower<T: Serialize>(v: T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
}

fn upper<T: Serialize>(v: T) -> String {
    lower(v).to_uppercase()
}

/// Replaces public IP addresses in text with stable placeholders.
struct Redactor<'a> {
    on: bool,
    keep: &'a [String],
    tokens: HashMap<IpAddr, String>,
}

impl<'a> Redactor<'a> {
    fn new(options: &'a Options) -> Self {
        Self { on: options.redact, keep: &options.keep, tokens: HashMap::new() }
    }

    fn text(&mut self, s: &str) -> String {
        if !self.on {
            return s.to_owned();
        }
        let mut out = String::with_capacity(s.len());
        let mut word = String::new();
        for c in s.chars().chain(std::iter::once(' ')) {
            if c.is_ascii_hexdigit() || c == '.' || c == ':' || c == '/' {
                word.push(c);
                continue;
            }
            out.push_str(&self.word(&word));
            word.clear();
            out.push(c);
        }
        out.pop();
        out
    }

    /// One run of address-like characters, for example `203.0.113.7/24` or `10:56`.
    fn word(&mut self, w: &str) -> String {
        // Trailing punctuation ("…at 1.2.3.4.") isn't part of the address.
        let trimmed = w.trim_end_matches(['.', ':']);
        let (addr, rest) = trimmed.split_once('/').map_or((trimmed, ""), |(a, p)| (a, p));
        let Ok(ip) = addr.parse::<IpAddr>() else { return w.to_owned() };
        if !public(ip) || WELL_KNOWN.contains(&addr) || self.keep.iter().any(|k| k == addr) {
            return w.to_owned();
        }
        let n = self.tokens.len() + 1;
        let token = self.tokens.entry(ip).or_insert_with(|| format!("<public-ip-{n}>")).clone();
        let prefix = if rest.is_empty() { String::new() } else { format!("/{rest}") };
        format!("{token}{prefix}{}", &w[trimmed.len()..])
    }
}

/// Whether an address could identify the user: globally routable, not a documentation example.
fn public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            let shared = a == 100 && (64..128).contains(&b);
            let documentation = matches!((a, b, c), (192, 0, 2) | (198, 51, 100) | (203, 0, 113));
            !(v4.is_private() || v4.is_loopback() || v4.is_link_local() || v4.is_unspecified() || v4.is_broadcast() || v4.is_multicast() || shared || documentation)
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            let global = (s[0] & 0xe000) == 0x2000;
            let documentation = s[0] == 0x2001 && s[1] == 0x0db8;
            global && !documentation
        }
    }
}

#[cfg(all(test, feature = "demo"))]
mod tests {
    use super::*;

    fn input<'a>(meta: &'a Meta, snapshot: &'a Snapshot) -> Input<'a> {
        Input { meta, snapshot, incident: None, incidents: &[], samples: &[], speed: &[] }
    }

    #[test]
    fn redacts_public_addresses_only() {
        let options = Options { keep: vec!["192.0.2.9".into()], ..Options::default() };
        let mut r = Redactor::new(&options);
        let s = r.text("me 93.184.215.14/24, gw 192.168.1.1, isp 100.64.0.1, dns 1.1.1.1, again 93.184.215.14. v6 2606:2800:21f:cb07::68 at 10:56");
        assert_eq!(s, "me <public-ip-1>/24, gw 192.168.1.1, isp 100.64.0.1, dns 1.1.1.1, again <public-ip-1>. v6 <public-ip-2> at 10:56");
        assert_eq!(r.text("fd7a:115c:a1e0::53 and 2001:db8::1"), "fd7a:115c:a1e0::53 and 2001:db8::1");
        let off = Options { redact: false, ..Options::default() };
        assert_eq!(Redactor::new(&off).text("93.184.215.14"), "93.184.215.14");
    }

    #[test]
    fn every_demo_scenario_briefs_compactly() {
        for scenario in crate::demo::scenarios() {
            let meta = crate::demo::meta(&scenario.id);
            let snapshot = crate::demo::snapshot(&scenario.id, Timestamp::now());
            let brief = build(input(&meta, &snapshot), &Options::default());
            let text = brief.text();
            assert!(text.starts_with("# wirewolf brief v1 · demo"), "{text}");
            assert!(text.contains(&format!("verdict: {}", upper(snapshot.verdict.severity))), "{text}");
            assert!(text.len() < 4000, "{} is {} bytes", scenario.id, text.len());
            let json = serde_json::to_string(&brief).unwrap();
            assert_eq!(serde_json::from_str::<Brief>(&json).unwrap(), brief);
        }
    }

    #[test]
    fn brief_describes_the_break_and_history() {
        let meta = crate::demo::meta("dns_down");
        let at = Timestamp::now();
        let snapshot = crate::demo::snapshot("dns_down", at);
        let samples: Vec<Sample> = (0..10)
            .map(|i| Sample { at: at - jiff::SignedDuration::from_mins(i), hop: "dns".into(), status: if i < 2 { Status::Fail } else { Status::Pass }, rtt_ms: Some(10.0 + i as f64), jitter_ms: None, loss_pct: Some(0.0) })
            .rev()
            .collect();
        let brief = build(Input { samples: &samples, ..input(&meta, &snapshot) }, &Options::default());
        let text = brief.text();
        assert!(text.contains("- dns [fail]"), "{text}");
        assert!(text.contains("- dns: rtt p50=") && text.contains("not pass 2/10"), "{text}");
        assert!(text.contains("blame: dns"), "{text}");
    }
}
