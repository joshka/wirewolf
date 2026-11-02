//! Words the verdict for someone who doesn't know networking: one step per hop saying what was
//! checked, what came back, the limit, and what that means, plus a bottom line and why the
//! confidence is what it is.
//!
//! [`diagnose`](crate::diagnose::diagnose) blames the first failing hop, or else the first
//! warning one. The steps follow that rule: the blamed hop is the cause, anything wrong after it
//! is collateral, and a hop that passed despite a number over its limit is excused.

use crate::config::{Limits, Thresholds};
use crate::model::{Confidence, Hop, HopKind, Severity, Status, Step, StepRole, Target, Verdict};

/// Fills the explanation fields of a path verdict.
pub fn explain(verdict: &mut Verdict, hops: &[Hop], limits: &Thresholds) {
    if verdict.severity == Severity::Unknown {
        return;
    }
    let blamed = verdict.blame.as_deref().and_then(|b| hops.iter().position(|h| h.id == b));
    verdict.steps = hops
        .iter()
        .enumerate()
        .map(|(i, hop)| {
            let inherits = blamed.is_some_and(|b| i > b) && hop.metrics.loss_pct.is_some_and(|l| l > 0.0);
            step(hop, role(hop, i, blamed), inherits, limits)
        })
        .collect();
    let cause = blamed.map(|i| &hops[i]);
    verdict.bottom_line = Some(bottom_line(verdict.severity, cause).into());
    verdict.confidence_reason = Some(confidence_reason(verdict, cause));
}

/// Adds the step for a critical target that decided an otherwise healthy verdict.
pub fn explain_target(verdict: &mut Verdict, target: &Target) {
    for s in &mut verdict.steps {
        s.means = format!("{} The path is fine, so this isn't why {} fails.", s.means, target.name);
    }
    let failed = target.status == Status::Fail;
    verdict.steps.push(Step {
        hop: target.id.clone(),
        status: target.status,
        role: StepRole::Cause,
        question: format!("Is {} answering?", target.name),
        subject: Some(format!("{} · marked critical", target.check)),
        did: format!("Checked it the way the config says ({})", target.kind),
        saw: sentence(&target.detail),
        limit: (!failed).then(|| "slower or lossier than the target limits".into()),
        means: if failed {
            format!("{} itself isn't answering. Because it's marked critical, that degrades the verdict.", target.name)
        } else {
            format!("{} answers, but poorly. Because it's marked critical, that degrades the verdict.", target.name)
        },
    });
    verdict.bottom_line = Some(format!("Your internet connection is fine; the problem is {} itself, not the network path to it.", target.name));
    verdict.confidence_reason = Some(format!("Medium. Every path check passes, so the fault is local to {}; wirewolf can't see inside it.", target.name));
}

fn role(hop: &Hop, i: usize, blamed: Option<usize>) -> StepRole {
    match hop.status {
        Status::Pending | Status::NotApplicable | Status::Skip => StepRole::Skipped,
        _ if Some(i) == blamed => StepRole::Cause,
        Status::Fail | Status::Warn if blamed.is_some_and(|b| i > b) => StepRole::Collateral,
        Status::Pass if excused(hop) => StepRole::Excused,
        _ => StepRole::Pass,
    }
}

/// Passed although a raw number says otherwise: a router that answers pings slowly or not at
/// all while traffic through it flows, or an ISP router that doesn't reply to TTL probes.
fn excused(hop: &Hop) -> bool {
    match hop.kind {
        HopKind::Gateway => hop.summary.contains("traffic passes through") || hop.evidence.iter().any(|e| e.key == "note" && e.value.contains("deprioritizes")),
        HopKind::Isp => hop.summary.contains("silent") || hop.summary.starts_with("inferred"),
        _ => false,
    }
}

fn evidence<'a>(hop: &'a Hop, key: &str) -> Option<&'a str> {
    hop.evidence.iter().find(|e| e.key == key).map(|e| e.value.as_str())
}

/// The probe count from a method such as "ICMP echo × 20".
fn count(method: Option<&str>) -> Option<usize> {
    method?.rsplit_once('×')?.1.split_whitespace().next()?.parse().ok()
}

fn ms(v: f64) -> String {
    if v < 10.0 { format!("{v:.1} ms") } else { format!("{v:.0} ms") }
}

