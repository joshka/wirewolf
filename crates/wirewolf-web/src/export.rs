//! Data out of wirewolf: CSV downloads of history, incidents, and speed tests, and the current
//! state as Prometheus metrics. Both are served under `/api/v1`, so the read policy applies.

use std::fmt::Write;

use serde::Serialize;
use wirewolf_core::model::{Incident, Sample, Severity, Snapshot, SpeedRun, Status};

/// A serde enum's wire name (`"fail"`, `"down"`, `"A+"`).
fn word<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

/// One CSV field: quoted when it holds a comma, quote, or line break (RFC 4180).
fn field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

fn num(v: Option<f64>) -> String {
    v.map(|v| v.to_string()).unwrap_or_default()
}

fn csv(header: &[&str], rows: impl IntoIterator<Item = Vec<String>>) -> String {
    let mut out = header.join(",");
    out.push('\n');
    for row in rows {
        out.push_str(&row.iter().map(|f| field(f)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

pub fn history_csv(samples: &[Sample]) -> String {
    csv(
        &["at", "hop", "status", "rtt_ms", "jitter_ms", "loss_pct"],
        samples.iter().map(|s| vec![s.at.to_string(), s.hop.clone(), word(&s.status), num(s.rtt_ms), num(s.jitter_ms), num(s.loss_pct)]),
    )
}

pub fn incidents_csv(incidents: &[Incident]) -> String {
    csv(
        &["id", "started", "ended", "duration_s", "severity", "blame", "headline"],
        incidents.iter().map(|i| {
            let secs = i.ended.map(|e| e.duration_since(i.started).as_secs().to_string()).unwrap_or_default();
            vec![
                i.id.clone(),
                i.started.to_string(),
                i.ended.map(|e| e.to_string()).unwrap_or_default(),
                secs,
                word(&i.severity),
                i.blame.clone().unwrap_or_default(),
                i.headline.clone(),
            ]
        }),
    )
}

pub fn speed_csv(runs: &[SpeedRun]) -> String {
    csv(
        &["id", "started", "state", "backend", "server", "down_mbps", "up_mbps", "idle_latency_ms", "loaded_latency_ms", "bloat", "bytes_used", "error"],
        runs.iter().map(|r| {
            vec![
                r.id.clone(),
                r.started.to_string(),
                word(&r.state),
                r.backend.clone(),
                r.server.clone().unwrap_or_default(),
                num(r.down_mbps),
                num(r.up_mbps),
                num(r.idle_latency_ms),
                num(r.loaded_latency_ms),
                r.bloat.map(|b| word(&b)).unwrap_or_default(),
                r.bytes_used.map(|b| b.to_string()).unwrap_or_default(),
                r.error.clone().unwrap_or_default(),
            ]
        }),
    )
}

/// A Prometheus label value, escaped.
fn label(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

/// Writes one metric family: its HELP and TYPE lines, then one sample per `(labels, value)`.
fn family(out: &mut String, name: &str, help: &str, samples: impl IntoIterator<Item = (String, f64)>) {
    let samples: Vec<_> = samples.into_iter().collect();
    if samples.is_empty() {
        return;
    }
    let _ = writeln!(out, "# HELP {name} {help}\n# TYPE {name} gauge");
    for (labels, v) in samples {
        let _ = if labels.is_empty() { writeln!(out, "{name} {v}") } else { writeln!(out, "{name}{{{labels}}} {v}") };
    }
}

const SEVERITIES: [Severity; 4] = [Severity::Healthy, Severity::Degraded, Severity::Down, Severity::Unknown];
const STATUSES: [Status; 6] = [Status::Pass, Status::Warn, Status::Fail, Status::Skip, Status::NotApplicable, Status::Pending];

/// The current state in the Prometheus text format, in base units (seconds, ratios, bits per
/// second). States are one-hot (`wirewolf_verdict{severity="down"} 1`), so an alert is a plain
/// comparison.
pub fn metrics(snapshot: Option<&Snapshot>, runs: &[SpeedRun], incidents: &[Incident]) -> String {
    let mut out = String::new();
    if let Some(s) = snapshot {
        let v = &s.verdict;
        family(&mut out, "wirewolf_verdict", "The overall verdict: 1 for the current severity, 0 for the others.", SEVERITIES.iter().map(|sev| (format!("severity=\"{}\"", word(sev)), f64::from(u8::from(*sev == v.severity)))));
        let hop = |id: &str, kind: &str| format!("hop=\"{}\",kind=\"{}\"", label(id), label(kind));
        family(
            &mut out,
            "wirewolf_hop_status",
            "Each hop's status: 1 for the current status, 0 for the others.",
            s.hops.iter().flat_map(|h| STATUSES.iter().map(move |st| (format!("{},status=\"{}\"", hop(&h.id, &word(&h.kind)), word(st)), f64::from(u8::from(*st == h.status))))),
        );
        family(&mut out, "wirewolf_hop_rtt_seconds", "Each hop's round-trip time.", s.hops.iter().filter_map(|h| h.metrics.rtt_ms.map(|r| (hop(&h.id, &word(&h.kind)), r / 1000.0))));
        family(&mut out, "wirewolf_hop_jitter_seconds", "Each hop's jitter.", s.hops.iter().filter_map(|h| h.metrics.jitter_ms.map(|j| (hop(&h.id, &word(&h.kind)), j / 1000.0))));
        family(&mut out, "wirewolf_hop_loss_ratio", "Each hop's loss, from 0 to 1.", s.hops.iter().filter_map(|h| h.metrics.loss_pct.map(|l| (hop(&h.id, &word(&h.kind)), l / 100.0))));
        let target = |t: &wirewolf_core::model::Target| format!("target=\"{}\",name=\"{}\",critical=\"{}\"", label(&t.id), label(&t.name), t.critical);
        family(&mut out, "wirewolf_target_up", "1 when a user target passes or is degraded, 0 when it fails.", s.targets.iter().map(|t| (target(t), f64::from(u8::from(t.status != Status::Fail)))));
        family(&mut out, "wirewolf_target_rtt_seconds", "Each user target's round-trip time.", s.targets.iter().filter_map(|t| t.rtt_ms.map(|r| (target(t), r / 1000.0))));
        family(&mut out, "wirewolf_last_check_timestamp_seconds", "When the current verdict was measured.", [(String::new(), s.at.as_second() as f64)]);
    }
    let open = incidents.iter().filter(|i| i.ended.is_none()).count();
    family(&mut out, "wirewolf_incident_open", "1 while an incident is open.", [(String::new(), f64::from(u8::from(open > 0)))]);
    if let Some(r) = runs.iter().find(|r| r.state == wirewolf_core::model::SpeedState::Complete) {
        let mbps = |v: Option<f64>| v.map(|v| v * 1e6);
        family(&mut out, "wirewolf_speed_download_bits_per_second", "The latest completed speed test's download.", mbps(r.down_mbps).map(|v| (String::new(), v)));
        family(&mut out, "wirewolf_speed_upload_bits_per_second", "The latest completed speed test's upload.", mbps(r.up_mbps).map(|v| (String::new(), v)));
        family(
            &mut out,
            "wirewolf_speed_latency_seconds",
            "The latest completed speed test's latency, idle and under load.",
            [("idle", r.idle_latency_ms), ("loaded", r.loaded_latency_ms)].into_iter().filter_map(|(k, v)| v.map(|v| (format!("load=\"{k}\""), v / 1000.0))),
        );
        family(&mut out, "wirewolf_speed_last_run_timestamp_seconds", "When the latest completed speed test started.", [(String::new(), r.started.as_second() as f64)]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quotes_only_when_needed() {
        assert_eq!(field("plain"), "plain");
        assert_eq!(field("DNS is failing, badly"), "\"DNS is failing, badly\"");
        assert_eq!(field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn labels_are_escaped() {
        assert_eq!(label("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
    }

    #[test]
    fn empty_state_still_reports_no_open_incident() {
        let m = metrics(None, &[], &[]);
        assert!(m.contains("wirewolf_incident_open 0"), "{m}");
        assert!(!m.contains("wirewolf_verdict"), "{m}");
    }
}
