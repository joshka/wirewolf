//! Turns a path of hops into one verdict.
//!
//! The rule, borrowed from wtfi2: blame the *first* break on the path and treat everything
//! downstream as collateral. With no hard break, blame the first degraded hop. A few patterns get
//! a more specific explanation (DNS failing while the internet answers by IP, HTTP intercepted
//! while DNS and TCP work).

use crate::model::{Confidence, Hop, HopKind, Severity, Status, Verdict};

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
        HopKind::Isp | HopKind::Internet => "at your internet provider",
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
            None,
        )
    }
}