fn sentence(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

fn path_limit(l: &Limits) -> String {
    format!("loss under {:.0}%, under {:.0} ms, jitter under {:.0} ms", l.loss_warn_pct, l.rtt_warn_ms, l.jitter_warn_ms)
}

/// "3 of 20 lost (15%); replies in 6.2 ms", from the hop's metrics and probe count.
fn replies(hop: &Hop, n: Option<usize>, noun: &str) -> String {
    let m = &hop.metrics;
    let rtt = m.rtt_ms.map(|r| format!(", {}", ms(r))).unwrap_or_default();
    match (m.loss_pct, n) {
        (Some(l), Some(n)) if l >= 100.0 => format!("None of {n} {noun} answered"),
        (Some(l), None) if l >= 100.0 => format!("No {noun} answered"),
        (Some(l), Some(n)) if l > 0.0 => {
            let lost = (l / 100.0 * n as f64).round() as usize;
            format!("{lost} of {n} {noun} lost ({l:.0}%){rtt}")
        }
        (Some(_), Some(n)) => format!("{n} of {n} answered{rtt}"),
        _ => sentence(&hop.summary),
    }
}

fn step(hop: &Hop, role: StepRole, inherits: bool, t: &Thresholds) -> Step {
    let ok = hop.status == Status::Pass;
    let collateral = role == StepRole::Collateral;
    let method = evidence(hop, "method");
    let (question, did, saw, limit, means): (&str, String, String, Option<String>, String) = match hop.kind {
        HopKind::Host => (
            "Is this machine connected?",
            format!("Read {}'s link state and addresses", evidence(hop, "interface").unwrap_or("the network interface")),
            hop.summary.clone(),
            None,
            match hop.status {
                Status::Pass => "This machine has a working network setup, so the problem isn't a missing address or cable.".into(),
                Status::Warn => "The connection works, but this machine's own port or cable has a problem that slows everything it sends.".into(),
                _ => "This machine has no usable network connection, so nothing further could be tested.".into(),
            },
        ),
        HopKind::Gateway => {
            let n = count(method);
            let tcp = method.is_some_and(|m| m.starts_with("TCP"));
            let did = match n {
                Some(n) if tcp => format!("Opened {n} connections to the router (pings aren't available here)"),
                Some(n) => format!("Sent {n} pings to the router"),
                None => "Tried to ping the router".into(),
            };
            let means = match role {
                StepRole::Excused if hop.summary.contains("traffic passes through") => "The router ignores pings, but traffic through it reaches the internet, so it's working.".into(),
                StepRole::Excused => "The router answers pings slowly, but hops beyond it answer faster, so it just deprioritizes pings. It isn't slowing your traffic.".into(),
                StepRole::Cause if hop.status == Status::Fail => "Nothing answers from your router, so nothing past it can be reached. Check it's powered on and that this machine's Wi-Fi or cable is connected.".into(),
                StepRole::Cause if hop.metrics.loss_pct.is_some_and(|l| l >= t.gateway.loss_warn_pct) => "This is the first check that went wrong, so it's the cause. Lost pings this close to you usually mean weak Wi-Fi or interference.".into(),
                StepRole::Cause => "This is the first check that went wrong, so it's the cause. A slow router usually means weak Wi-Fi or something on your network using the whole connection.".into(),
                _ if ok => "Your Wi-Fi or cable and your router are working.".into(),
                _ => "Your router is also struggling, but the bigger problem is further on.".into(),
            };
            ("Can it reach your router?", did, replies(hop, n, "pings"), Some(path_limit(&t.gateway)), means)
        }
        HopKind::Isp => {
            let n = count(method);
            let inferred = hop.summary.starts_with("inferred");
            let did = if inferred {
                "Couldn't probe it directly (that needs raw sockets), so judged it from whether the internet answers".into()
            } else if method == Some("ICMP TTL sweep") {
                "Traced the route one step at a time".into()
            } else {
                format!("Sent {} pings that expire one step past your router", n.map_or("a few".into(), |n| n.to_string()))
            };
            let saw = if hop.metrics.loss_pct.is_some() { replies(hop, n, "pings") } else { sentence(&hop.summary) };
            let means = match role {
                StepRole::Excused if inferred => "Traffic gets through your provider, so it's working.".into(),
                StepRole::Excused => "Plenty of ISP routers don't answer these pings. Traffic gets through, so your provider is working.".into(),
                StepRole::Cause if hop.status == Status::Fail => "Your router answers but nothing past it does, so the break is at your provider, right after your router.".into(),
                StepRole::Cause => "Packets are lost at your provider, and the internet checks lose them too, so it's real and not just a router ignoring pings.".into(),
                StepRole::Collateral => "Expected: this is past the problem, so it fails too.".into(),
                _ => "Your provider's equipment is answering. These routers often drop pings on purpose, so loss here only counts if the internet checks lose packets too.".into(),
            };
            let limit = if hop.status == Status::Fail {
                "any reply counts as up".into()
            } else {
                format!("loss over {:.0}% counts only if the internet also loses packets; no latency limit", t.isp.loss_warn_pct)
            };
            ("Does your provider's first router answer?", did, saw, Some(limit), means)
        }
        HopKind::Internet => {
            let n = count(method);
            let sites = hop.target.as_deref().map_or(0, |t| t.split(" · ").count());
            let did = match n {
                Some(n) => format!("Opened {n} connections to each of {sites} well-known services"),
                None => "Opened connections to well-known services".into(),
            };
            let saw = match (hop.status, hop.metrics.loss_pct, hop.metrics.rtt_ms) {
                (Status::Fail, ..) => "No connection completed".into(),
                (_, Some(l), Some(r)) if l > 0.0 => format!("{l:.0}% of connections failed; {} when they worked", ms(r)),
                (_, _, Some(r)) => format!("All connected; fastest {}", ms(r)),
                _ => sentence(&hop.summary),
            };
            let means = match role {
                StepRole::Collateral => "Also wrong, but this traffic passes through the problem above first, so it inherits it. Not a separate fault.".into(),
                StepRole::Cause if hop.status == Status::Fail => "Your router answers, but no internet service accepts a connection. Usually the modem or your provider.".into(),
                StepRole::Cause if hop.summary.contains("broken") => "One kind of address (IPv4 or IPv6) doesn't work while the other does. Most sites still load, some won't.".into(),
                StepRole::Cause => "Everything inside your home is fine, but the internet is slow or lossy past your router. If it lasts, it's likely your provider.".into(),
                _ if inherits => "Reachable. The loss here comes from the problem above, which this traffic passes through.".into(),
                _ => "The internet is reachable and responsive.".into(),
            };
            ("Can it reach the internet?", did, saw, Some(path_limit(&t.internet)), means)
        }
        HopKind::Dns => {
            let resolvers = evidence(hop, "resolvers").unwrap_or("the system resolver");
            let means = match role {
                StepRole::Collateral => "Expected: lookups need the internet, which is past the problem.".into(),
                StepRole::Cause if hop.status == Status::Fail => "The connection works, but names don't resolve, so websites look down. Fixing DNS fixes it.".into(),
                StepRole::Cause if hop.summary.contains("unexpected") => "Lookups return the wrong address, which usually means a filter or a hijacking resolver.".into(),
                StepRole::Cause => "Name lookups are slow or partly failing, which makes every new site slow to start.".into(),
                _ => "Name lookups work and return the expected answer.".into(),
            };
            ("Do website names resolve?", format!("Looked up one.one.one.one on {resolvers} and checked the answer"), sentence(&hop.summary), Some(format!("under {:.0} ms, with the expected answer", t.dns.rtt_warn_ms)), means)
        }
        HopKind::Web => {
            let means = match role {
                StepRole::Collateral => "Expected: pages need everything above to work.".into(),
                StepRole::Cause if hop.summary.contains("login") || hop.summary.contains("intercepted") => "Something is answering in place of the real site, usually a Wi-Fi login page or a filtering proxy.".into(),
                StepRole::Cause => "Connections work, but web requests fail.".into(),
                _ => "No login page or filter is in the way, and secure connections are trusted.".into(),
            };
            let host = hop.target.as_deref().unwrap_or("a test site");
            ("Do web pages load?", format!("Fetched a test page from {host} that should come back empty (HTTP 204), and made a secure connection"), sentence(&hop.summary), Some("HTTP 204 and a trusted certificate".into()), means)
        }
    };
    if role == StepRole::Skipped {
        return Step {
            hop: hop.id.clone(),
            status: hop.status,
            role,
            question: question.into(),
            subject: hop.target.clone(),
            did: "Not run this time".into(),
            saw: sentence(&hop.summary),
            limit: None,
            means: "Not tested: it can't work past the break, so it says nothing either way.".into(),
        };
    }
    Step {
        hop: hop.id.clone(),
        status: hop.status,
        role,
        question: question.into(),
        subject: hop.target.clone(),
        did,
        saw,
        limit,
        means: if collateral && means.is_empty() { "Expected: this is past the problem.".into() } else { means },
    }
}

fn bottom_line(severity: Severity, cause: Option<&Hop>) -> &'static str {
    let down = severity == Severity::Down;
    match cause.map(|h| h.kind) {
        None => "Everything from this machine to the internet is working.",
        Some(HopKind::Host) if down => "The problem is this machine's own connection. Nothing past it could be tested.",
        Some(HopKind::Host) => "The problem is this machine's own network port or cable, not your router or provider.",
        Some(HopKind::Gateway) if down => "The break is between this machine and your router, inside your home. Your internet provider isn't the cause.",
        Some(HopKind::Gateway) => "The problem starts between this machine and your router, inside your home. Your internet provider isn't the cause, so calling them won't help yet.",
        Some(HopKind::Isp) if down => "Your home network is fine; the break is at your internet provider, right after your router. This is the one to report to them.",
        Some(HopKind::Isp) => "Your home network is fine; the loss starts at your internet provider. Worth reporting to them if it lasts.",
        Some(HopKind::Internet) if down => "Your router answers, but nothing on the internet does. Restart the modem first; if that doesn't help, it's your provider.",
        Some(HopKind::Internet) => "Your home network is fine; the internet is slow or lossy past your router. If it lasts, report it to your provider.",
        Some(HopKind::Dns) => "The internet connection itself works; only name lookups are failing, so websites look down. Fix DNS; there's no need to call your provider.",
        Some(HopKind::Web) => "Your connection works, but web traffic is being intercepted, usually by a Wi-Fi login page or a filter.",
    }
}

