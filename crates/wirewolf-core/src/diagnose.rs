//! Turns a path of hops into one verdict.
//!
//! The rule, borrowed from wtfi2: blame the *first* break on the path and treat everything
//! downstream as collateral. With no hard break, blame the first degraded hop. A few patterns get
//! a more specific explanation (DNS failing while the internet answers by IP, HTTP intercepted
//! while DNS and TCP work).

use crate::model::{Confidence, Hop, HopKind, Severity, Status, Target, Verdict};

/// Diagnoses a path. `hops` must be in path order.
pub fn diagnose(hops: &[Hop]) -> Verdict {
    let probes = hops.iter().map(|h| h.checks.len().max(1)).sum::<usize>();
    let method = format!("{probes} checks across {} hops", hops.len());

    if hops.iter().all(|h| matches!(h.status, Status::Pending | Status::NotApplicable)) {
        return Verdict {
            severity: Severity::Unknown,
            headline: "Waiting for the first measurements".into(),
            detail: "No probes have completed yet.".into(),
            blame: None,
            confidence: Confidence::Low,
            fix_hint: None,
            method,
        };
    }

    let status_of = |kind: HopKind| hops.iter().find(|h| h.kind == kind).map(|h| h.status);

    if let Some(hop) = hops.iter().find(|h| h.status == Status::Fail) {
        let (headline, detail, fix_hint) = explain_failure(hop, &status_of);
        return Verdict {
            severity: Severity::Down,
            headline,
            detail,
            blame: Some(hop.id.clone()),
            confidence: if hop.checks.len() >= 2 { Confidence::High } else { Confidence::Medium },
            fix_hint,
            method,
        };
    }

    if let Some(hop) = hops.iter().find(|h| h.status == Status::Warn) {
        let (headline, detail, fix_hint) = explain_degraded(hop);
        return Verdict {
            severity: Severity::Degraded,
            headline,
            detail,
            blame: Some(hop.id.clone()),
            confidence: Confidence::Medium,
            fix_hint,
            method,
        };
    }

    Verdict {
        severity: Severity::Healthy,
        headline: "Everything looks good".into(),
        detail: "The gateway, internet, DNS, and web checks all pass.".into(),
        blame: None,
        confidence: Confidence::High,
        fix_hint: None,
        method,
    }
}

type Explanation = (String, String, Option<String>);

fn explain_failure(hop: &Hop, status_of: &dyn Fn(HopKind) -> Option<Status>) -> Explanation {
    let s = |x: &str| x.to_string();
    match hop.kind {
        HopKind::Host => (
            s("This machine has no working network connection"),
            s("The network interface is down or has no usable address."),
            Some(s("Check the cable or Wi-Fi association, then DHCP.")),
        ),
        HopKind::Gateway => (
            s("Can't reach the router"),
            s("The default gateway isn't answering, so nothing beyond it can be reached."),
            Some(s("Check the router is powered on and the local link (cable or Wi-Fi) is up.")),
        ),
        HopKind::Isp => (
            s("Your internet provider's connection is down"),
            s("The router answers, but traffic dies just beyond it on the provider's side."),
            Some(s("Check the modem/ONT lights and your ISP's status page.")),
        ),
        HopKind::Internet => (
            s("The internet is unreachable"),
            s("The router answers, but no internet hosts accept connections."),
            Some(s("Restart the modem/router; if it persists, contact your ISP.")),
        ),
        HopKind::Dns if status_of(HopKind::Internet) == Some(Status::Pass) => (
            s("DNS is failing — the internet is reachable by IP"),
            s("Hosts answer by IP address, but names don't resolve. Websites will look down."),
            Some(s("Switch resolver (for example 1.1.1.1) or restart the router's DNS forwarder.")),
        ),
        HopKind::Dns => (
            s("DNS is failing"),
            s("Names don't resolve."),
            Some(s("Check the configured resolver.")),
        ),
        HopKind::Web => (
            s("Web traffic is being intercepted"),
            s("DNS and connections work, but HTTP responses are not what was expected. This is \
               usually a captive portal or a filtering proxy."),
            Some(s("Open a browser to complete any login page, or check proxy settings.")),
        ),
    }
}

