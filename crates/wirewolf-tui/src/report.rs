//! Plain-text renderings of a snapshot, for "Copy report" and `wirewolf check`.

use std::fmt::Write;

use jiff::Timestamp;
use wirewolf_core::model::{Hop, Incident, Snapshot};

use crate::format;

/// A summary of an incident suitable for pasting to an ISP.
pub fn incident(incident: &Incident, host: Option<&str>) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Network incident report (wirewolf)");
    let _ = writeln!(out, "==================================");
    if let Some(host) = host {
        let _ = writeln!(out, "Measured from: {host}");
    }
    let _ = writeln!(out, "Started:       {}", format::datetime(incident.started));
    match incident.ended {
        Some(ended) => {
            let _ = writeln!(out, "Ended:         {}", format::datetime(ended));
            let _ = writeln!(
                out,
                "Duration:      {}",
                format::duration(ended.duration_since(incident.started))
            );
        }
        None => {
            let elapsed = Timestamp::now().duration_since(incident.started);
            let _ = writeln!(
                out,
                "Ended:         ongoing ({} so far)",
                format::duration(elapsed)
            );
        }
    }
    let _ = writeln!(
        out,
        "Worst state:   {}",
        format::severity_word(incident.severity).to_uppercase()
    );
    let _ = writeln!(out);
    out.push_str(&snapshot(&incident.evidence));
    out
}

/// The verdict, the path, and the evidence for the blamed hop.
pub fn snapshot(snapshot: &Snapshot) -> String {
    let verdict = &snapshot.verdict;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}: {}",
        format::severity_word(verdict.severity).to_uppercase(),
        verdict.headline
    );
    let _ = writeln!(out, "{}", verdict.detail);
    if let Some(hint) = &verdict.fix_hint {
        let _ = writeln!(out, "Suggested fix: {hint}");
    }
    let _ = writeln!(
        out,
        "Confidence: {:?}. Method: {}.",
        verdict.confidence,
        verdict.method.trim_end_matches('.')
    );
    let _ = writeln!(out, "Measured at: {}", format::datetime(snapshot.at));
    let _ = writeln!(out);
    let _ = writeln!(out, "Path:");
    let label_width = snapshot
        .hops
        .iter()
        .map(|h| h.label.chars().count())
        .max()
        .unwrap_or(0);
    let target_width = snapshot
        .hops
        .iter()
        .map(|h| target(h).chars().count())
        .max()
        .unwrap_or(0);
    for hop in &snapshot.hops {
        let blamed = verdict.blame.as_deref() == Some(hop.id.as_str());
        let _ = writeln!(
            out,
            "  [{:<4}] {:<label_width$}  {:<target_width$}  {}{}",
            format::status_word(hop.status).to_uppercase(),
            hop.label,
            target(hop),
            hop.summary,
            if blamed { "   <-- problem here" } else { "" }
        );
    }
    if let Some(hop) = verdict
        .blame
        .as_deref()
        .and_then(|id| snapshot.hops.iter().find(|h| h.id == id))
    {
        let _ = writeln!(out);
        hop_detail(&mut out, hop);
    }
    out
}

fn hop_detail(out: &mut String, hop: &Hop) {
    let _ = writeln!(out, "{} checks:", hop.label);
    for check in &hop.checks {
        let rtt = check
            .rtt_ms
            .map(|rtt| format!(" ({})", format::ms(rtt)))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  [{:<4}] {}: {}{rtt}",
            format::status_word(check.status).to_uppercase(),
            check.name,
            check.detail
        );
    }
    if !hop.evidence.is_empty() {
        let _ = writeln!(out, "{} evidence:", hop.label);
        for evidence in &hop.evidence {
            let _ = writeln!(out, "  {} = {}", evidence.key, evidence.value);
        }
    }
}

/// The target column: collapsed and capped so the summaries stay aligned and readable.
fn target(hop: &Hop) -> String {
    hop.target.as_deref().map(|t| format::target(t, 40)).unwrap_or_else(|| "-".into())
}
