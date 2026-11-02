//! Turns a path of hops into one verdict.
//!
//! The rule, borrowed from wtfi2: blame the *first* break on the path and treat everything
//! downstream as collateral. With no hard break, blame the first degraded hop. A few patterns get
//! a more specific explanation (DNS failing while the internet answers by IP, HTTP intercepted
//! while DNS and TCP work).

use crate::config::Thresholds;
use crate::explain::{explain, explain_target};
use crate::model::{Confidence, Hop, HopKind, Severity, Status, Target, Verdict};

/// Diagnoses a path with the default limits. `hops` must be in path order.
pub fn diagnose(hops: &[Hop]) -> Verdict {
    diagnose_with(hops, &Thresholds::default())
}

/// Diagnoses a path and explains it against the configured limits. `hops` must be in path order.
pub fn diagnose_with(hops: &[Hop], limits: &Thresholds) -> Verdict {
    let mut verdict = conclude(hops);
    explain(&mut verdict, hops, limits);
    verdict
}

fn conclude(hops: &[Hop]) -> Verdict {
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
            bottom_line: None,
            confidence_reason: None,
            steps: vec![],
            held_cycles: None,
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
            bottom_line: None,
            confidence_reason: None,
            steps: vec![],
            held_cycles: None,
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
            bottom_line: None,
            confidence_reason: None,
            steps: vec![],
            held_cycles: None,
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
        bottom_line: None,
        confidence_reason: None,
        steps: vec![],
        held_cycles: None,
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

/// A warning about one of the host's ports, from its check name (`eth0 speed`, `eth0 errors`, …).
fn explain_port(hop: &Hop) -> Option<Explanation> {
    let check = hop.checks.iter().find(|c| c.status == Status::Warn)?;
    let (detail, hint) = match check.name.rsplit(' ').next()? {
        "speed" => (
            "The port negotiated below gigabit, which caps every transfer from this host.",
            "Check the cable: a damaged or old cable (or one with a broken pair) drops to 100 Mbps. Try another cable and switch port.",
        ),
        "duplex" => (
            "Half duplex means collisions and slow, lossy transfers. It usually comes from a failed autonegotiation.",
            "Set both ends to autonegotiate, then replace the cable if it persists.",
        ),
        "errors" => (
            "The port is receiving or sending corrupted frames, which get retransmitted or lost.",
            "Replace the cable or try another switch port; a bad patch lead is the usual cause.",
        ),
        "flapping" => (
            "The link went down and came back, interrupting everything on this host for a moment each time.",
            "Reseat or replace the cable, check the switch port, and turn off Energy-Efficient Ethernet if the switch offers it.",
        ),
        "subnet" => (
            "Two connected ports have addresses on the same network, so replies can leave by the wrong port.",
            "Bond the ports, or unplug or disable the one you don't use.",
        ),
        _ => return None,
    };
    Some((check.detail.clone(), detail.into(), Some(hint.into())))
}

fn explain_degraded(hop: &Hop) -> Explanation {
    let s = |x: &str| x.to_string();
    if let Some(e) = (hop.kind == HopKind::Host).then(|| explain_port(hop)).flatten() {
        return e;
    }
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
    let mut verdict = Verdict {
        severity: Severity::Degraded,
        headline: if failed { format!("{} is unreachable{more}", target.name) } else { format!("{} is slow{more}", target.name) },
        detail: format!("The path to the internet is fine, but {} ({}) {}: {}.", target.name, target.check, if failed { "fails" } else { "is degraded" }, target.detail),
        blame: Some(target.id.clone()),
        confidence: Confidence::Medium,
        fix_hint: Some(format!("Check {} itself: power, network cable, or the service on it.", target.name)),
        method: format!("{}, plus {} targets", verdict.method, targets.len()),
        bottom_line: None,
        confidence_reason: None,
        steps: verdict.steps,
        held_cycles: None,
    };
    explain_target(&mut verdict, target);
    verdict
}

#[cfg(test)]
mod path_tests {
    use super::*;
    use crate::model::{Check, HopMetrics};

    fn hop(id: &str, kind: HopKind, status: Status) -> Hop {
        Hop {
            id: id.into(),
            kind,
            label: id.to_uppercase(),
            target: None,
            status,
            summary: "6.2 ms, no loss".into(),
            metrics: HopMetrics { rtt_ms: Some(6.2), jitter_ms: Some(0.3), loss_pct: Some(0.0) },
            evidence: vec![],
            checks: vec![],
            interfaces: vec![],
        }
    }

    /// The six hops in path order, all passing.
    fn path() -> Vec<Hop> {
        vec![
            hop("host", HopKind::Host, Status::Pass),
            hop("gateway", HopKind::Gateway, Status::Pass),
            hop("isp", HopKind::Isp, Status::Pass),
            hop("internet", HopKind::Internet, Status::Pass),
            hop("dns", HopKind::Dns, Status::Pass),
            hop("web", HopKind::Web, Status::Pass),
        ]
    }

    fn with(mut hops: Vec<Hop>, id: &str, f: impl FnOnce(&mut Hop)) -> Vec<Hop> {
        f(hops.iter_mut().find(|h| h.id == id).expect("hop"));
        hops
    }

    /// Everything after a hard failure is collateral, as the live assess step marks it.
    fn fail_at(id: &str) -> Vec<Hop> {
        let mut hops = path();
        let at = hops.iter().position(|h| h.id == id).unwrap();
        hops[at].status = Status::Fail;
        for h in &mut hops[at + 1..] {
            h.status = Status::Skip;
        }
        hops
    }

    #[test]
    fn nothing_measured_is_unknown() {
        let hops: Vec<Hop> = path().into_iter().map(|mut h| {
            h.status = Status::Pending;
            h
        }).collect();
        let v = diagnose(&hops);
        assert_eq!((v.severity, v.confidence), (Severity::Unknown, Confidence::Low));
        assert_eq!(v.blame, None);
        assert!(v.steps.is_empty(), "no steps until something was measured");
    }

    #[test]
    fn all_pass_is_healthy_with_high_confidence() {
        let v = diagnose(&path());
        assert_eq!((v.severity, v.confidence), (Severity::Healthy, Confidence::High));
        assert_eq!(v.blame, None);
        assert_eq!(v.method, "6 checks across 6 hops");
        assert_eq!(v.steps.len(), 6);
    }

    #[test]
    fn the_first_failure_is_blamed_with_its_own_headline() {
        for (id, headline) in [
            ("host", "This machine has no working network connection"),
            ("gateway", "Can't reach the router"),
            ("isp", "Your internet provider's connection is down"),
            ("internet", "The internet is unreachable"),
            ("web", "Web traffic is being intercepted"),
        ] {
            let v = diagnose(&fail_at(id));
            assert_eq!(v.severity, Severity::Down, "{id}");
            assert_eq!(v.blame.as_deref(), Some(id));
            assert_eq!(v.headline, headline);
            assert!(v.fix_hint.is_some(), "{id} has a next step");
        }
    }

    #[test]
    fn dns_failure_reads_differently_when_the_internet_answers_by_ip() {
        let v = diagnose(&fail_at("dns"));
        assert_eq!(v.headline, "DNS is failing — the internet is reachable by IP");
        // When the internet hop isn't known to pass, don't claim it does.
        let v = diagnose(&with(fail_at("dns"), "internet", |h| h.status = Status::Warn));
        assert_eq!(v.headline, "DNS is failing");
    }

    #[test]
    fn a_failure_outranks_an_earlier_warning() {
        let hops = with(fail_at("isp"), "gateway", |h| h.status = Status::Warn);
        let v = diagnose(&hops);
        assert_eq!((v.severity, v.blame.as_deref()), (Severity::Down, Some("isp")));
    }

    #[test]
    fn confidence_follows_the_blamed_hops_check_count() {
        let one = diagnose(&fail_at("isp"));
        assert_eq!(one.confidence, Confidence::Medium);
        let check = |name: &str| Check { name: name.into(), status: Status::Fail, detail: String::new(), rtt_ms: None };
        let two = diagnose(&with(fail_at("isp"), "isp", |h| h.checks = vec![check("TTL 2"), check("TTL 3")]));
        assert_eq!(two.confidence, Confidence::High);
    }

    #[test]
    fn the_first_degraded_hop_is_blamed_as_loss_or_slowness() {
        let lossy = with(path(), "gateway", |h| {
            h.status = Status::Warn;
            h.metrics.loss_pct = Some(9.0);
        });
        let v = diagnose(&lossy);
        assert_eq!((v.severity, v.confidence), (Severity::Degraded, Confidence::Medium));
        assert_eq!(v.blame.as_deref(), Some("gateway"));
        assert_eq!(v.headline, "Packet loss on your local network (Wi-Fi or LAN)");
        assert!(v.detail.contains("9%"), "{}", v.detail);

        let slow = with(path(), "isp", |h| {
            h.status = Status::Warn;
            h.summary = "33.8 ms, no loss".into();
        });
        let v = diagnose(&slow);
        assert_eq!(v.headline, "Slow at your internet provider");
        assert!(v.detail.contains("33.8 ms"), "{}", v.detail);
        assert!(v.fix_hint.as_deref().unwrap_or_default().contains("speed test"));
    }

    #[test]
    fn degraded_headlines_name_each_side() {
        for (id, side) in [("internet", "reaching the internet"), ("dns", "in name resolution"), ("web", "in web requests")] {
            let v = diagnose(&with(path(), id, |h| h.status = Status::Warn));
            assert_eq!(v.headline, format!("Slow {side}"));
        }
    }

    #[test]
    fn a_warning_about_a_host_port_explains_the_cable() {
        let hops = with(path(), "host", |h| {
            h.status = Status::Warn;
            h.checks = vec![Check { name: "eth0 speed".into(), status: Status::Warn, detail: "eth0 linked at 100 Mbps".into(), rtt_ms: None }];
        });
        let v = diagnose(&hops);
        assert_eq!(v.severity, Severity::Degraded);
        assert_eq!(v.headline, "eth0 linked at 100 Mbps");
        assert!(v.fix_hint.as_deref().unwrap_or_default().contains("cable"));
        // An unknown port check falls back to the generic wording.
        let hops = with(path(), "host", |h| {
            h.status = Status::Warn;
            h.checks = vec![Check { name: "eth0 mystery".into(), status: Status::Warn, detail: "?".into(), rtt_ms: None }];
        });
        assert_eq!(diagnose(&hops).headline, "Slow on your local network (Wi-Fi or LAN)");
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
            bottom_line: None,
            confidence_reason: None,
            steps: vec![],
            held_cycles: None,
        }
    }

    #[test]
    fn a_broken_path_keeps_the_headline() {
        let down = Verdict { severity: Severity::Down, headline: "Can't reach the router".into(), blame: Some("gateway".into()), ..healthy() };
        let v = with_targets(down, &[target("NAS", true, Status::Fail)]);
        assert_eq!((v.severity, v.blame.as_deref()), (Severity::Down, Some("gateway")));
    }

    #[test]
    fn a_failing_target_outranks_a_slow_one_and_counts_the_rest() {
        let v = with_targets(healthy(), &[target("Printer", true, Status::Warn), target("NAS", true, Status::Fail), target("Pi-hole", true, Status::Warn)]);
        assert_eq!(v.blame.as_deref(), Some("target:nas"));
        assert_eq!(v.headline, "NAS is unreachable (and 2 more targets)");
        let v = with_targets(healthy(), &[target("Printer", true, Status::Warn)]);
        assert_eq!(v.headline, "Printer is slow");
        assert_eq!(v.steps.last().map(|s| s.hop.as_str()), Some("target:printer"));
        assert!(v.bottom_line.as_deref().unwrap_or_default().contains("Printer itself"));
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