fn explain_degraded(hop: &Hop) -> Explanation {
    let s = |x: &str| x.to_string();
    let loss = hop.metrics.loss_pct.unwrap_or(0.0);
    let lossy = loss >= 1.0;
    let side = match hop.kind {
        HopKind::Host | HopKind::Gateway => "on your local network (Wi-Fi or LAN)",
        HopKind::Isp => "at your internet provider",
        // The internet hop is the anycast targets, so slowness there isn't pinned on the ISP.
        HopKind::Internet => "reaching the internet",
        HopKind::Dns => "in name resolution",
        HopKind::Web => "in web requests",
    };
    if lossy {
        (
            format!("Packet loss {side}"),
            format!("{} is dropping {loss:.0}% of packets; everything after it inherits that.", hop.label),
            Some(match hop.kind {
                HopKind::Host | HopKind::Gateway => s("Check Wi-Fi signal or interference, or try a cable."),
                _ => s("If it persists, report the loss to your ISP with the incident report."),
            }),
        )
    } else {
        (
            format!("Slow {side}"),
            format!("{} is responding slowly ({}).", hop.label, hop.summary),
            Some(match hop.kind {
                HopKind::Host | HopKind::Gateway => s("Check Wi-Fi signal and whether a big upload, download, or backup is saturating the network."),
                HopKind::Isp | HopKind::Internet => s("Check nothing on your network is using the whole connection, then run a speed test."),
                HopKind::Dns => s("Compare against another resolver (for example 1.1.1.1); switch if it's faster."),
                HopKind::Web => s("If every site is slow, run a speed test; if only some are, it's those sites."),
            }),
        )
    }
}

/// Folds user targets into a verdict. A failing `critical` target degrades an otherwise healthy
/// verdict and is blamed by its id. A broken path stays the headline, since it explains the
/// target too.
pub fn with_targets(verdict: Verdict, targets: &[Target]) -> Verdict {
    if verdict.severity != Severity::Healthy {
        return verdict;
    }
    let bad = targets.iter().filter(|t| t.critical && matches!(t.status, Status::Fail | Status::Warn));
    let Some(target) = bad.clone().find(|t| t.status == Status::Fail).or_else(|| bad.clone().next()) else {
        return verdict;
    };
    let others = bad.count() - 1;
    let more = match others {
        0 => String::new(),
        1 => " (and 1 more target)".into(),
        n => format!(" (and {n} more targets)"),
    };
    let failed = target.status == Status::Fail;
    Verdict {
        severity: Severity::Degraded,
        headline: if failed { format!("{} is unreachable{more}", target.name) } else { format!("{} is slow{more}", target.name) },
        detail: format!("The path to the internet is fine, but {} ({}) {}: {}.", target.name, target.check, if failed { "fails" } else { "is degraded" }, target.detail),
        blame: Some(target.id.clone()),
        confidence: Confidence::Medium,
        fix_hint: Some(format!("Check {} itself: power, network cable, or the service on it.", target.name)),
        method: format!("{}, plus {} targets", verdict.method, targets.len()),
    }
}

#[cfg(test)]
mod target_tests {
    use super::*;

    fn target(name: &str, critical: bool, status: Status) -> Target {
        Target {
            id: format!("target:{}", name.to_lowercase()),
            name: name.into(),
            check: "tcp://nas.local:445".into(),
            kind: "tcp".into(),
            critical,
            status,
            detail: "connection refused".into(),
            rtt_ms: None,
            loss_pct: None,
        }
    }

    fn healthy() -> Verdict {
        Verdict {
            severity: Severity::Healthy,
            headline: "Everything looks good".into(),
            detail: String::new(),
            blame: None,
            confidence: Confidence::High,
            fix_hint: None,
            method: "m".into(),
        }
    }

    #[test]
    fn only_critical_targets_degrade() {
        let v = with_targets(healthy(), &[target("Printer", false, Status::Fail)]);
        assert_eq!(v.severity, Severity::Healthy);
        let v = with_targets(healthy(), &[target("Printer", false, Status::Fail), target("NAS", true, Status::Fail)]);
        assert_eq!(v.severity, Severity::Degraded);
        assert_eq!(v.blame.as_deref(), Some("target:nas"));
        assert_eq!(v.headline, "NAS is unreachable");
    }
}