/// The hop as the reader would call it.
fn name(hop: &Hop) -> &'static str {
    match hop.kind {
        HopKind::Host => "this machine",
        HopKind::Gateway => "your router",
        HopKind::Isp => "your provider's first router",
        HopKind::Internet => "the internet",
        HopKind::Dns => "name lookups",
        HopKind::Web => "web requests",
    }
}

fn confidence_reason(v: &Verdict, cause: Option<&Hop>) -> String {
    let level = match v.confidence {
        Confidence::High => "High",
        Confidence::Medium => "Medium",
        Confidence::Low => "Low",
    };
    let why = match (v.severity, cause) {
        (Severity::Healthy, _) => "every check passed".to_string(),
        (Severity::Down, Some(h)) if crate::diagnose::inferred(h) => format!("nothing past your router answers, but {} couldn't be probed directly (that needs a raw socket), so the break could be there or further out", name(h)),
        (Severity::Down, Some(h)) if h.checks.len() >= 2 => format!("{} separate checks on {} agree, and everything past it fails the same way", h.checks.len(), name(h)),
        (Severity::Down, Some(h)) => format!("one check on {} failed and nothing past it could be reached, but there's only that one check to go on", name(h)),
        (_, Some(h)) => format!("there's one clear signal at {} and the checks after it fit, but slowness and loss can come and go", name(h)),
        _ => "only some checks have run".to_string(),
    };
    format!("{level}: {why}.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnose::diagnose_with;
    use crate::model::{Evidence, HopMetrics};

    fn hop(id: &str, kind: HopKind, status: Status, summary: &str, loss: Option<f64>, method: &str) -> Hop {
        Hop {
            id: id.into(),
            kind,
            label: id.into(),
            target: Some("192.0.2.1".into()),
            status,
            summary: summary.into(),
            metrics: HopMetrics { rtt_ms: Some(6.2), jitter_ms: None, loss_pct: loss },
            evidence: vec![Evidence { key: "method".into(), value: method.into() }],
            checks: vec![],
            interfaces: vec![],
        }
    }

    fn path(gateway: (Status, Option<f64>), internet: Status) -> Vec<Hop> {
        vec![
            hop("host", HopKind::Host, Status::Pass, "en0 up, 192.0.2.24", None, ""),
            hop("gateway", HopKind::Gateway, gateway.0, "6.2 ms", gateway.1, "ICMP echo × 20"),
            hop("isp", HopKind::Isp, Status::Pass, "edge router silent — traffic flows", Some(100.0), "ICMP TTL 2 × 5"),
            hop("internet", HopKind::Internet, internet, "18 ms", Some(14.0), "TCP :443 handshake × 5 per target"),
        ]
    }

    #[test]
    fn wifi_loss_blames_gateway_and_marks_internet_collateral() {
        let v = diagnose_with(&path((Status::Warn, Some(15.0)), Status::Warn), &Thresholds::default());
        let roles: Vec<StepRole> = v.steps.iter().map(|s| s.role).collect();
        assert_eq!(roles, [StepRole::Pass, StepRole::Cause, StepRole::Excused, StepRole::Collateral]);
        assert_eq!(v.steps[1].did, "Sent 20 pings to the router");
        assert_eq!(v.steps[1].saw, "3 of 20 pings lost (15%), 6.2 ms");
        assert!(v.bottom_line.unwrap().contains("inside your home"));
    }

    #[test]
    fn healthy_has_steps_and_no_cause() {
        let v = diagnose_with(&path((Status::Pass, Some(0.0)), Status::Pass), &Thresholds::default());
        assert!(v.steps.iter().all(|s| s.role != StepRole::Cause));
        assert_eq!(v.steps[1].saw, "20 of 20 answered, 6.2 ms");
        assert_eq!(v.confidence_reason.as_deref(), Some("High: every check passed."));
    }
}
