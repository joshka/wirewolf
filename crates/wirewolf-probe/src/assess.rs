//! Turns raw measurements into hops. Pure functions only, so the status rules are testable on
//! real-shaped input without touching the network.

use std::net::IpAddr;

use wirewolf_core::config::{Limits, Thresholds};
use wirewolf_core::model::{Check, Evidence, Hop, HopKind, HopMetrics, Status};

/// Everything one probe cycle measured.
#[derive(Debug, Clone, Default)]
pub struct Measurements {
    pub host: HostInfo,
    pub gateway: GatewayProbe,
    pub isp: IspProbe,
    pub internet: Vec<TcpTarget>,
    pub dns: Vec<DnsResult>,
    pub web: WebResult,
    /// The limits the hops are judged by.
    pub thresholds: Thresholds,
}

#[derive(Debug, Clone, Default)]
pub struct HostInfo {
    pub interface: Option<String>,
    pub up: bool,
    /// CIDR strings.
    pub ipv4: Vec<String>,
    /// Global IPv6 addresses only, CIDR strings.
    pub ipv6: Vec<String>,
    pub mtu: Option<u32>,
    pub gateway: Option<IpAddr>,
    pub link_mbps: Option<u64>,
    pub public_v4: Option<String>,
    pub public_v6: Option<String>,
    /// Running in a container on a bridge network, so the "gateway" is the bridge.
    pub container_bridge: bool,
    pub icmp: String,
    /// The largest packet that reached the internet with don't-fragment set, if measured.
    pub path_mtu: Option<PathMtu>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathMtu {
    pub mtu: u32,
    pub target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PingMethod {
    Icmp,
    Tcp,
    #[default]
    None,
}

#[derive(Debug, Clone, Default)]
pub struct GatewayProbe {
    pub method: PingMethod,
    /// One entry per probe: RTT in ms, or `None` if lost.
    pub rtts: Vec<Option<f64>>,
}

/// `(ttl, Some((router, rtt_ms)))`, or `None` if nothing answered at that TTL.
pub type TraceHop = (u32, Option<(IpAddr, f64)>);

#[derive(Debug, Clone, Default)]
pub struct IspProbe {
    /// False when this host can't see TTL-exceeded replies; the hop is then inferred.
    pub traceable: bool,
    /// Address of the first router past the gateway, if it answered.
    pub edge: Option<IpAddr>,
    pub rtts: Vec<Option<f64>>,
    /// A full sweep, run only when something upstream is wrong: `(ttl, Some((router, rtt_ms)))`.
    pub trace: Option<Vec<TraceHop>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    V4,
    V6,
}

#[derive(Debug, Clone)]
pub struct TcpTarget {
    pub name: String,
    pub family: Family,
    pub addr: String,
    pub outcome: TcpOutcome,
    /// ICMP echo RTTs to the same address, if ICMP pings ran.
    pub icmp: Option<Vec<Option<f64>>>,
}

#[derive(Debug, Clone)]
pub enum TcpOutcome {
    /// For example, no IPv6 on this host.
    NotApplicable(String),
    /// Handshake times in ms, `None` for each one that failed. `error` is the last failure.
    Attempts { rtts: Vec<Option<f64>>, error: Option<String> },
}

#[derive(Debug, Clone)]
pub struct DnsResult {
    pub name: String,
    pub server: String,
    pub system: bool,
    /// What the resolver is, if known, for example "Tailscale MagicDNS" or "Pi-hole".
    pub label: Option<String>,
    /// The headline query name. Empty means [`QUERY_NAME`].
    pub query: String,
    /// The headline query: the first configured name, type A.
    pub outcome: Result<DnsAnswer, String>,
    /// The other names and record types, for example `AAAA www.google.com`.
    pub extra: Vec<ExtraQuery>,
}

#[derive(Debug, Clone)]
pub struct ExtraQuery {
    /// For example `AAAA www.google.com`.
    pub query: String,
    /// The RTT, or why it failed. A name with no records of that type is a success.
    pub outcome: Result<f64, String>,
}

/// Names a resolver from its address alone. `gateway` is the default gateway.
pub fn known_resolver(ip: IpAddr, gateway: Option<IpAddr>) -> Option<&'static str> {
    let s = ip.to_string();
    Some(match s.as_str() {
        "100.100.100.100" | "fd7a:115c:a1e0::53" => "Tailscale MagicDNS",
        "1.1.1.1" | "1.0.0.1" | "2606:4700:4700::1111" | "2606:4700:4700::1001" => "Cloudflare",
        "1.1.1.2" | "1.0.0.2" | "1.1.1.3" | "1.0.0.3" => "Cloudflare for Families",
        "8.8.8.8" | "8.8.4.4" | "2001:4860:4860::8888" | "2001:4860:4860::8844" => "Google Public DNS",
        "9.9.9.9" | "149.112.112.112" | "2620:fe::fe" | "2620:fe::9" => "Quad9",
        "94.140.14.14" | "94.140.15.15" => "AdGuard DNS",
        "208.67.222.222" | "208.67.220.220" => "OpenDNS",
        _ if s.starts_with("45.90.28.") || s.starts_with("45.90.30.") => "NextDNS",
        _ if Some(ip) == gateway => "your router",
        _ => return None,
    })
}

/// Names DNS software from its `version.bind` CHAOS TXT answer.
pub fn resolver_software(version: &str) -> Option<String> {
    let v = version.to_ascii_lowercase();
    let name = if v.contains("pi-hole") {
        "Pi-hole"
    } else if v.contains("adguard") {
        "AdGuard Home"
    } else if v.contains("dnsmasq") {
        "dnsmasq"
    } else if v.contains("unbound") {
        "Unbound"
    } else if v.contains("powerdns") {
        "PowerDNS"
    } else if v.contains("bind") || v.starts_with('9') {
        "BIND"
    } else {
        return None;
    };
    Some(format!("{name} ({})", sanitize(version)))
}

#[derive(Debug, Clone)]
pub struct DnsAnswer {
    pub rtt_ms: f64,
    pub addrs: Vec<IpAddr>,
    /// The answer contains the address the query name is known to have.
    pub expected: bool,
}

#[derive(Debug, Clone, Default)]
pub struct WebResult {
    /// The captive-portal URL probed. Empty means [`PORTAL_URL`].
    pub portal_url: String,
    pub portal: Option<Result<HttpAnswer, String>>,
    pub tls: Option<Result<TlsAnswer, String>>,
    pub sites: Vec<SiteResult>,
}

#[derive(Debug, Clone)]
pub struct SiteResult {
    pub url: String,
    pub outcome: Result<HttpAnswer, String>,
}

#[derive(Debug, Clone)]
pub struct HttpAnswer {
    pub status: u16,
    pub location: Option<String>,
    pub rtt_ms: f64,
}

#[derive(Debug, Clone)]
pub struct TlsAnswer {
    pub rtt_ms: f64,
    pub version: String,
    pub host: String,
}

pub const PORTAL_URL: &str = "http://cp.cloudflare.com/generate_204";

// ---- stats ----

pub fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let mid = v.len() / 2;
    Some(if v.len().is_multiple_of(2) { (v[mid - 1] + v[mid]) / 2.0 } else { v[mid] })
}

/// Mean absolute difference between consecutive successful samples (RFC 3550 style, unsmoothed).
pub fn jitter(rtts: &[Option<f64>]) -> Option<f64> {
    let ok: Vec<f64> = rtts.iter().flatten().copied().collect();
    (ok.len() >= 2).then(|| ok.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (ok.len() - 1) as f64)
}

pub fn loss(rtts: &[Option<f64>]) -> Option<f64> {
    (!rtts.is_empty()).then(|| rtts.iter().filter(|r| r.is_none()).count() as f64 * 100.0 / rtts.len() as f64)
}

fn ok(rtts: &[Option<f64>]) -> Vec<f64> {
    rtts.iter().flatten().copied().collect()
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn metrics(rtts: &[Option<f64>]) -> HopMetrics {
    HopMetrics {
        rtt_ms: median(&ok(rtts)).map(round1),
        jitter_ms: jitter(rtts).map(round1),
        loss_pct: loss(rtts).map(round1),
    }
}

fn ev(key: &str, value: impl ToString) -> Evidence {
    Evidence { key: key.into(), value: value.to_string() }
}

fn check(name: &str, status: Status, detail: impl Into<String>, rtt_ms: Option<f64>) -> Check {
    Check { name: name.into(), status, detail: detail.into(), rtt_ms: rtt_ms.map(round1) }
}

pub fn summary(m: &HopMetrics) -> String {
    match (m.rtt_ms, m.loss_pct) {
        (Some(rtt), Some(loss)) if loss >= 0.5 => format!("{rtt:.1} ms, {loss:.0}% loss"),
        (Some(rtt), _) => format!("{rtt:.1} ms, no loss"),
        (None, Some(loss)) if loss > 0.0 => format!("{loss:.0}% loss"),
        _ => "no response".into(),
    }
}

fn worst(statuses: impl IntoIterator<Item = Status>) -> Status {
    let rank = |s: Status| match s {
        Status::Fail => 4,
        Status::Warn => 3,
        Status::Pass => 2,
        Status::Skip => 1,
        Status::Pending => 0,
        Status::NotApplicable => -1,
    };
    statuses.into_iter().max_by_key(|s| rank(*s)).unwrap_or(Status::NotApplicable)
}

// ---- hops ----

/// Builds the path, in order, and marks failures downstream of the first hard break as collateral.
pub fn hops(m: &Measurements) -> Vec<Hop> {
    let internet = internet(&m.internet, &m.thresholds.internet);
    let internet_ok = matches!(internet.status, Status::Pass | Status::Warn);
    // The fastest reply from beyond the gateway. Traffic through the router can't be faster than
    // the router itself, so if this beats the gateway's RTT, the router is just slow to answer
    // probes.
    let downstream = [median(&ok(&m.isp.rtts)), internet.metrics.rtt_ms].into_iter().flatten().reduce(f64::min);
    let t = &m.thresholds;
    let mut hops = vec![host(&m.host), gateway(&m.gateway, &m.host, internet_ok, downstream, &t.gateway), isp(&m.isp, internet_ok, &internet, t), internet, dns(&m.dns, &t.dns), web(&m.web)];
    // Hard breaks on the path itself (host → internet) make later failures collateral.
    if let Some(broken) = hops.iter().take(4).position(|h| h.status == Status::Fail) {
        let label = hops[broken].label.clone();
        for hop in &mut hops[broken + 1..] {
            if matches!(hop.status, Status::Fail | Status::Warn) {
                hop.status = Status::Skip;
                hop.summary = format!("collateral — {label} is down");
            }
        }
    }
    // Names failing makes the web check collateral too.
    if hops[4].status == Status::Fail && hops[5].status == Status::Fail {
        hops[5].status = Status::Skip;
        hops[5].summary = "collateral — no name resolution".into();
    }
    hops
}

pub fn host(h: &HostInfo) -> Hop {
    let iface = h.interface.clone().unwrap_or_else(|| "none".into());
    let has_addr = !h.ipv4.is_empty() || !h.ipv6.is_empty();
    let status = if h.interface.is_none() || !h.up || !has_addr { Status::Fail } else { Status::Pass };
    let first = h.ipv4.first().or(h.ipv6.first()).cloned();
    let mut evidence = vec![ev("interface", &iface)];
    evidence.extend(h.ipv4.iter().map(|a| ev("ipv4", a)));
    evidence.extend(h.ipv6.iter().map(|a| ev("ipv6", a)));
    if let Some(mtu) = h.mtu {
        evidence.push(ev("mtu", mtu));
    }
    if let Some(ip) = &h.public_v4 {
        evidence.push(ev("public ipv4", ip));
    }
    if let Some(ip) = &h.public_v6 {
        evidence.push(ev("public ipv6", ip));
    }
    evidence.push(ev("cgnat", cgnat(h)));
    evidence.push(ev("icmp", &h.icmp));
    if h.container_bridge {
        evidence.push(ev("note", "container on a bridge network: the gateway is the container bridge, not your router (use --network host)"));
    }
    let link = h.link_mbps.map(|m| if m >= 1000 { format!(", {} Gbps", m / 1000) } else { format!(", {m} Mbps") }).unwrap_or_default();
    let mut checks = vec![check(
        "Interface up",
        if h.interface.is_some() && h.up { Status::Pass } else { Status::Fail },
        if h.interface.is_some() { format!("{iface}{link}") } else { "no default route".into() },
        None,
    )];
    checks.push(match h.ipv4.first() {
        Some(a) => check("IPv4 address", Status::Pass, a, None),
        None => check("IPv4 address", if has_addr { Status::Warn } else { Status::Fail }, "none", None),
    });
    checks.push(match h.ipv6.first() {
        Some(a) => check("IPv6 address", Status::Pass, a, None),
        None => check("IPv6 address", Status::NotApplicable, "no global IPv6 address", None),
    });
    if let Some(p) = &h.path_mtu {
        evidence.push(ev("path mtu", format!("{} (to {})", p.mtu, p.target)));
        // 1280 is the IPv6 minimum; below it, something on the path is badly configured.
        let status = if p.mtu < 1280 { Status::Warn } else { Status::Pass };
        let note = match (h.mtu, p.mtu) {
            (Some(local), path) if path < local => format!("{path} bytes to {}; smaller than the local {local} (PPPoE or a tunnel)", p.target),
            (_, path) => format!("{path} bytes to {}", p.target),
        };
        checks.push(check("Path MTU", status, note, None));
    }
    Hop {
        id: "host".into(),
        kind: HopKind::Host,
        label: "This host".into(),
        target: first.as_ref().map(|a| format!("{iface} · {a}")),
        status,
        summary: match (&first, status) {
            (Some(a), Status::Pass) => format!("{iface} up, {}", a.split('/').next().unwrap_or(a)),
            _ if h.interface.is_none() => "no default route".into(),
            _ => format!("{iface} has no usable address"),
        },
        metrics: HopMetrics::default(),
        evidence,
        checks,
    }
}

fn cgnat(h: &HostInfo) -> String {
    let in_shared = |ip: &str| ip.split('/').next().and_then(|a| a.parse::<std::net::Ipv4Addr>().ok()).is_some_and(|a| a.octets()[0] == 100 && (64..128).contains(&a.octets()[1]));
    if h.ipv4.iter().any(|a| in_shared(a)) || h.public_v4.as_deref().is_some_and(in_shared) {
        "likely (100.64.0.0/10 address)".into()
    } else {
        "not detected".into()
    }
}

pub fn gateway(g: &GatewayProbe, h: &HostInfo, internet_ok: bool, downstream_rtt: Option<f64>, limits: &Limits) -> Hop {
    let target = h.gateway.map(|g| g.to_string());
    let base = |status, summary: String, metrics, evidence: Vec<Evidence>, checks| Hop {
        id: "gateway".into(),
        kind: HopKind::Gateway,
        label: "Gateway".into(),
        target: target.clone(),
        status,
        summary,
        metrics,
        evidence,
        checks,
    };
    let Some(addr) = h.gateway else {
        return base(Status::Fail, "no default gateway".into(), HopMetrics::default(), vec![], vec![check("Default route", Status::Fail, "no default gateway", None)]);
    };
    let method = match g.method {
        PingMethod::Icmp => format!("ICMP echo × {}", g.rtts.len()),
        PingMethod::Tcp => format!("TCP connect × {} (ICMP unavailable)", g.rtts.len()),
        PingMethod::None => "none".into(),
    };
    let mut evidence = vec![ev("address", addr), ev("method", &method)];
    let m = metrics(&g.rtts);
    let lost = m.loss_pct.unwrap_or(100.0);
    if g.method == PingMethod::None || lost >= 100.0 {
        // Some routers drop probes. If traffic flows through, the gateway is fine.
        if internet_ok {
            let detail = if g.method == PingMethod::None { "can't probe without ICMP" } else { "doesn't answer probes" };
            return base(
                Status::Pass,
                format!("{detail}; traffic passes through"),
                HopMetrics::default(),
                evidence,
                vec![check(&method_name(g.method), Status::Pass, format!("{detail}, but the internet is reachable"), None)],
            );
        }
        return base(Status::Fail, "no response".into(), m, evidence, vec![check(&method_name(g.method), Status::Fail, format!("{} sent, all lost", g.rtts.len()), None)]);
    }
    // A router that's slow to answer probes is excused only when traffic through it is fast. If
    // the hops beyond are slow too, the gateway's latency is real and everything inherits it.
    let deprioritized = matches!((m.rtt_ms, downstream_rtt), (Some(gw), Some(down)) if down < gw && down <= limits.rtt_warn_ms);
    if deprioritized {
        evidence.push(ev("note", format!("hops beyond answer faster ({:.1} ms), so the router deprioritizes probes; its RTT isn't the bottleneck", downstream_rtt.unwrap_or_default())));
    }
    let slow = !deprioritized
        && (m.rtt_ms.is_some_and(|r| r > limits.rtt_warn_ms) || m.jitter_ms.is_some_and(|j| j > limits.jitter_warn_ms));
    let degraded = lost >= limits.loss_warn_pct || slow;
    let status = if degraded { Status::Warn } else { Status::Pass };
    let detail = format!("{} sent, {:.0}% lost", g.rtts.len(), lost);
    base(status, summary(&m), m, evidence, vec![check(&method_name(g.method), status, detail, m.rtt_ms)])
}

fn method_name(m: PingMethod) -> String {
    match m {
        PingMethod::Icmp => "ICMP echo".into(),
        PingMethod::Tcp => "TCP connect".into(),
        PingMethod::None => "Probe".into(),
    }
}

pub fn isp(p: &IspProbe, internet_ok: bool, internet: &Hop, limits: &Thresholds) -> Hop {
    let mut evidence = Vec::new();
    if let Some(trace) = &p.trace {
        evidence.push(ev("method", "ICMP TTL sweep"));
        for (ttl, hop) in trace {
            evidence.push(ev(&format!("hop {ttl}"), match hop {
                Some((addr, rtt)) => format!("{addr} · {rtt:.1} ms"),
                None => "* (no reply)".into(),
            }));
        }
    }
    let inferred = |status, summary: &str, detail: &str, evidence: Vec<Evidence>| Hop {
        id: "isp".into(),
        kind: HopKind::Isp,
        label: "ISP".into(),
        target: p.edge.map(|a| a.to_string()),
        status,
        summary: summary.into(),
        metrics: HopMetrics::default(),
        evidence,
        checks: vec![check("First ISP hop", status, detail, None)],
    };
    if !p.traceable {
        evidence.insert(0, ev("method", "inferred from internet reachability (TTL probes need a raw socket)"));
        return if internet_ok {
            inferred(Status::Pass, "inferred — internet reachable", "not probed directly; traffic flows", evidence)
        } else {
            inferred(Status::Fail, "inferred — nothing past the gateway answers", "not probed directly; no internet host answers", evidence)
        };
    }
    let m = metrics(&p.rtts);
    let lost = m.loss_pct.unwrap_or(100.0);
    if p.trace.is_none() {
        evidence.push(ev("method", format!("ICMP TTL 2 × {}", p.rtts.len())));
    }
    if let Some(edge) = p.edge {
        evidence.push(ev("edge router", edge));
    }
    if lost >= 100.0 {
        return if internet_ok {
            // Plenty of ISP routers don't send TTL-exceeded replies.
            inferred(Status::Pass, "edge router silent — traffic flows", "no TTL-exceeded replies, but the internet is reachable", evidence)
        } else {
            inferred(Status::Fail, "no reply past the gateway", &format!("no reply from hop 2 within {}s", 1), evidence)
        };
    }
    // TTL-exceeded replies are rate-limited, so loss here alone isn't trusted.
    let internet_lossy = internet.metrics.loss_pct.is_some_and(|l| l >= limits.internet.loss_warn_pct);
    let status = if lost >= limits.isp.loss_warn_pct && internet_lossy { Status::Warn } else { Status::Pass };
    Hop {
        id: "isp".into(),
        kind: HopKind::Isp,
        label: "ISP".into(),
        target: p.edge.map(|a| a.to_string()),
        status,
        summary: summary(&m),
        metrics: m,
        evidence,
        checks: vec![check("First ISP hop", status, format!("{} answers", p.edge.map(|a| a.to_string()).unwrap_or_else(|| "edge".into())), m.rtt_ms)],
    }
}

pub fn internet(targets: &[TcpTarget], limits: &Limits) -> Hop {
    let mut checks = Vec::new();
    let mut all: Vec<Option<f64>> = Vec::new();
    let mut family_ok = [None::<bool>; 2];
    let mut jitters = Vec::new();
    for t in targets {
        let name = format!("{} {}", t.name, if t.family == Family::V4 { "v4" } else { "v6" });
        let fam = t.family as usize;
        match &t.outcome {
            TcpOutcome::NotApplicable(why) => checks.push(check(&name, Status::NotApplicable, why.as_str(), None)),
            TcpOutcome::Attempts { rtts, error } => {
                all.extend(rtts);
                let m = metrics(rtts);
                jitters.extend(m.jitter_ms);
                let lost = m.loss_pct.unwrap_or(100.0);
                let any = lost < 100.0;
                *family_ok[fam].get_or_insert(false) |= any;
                let status = if !any {
                    Status::Fail
                } else if lost >= limits.loss_warn_pct {
                    Status::Warn
                } else {
                    Status::Pass
                };
                let detail = if any {
                    format!("TCP to {} — {}/{} handshakes", t.addr, rtts.iter().flatten().count(), rtts.len())
                } else {
                    format!("{} — {}", t.addr, error.as_deref().unwrap_or("no handshake completed"))
                };
                checks.push(check(&name, status, detail, m.rtt_ms));
            }
        }
    }
    // ICMP is informational: some networks drop it, so it never changes the hop's status.
    let mut pings = Vec::new();
    for t in targets {
        let Some(rtts) = &t.icmp else { continue };
        let name = format!("{} ICMP", t.name);
        let m = metrics(rtts);
        let ip = t.addr.rsplit_once(':').map_or(t.addr.as_str(), |(ip, _)| ip);
        pings.push(match m.loss_pct {
            Some(l) if l < 100.0 => check(&name, Status::Pass, format!("echo to {ip} — {}", summary(&m)), m.rtt_ms),
            _ => check(&name, Status::NotApplicable, format!("no echo replies from {ip} (ICMP filtered?)"), None),
        });
    }
    let best = checks.iter().filter_map(|c| c.rtt_ms).fold(None, |a: Option<f64>, r| Some(a.map_or(r, |a| a.min(r))));
    let m = HopMetrics {
        rtt_ms: best,
        jitter_ms: (!jitters.is_empty()).then(|| round1(jitters.iter().sum::<f64>() / jitters.len() as f64)),
        loss_pct: loss(&all).map(round1),
    };
    checks.extend(pings);
    let any_ok = family_ok.contains(&Some(true));
    let broken_family = family_ok.iter().position(|f| *f == Some(false));
    let status = if checks.iter().all(|c| c.status == Status::NotApplicable) {
        Status::NotApplicable
    } else if !any_ok {
        Status::Fail
    } else if broken_family.is_some()
        || m.loss_pct.is_some_and(|l| l >= limits.loss_warn_pct)
        || m.rtt_ms.is_some_and(|r| r > limits.rtt_warn_ms)
        || m.jitter_ms.is_some_and(|j| j > limits.jitter_warn_ms)
    {
        Status::Warn
    } else {
        Status::Pass
    };
    let summary = match broken_family {
        Some(0) if any_ok => format!("IPv4 broken, IPv6 works ({})", summary(&m)),
        Some(1) if any_ok => format!("IPv6 broken, IPv4 works ({})", summary(&m)),
        _ if !any_ok => "no internet host accepts connections".into(),
        _ => summary(&m),
    };
    let per = targets.iter().find_map(|t| match &t.outcome {
        TcpOutcome::Attempts { rtts, .. } => Some(rtts.len()),
        _ => None,
    });
    let families = match (family_ok[0].is_some(), family_ok[1].is_some()) {
        (true, true) => "IPv4 + IPv6",
        (true, false) => "IPv4 only",
        (false, true) => "IPv6 only",
        _ => "none",
    };
    let port = targets.first().and_then(|t| t.addr.rsplit_once(':')).map_or("443", |(_, p)| p);
    let mut names: Vec<&str> = targets.iter().map(|t| t.name.as_str()).collect();
    names.dedup();
    Hop {
        id: "internet".into(),
        kind: HopKind::Internet,
        label: "Internet".into(),
        target: Some(names.join(" · ")),
        status,
        summary,
        metrics: m,
        evidence: vec![ev("method", format!("TCP :{port} handshake × {} per target", per.unwrap_or(0))), ev("families", families)],
        checks,
    }
}

pub fn dns(results: &[DnsResult], limits: &Limits) -> Hop {
    let mut checks = Vec::new();
    for r in results {
        let label = match (&r.label, r.system) {
            (Some(l), true) => format!("{} ({l}) — ", r.server),
            (None, true) => format!("{} — ", r.server),
            _ => String::new(),
        };
        let query = if r.query.is_empty() { QUERY_NAME } else { r.query.as_str() };
        let failed: Vec<String> = r.extra.iter().filter_map(|x| x.outcome.as_ref().err().map(|e| format!("{}: {e}", x.query))).collect();
        let others = match failed.first() {
            None if r.extra.is_empty() => String::new(),
            None => format!("; {} more lookups ok", r.extra.len()),
            Some(first) => format!("; {}/{} more lookups failed ({first})", failed.len(), r.extra.len()),
        };
        checks.push(match &r.outcome {
            Ok(a) if a.expected && failed.is_empty() => check(&r.name, Status::Pass, format!("{label}{query} → {}{others}", list(&a.addrs)), Some(a.rtt_ms)),
            Ok(a) if a.expected => check(&r.name, Status::Warn, format!("{label}{query} → {}{others}", list(&a.addrs)), Some(a.rtt_ms)),
            Ok(a) => check(&r.name, Status::Warn, format!("{label}unexpected answer {} for {query} (filtered or hijacked?){others}", list(&a.addrs)), Some(a.rtt_ms)),
            Err(e) => check(&r.name, Status::Fail, format!("{label}{e}"), None),
        });
    }
    let partial = results.iter().any(|r| r.system && r.outcome.is_ok() && r.extra.iter().any(|x| x.outcome.is_err()));
    let system: Vec<&DnsResult> = results.iter().filter(|r| r.system).collect();
    let public: Vec<&DnsResult> = results.iter().filter(|r| !r.system).collect();
    let sys_ok: Vec<&DnsAnswer> = system.iter().filter_map(|r| r.outcome.as_ref().ok()).collect();
    let pub_ok = public.iter().any(|r| r.outcome.is_ok());
    let sys_rtt = sys_ok.iter().map(|a| a.rtt_ms).fold(None, |m: Option<f64>, r| Some(m.map_or(r, |m| m.min(r))));
    let servers: Vec<String> = system.iter().map(|r| match &r.label {
        Some(l) => format!("{} ({l})", r.server),
        None => r.server.clone(),
    }).collect();
    let via = system.iter().find_map(|r| r.label.as_deref()).map_or("system resolver".to_string(), |l| l.to_string());
    let (status, summary) = if system.is_empty() {
        (Status::Fail, "no system resolver configured".to_string())
    } else if sys_ok.is_empty() && pub_ok {
        (Status::Fail, "system resolver failing; public resolvers fine".into())
    } else if sys_ok.is_empty() {
        (Status::Fail, "no resolver answers".into())
    } else if sys_ok.iter().any(|a| !a.expected) {
        (Status::Warn, "system resolver gives unexpected answers".into())
    } else if partial {
        (Status::Warn, format!("{:.1} ms via {via}, but some lookups fail", sys_rtt.unwrap()))
    } else if sys_rtt.is_some_and(|r| r > limits.rtt_warn_ms) {
        (Status::Warn, format!("slow: {:.1} ms via {via}", sys_rtt.unwrap()))
    } else if !public.is_empty() && !pub_ok {
        (Status::Warn, format!("{:.1} ms via {via}; public resolvers blocked", sys_rtt.unwrap()))
    } else {
        (Status::Pass, format!("{:.1} ms via {via}", sys_rtt.unwrap()))
    };
    let sys_loss = (!system.is_empty()).then(|| round1((system.len() - sys_ok.len()) as f64 * 100.0 / system.len() as f64));
    Hop {
        id: "dns".into(),
        kind: HopKind::Dns,
        label: "DNS".into(),
        target: (!servers.is_empty()).then(|| servers.join(", ")),
        status,
        summary,
        metrics: HopMetrics { rtt_ms: sys_rtt.map(round1), jitter_ms: None, loss_pct: sys_loss },
        evidence: vec![ev("resolvers", if servers.is_empty() { "none".into() } else { servers.join(", ") }), ev("queries", queries(results))],
        checks,
    }
}

/// Queried on every resolver; it must contain one of [`QUERY_EXPECT`].
pub const QUERY_NAME: &str = "one.one.one.one";
pub const QUERY_EXPECT: [IpAddr; 2] = [IpAddr::V4(std::net::Ipv4Addr::new(1, 1, 1, 1)), IpAddr::V4(std::net::Ipv4Addr::new(1, 0, 0, 1))];

/// The query names and types asked, for the evidence list.
fn queries(results: &[DnsResult]) -> String {
    let Some(r) = results.first() else { return format!("{QUERY_NAME} A") };
    let first = format!("{} A", if r.query.is_empty() { QUERY_NAME } else { &r.query });
    std::iter::once(first).chain(r.extra.iter().map(|x| x.query.clone())).collect::<Vec<_>>().join(", ")
}

fn list(addrs: &[IpAddr]) -> String {
    if addrs.is_empty() {
        return "no addresses".into();
    }
    addrs.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
}

pub fn web(w: &WebResult) -> Hop {
    let mut evidence = Vec::new();
    let portal_url = if w.portal_url.is_empty() { PORTAL_URL } else { w.portal_url.as_str() };
    let portal_host = host_of(portal_url);
    let portal = match &w.portal {
        None => check("Captive portal", Status::Pending, "not run", None),
        Some(Ok(a)) if a.status == 204 => {
            evidence.push(ev("http status", "204 No Content"));
            check("Captive portal", Status::Pass, format!("204 from {portal_host}"), Some(a.rtt_ms))
        }
        Some(Ok(a)) => {
            evidence.push(ev("http status", a.status));
            let to = a.location.as_deref().map(|l| format!(" → {}", sanitize(l))).unwrap_or_default();
            if let Some(l) = &a.location {
                evidence.push(ev("location", sanitize(l)));
            }
            check("Captive portal", Status::Fail, format!("expected 204, got {}{to}", a.status), Some(a.rtt_ms))
        }
        Some(Err(e)) => check("Captive portal", Status::Fail, format!("{portal_url}: {e}"), None),
    };
    let tls = match &w.tls {
        None => check("TLS handshake", Status::Pending, "not run", None),
        Some(Ok(t)) => {
            evidence.push(ev("tls", format!("{}, certificate trusted", t.version)));
            check("TLS handshake", Status::Pass, format!("{} to {}", t.version, t.host), Some(t.rtt_ms))
        }
        Some(Err(e)) => check("TLS handshake", Status::Fail, e.as_str(), None),
    };
    let sites: Vec<Check> = w
        .sites
        .iter()
        .map(|site| {
            let name = host_of(&site.url);
            match &site.outcome {
                Ok(a) if a.status < 500 => check(name, Status::Pass, format!("{} in {:.0} ms", a.status, a.rtt_ms), Some(a.rtt_ms)),
                Ok(a) => check(name, Status::Warn, format!("server error {}", a.status), Some(a.rtt_ms)),
                Err(e) => check(name, Status::Fail, e.as_str(), None),
            }
        })
        .collect();
    let sites_failed = sites.iter().filter(|c| c.status == Status::Fail).count();
    // One site down is that site's problem; all of them down is ours.
    let sites_status = match sites_failed {
        0 => worst(sites.iter().map(|c| c.status)),
        n if n == sites.len() => Status::Fail,
        _ => Status::Warn,
    };
    let status = worst([portal.status, tls.status, if sites.is_empty() { Status::NotApplicable } else { sites_status }]);
    let intercepted = matches!(&w.portal, Some(Ok(a)) if a.status != 204);
    let summary = match status {
        Status::Pass if !sites.is_empty() => format!("HTTP in {:.0} ms, TLS in {:.0} ms, {} sites up", portal.rtt_ms.unwrap_or_default(), tls.rtt_ms.unwrap_or_default(), sites.len()),
        Status::Pass => format!("HTTP in {:.0} ms, TLS in {:.0} ms", portal.rtt_ms.unwrap_or_default(), tls.rtt_ms.unwrap_or_default()),
        _ if intercepted && matches!(&w.portal, Some(Ok(a)) if a.location.is_some()) => "HTTP redirected to a login page".into(),
        _ if intercepted => "HTTP intercepted".into(),
        Status::Fail if portal.status == Status::Fail => "HTTP requests fail".into(),
        Status::Fail if tls.status == Status::Fail => "TLS handshake fails".into(),
        Status::Fail => "no HTTPS site answers".into(),
        Status::Warn if sites_failed > 0 => format!("{sites_failed} of {} sites unreachable", sites.len()),
        _ => "pending".into(),
    };
    Hop {
        id: "web".into(),
        kind: HopKind::Web,
        label: "Web".into(),
        target: Some(portal_host.into()),
        status,
        summary,
        metrics: HopMetrics { rtt_ms: portal.rtt_ms, jitter_ms: None, loss_pct: None },
        evidence,
        checks: [portal, tls].into_iter().chain(sites).collect(),
    }
}

/// The host part of a URL, for labels.
fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    authority.rsplit_once('@').map_or(authority, |(_, h)| h)
}

/// Keeps remote-supplied text short and printable.
pub fn sanitize(s: &str) -> String {
    let clean: String = s.chars().filter(|c| !c.is_control()).take(120).collect();
    if s.chars().count() > 120 { format!("{clean}…") } else { clean }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use wirewolf_core::diagnose::diagnose;
    use wirewolf_core::model::Severity;

    use super::*;

    fn gw() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))
    }

    fn host() -> HostInfo {
        HostInfo {
            interface: Some("en0".into()),
            up: true,
            ipv4: vec!["192.168.1.42/24".into()],
            ipv6: vec!["2001:db8::42/64".into()],
            mtu: Some(1500),
            gateway: Some(gw()),
            icmp: "datagram socket".into(),
            ..Default::default()
        }
    }

    fn rtts(base: f64, n: usize, lost: usize) -> Vec<Option<f64>> {
        (0..n).map(|i| (i >= lost).then_some(base + (i % 3) as f64 * 0.2)).collect()
    }

    fn tcp(name: &str, family: Family, r: Vec<Option<f64>>) -> TcpTarget {
        let error = r.iter().any(Option::is_none).then(|| "timed out after 1s".to_string());
        TcpTarget { name: name.into(), family, addr: "1.1.1.1:443".into(), outcome: TcpOutcome::Attempts { rtts: r, error }, icmp: None }
    }

    fn dns_ok(name: &str, system: bool, rtt: f64) -> DnsResult {
        DnsResult {
            name: name.into(),
            server: "192.168.1.1".into(),
            system,
            label: None,
            query: String::new(),
            outcome: Ok(DnsAnswer { rtt_ms: rtt, addrs: QUERY_EXPECT.to_vec(), expected: true }),
            extra: vec![],
        }
    }

    fn web_ok() -> WebResult {
        WebResult {
            portal: Some(Ok(HttpAnswer { status: 204, location: None, rtt_ms: 22.0 })),
            tls: Some(Ok(TlsAnswer { rtt_ms: 15.0, version: "TLS 1.3".into(), host: "www.cloudflare.com".into() })),
            ..Default::default()
        }
    }

    /// Shaped after a real cycle from a Mac on fiber.
    fn healthy() -> Measurements {
        Measurements {
            host: host(),
            gateway: GatewayProbe { method: PingMethod::Icmp, rtts: rtts(2.7, 20, 0) },
            isp: IspProbe { traceable: true, edge: Some("198.51.100.1".parse().unwrap()), rtts: rtts(8.5, 5, 0), trace: None },
            internet: vec![
                tcp("Cloudflare", Family::V4, rtts(6.1, 5, 0)),
                tcp("Cloudflare", Family::V6, rtts(6.8, 5, 0)),
                tcp("Google", Family::V4, rtts(9.0, 5, 0)),
                tcp("Google", Family::V6, rtts(9.4, 5, 0)),
            ],
            dns: vec![dns_ok("System resolver", true, 12.0), dns_ok("Cloudflare 1.1.1.1", false, 7.0)],
            web: web_ok(),
            thresholds: Thresholds::default(),
        }
    }

    #[test]
    fn healthy_cycle_is_healthy() {
        let hops = hops(&healthy());
        let ids: Vec<&str> = hops.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, ["host", "gateway", "isp", "internet", "dns", "web"]);
        assert!(hops.iter().all(|h| h.status == Status::Pass), "{hops:#?}");
        let v = diagnose(&hops);
        assert_eq!(v.severity, Severity::Healthy);
        assert_eq!(hops[3].metrics.rtt_ms, Some(6.3));
        assert_eq!(hops[3].metrics.loss_pct, Some(0.0));
    }

    #[test]
    fn isp_outage_blames_isp_and_marks_downstream_collateral() {
        let mut m = healthy();
        m.isp.rtts = vec![None; 5];
        for t in &mut m.internet {
            t.outcome = TcpOutcome::Attempts { rtts: vec![None; 5], error: Some("timed out after 1s".into()) };
        }
        m.dns = vec![
            DnsResult { outcome: Err("timed out".into()), ..dns_ok("System resolver", true, 0.0) },
            DnsResult { outcome: Err("timed out".into()), ..dns_ok("Cloudflare 1.1.1.1", false, 0.0) },
        ];
        m.web.portal = Some(Err("connect timed out".into()));
        let hops = hops(&m);
        assert_eq!(hops[2].status, Status::Fail);
        assert_eq!(hops[3].status, Status::Skip);
        assert_eq!(hops[4].status, Status::Skip);
        let v = diagnose(&hops);
        assert_eq!(v.severity, Severity::Down);
        assert_eq!(v.blame.as_deref(), Some("isp"));
    }

    #[test]
    fn without_ttl_probes_isp_is_inferred() {
        let mut m = healthy();
        m.isp = IspProbe { traceable: false, ..Default::default() };
        assert_eq!(hops(&m)[2].status, Status::Pass);
        for t in &mut m.internet {
            t.outcome = TcpOutcome::Attempts { rtts: vec![None; 5], error: None };
        }
        let v = diagnose(&hops(&m));
        assert_eq!(v.blame.as_deref(), Some("isp"));
    }

    #[test]
    fn silent_isp_router_is_fine_when_traffic_flows() {
        let mut m = healthy();
        m.isp.rtts = vec![None; 5];
        assert_eq!(hops(&m)[2].status, Status::Pass);
    }

    #[test]
    fn dns_down_with_internet_up() {
        let mut m = healthy();
        m.dns[0].outcome = Err("timed out after 3s".into());
        let hops = hops(&m);
        assert_eq!(hops[4].status, Status::Fail);
        let v = diagnose(&hops);
        assert_eq!(v.blame.as_deref(), Some("dns"));
        assert!(v.headline.contains("reachable by IP"), "{}", v.headline);
    }

    #[test]
    fn labels_known_resolvers() {
        let gw = Some(gw());
        assert_eq!(known_resolver("100.100.100.100".parse().unwrap(), gw), Some("Tailscale MagicDNS"));
        assert_eq!(known_resolver("192.168.1.1".parse().unwrap(), gw), Some("your router"));
        assert_eq!(known_resolver("192.168.1.5".parse().unwrap(), gw), None);
        assert_eq!(resolver_software("dnsmasq-pi-hole-v2.90+1").as_deref(), Some("Pi-hole (dnsmasq-pi-hole-v2.90+1)"));
        assert_eq!(resolver_software("dnsmasq-2.85").as_deref(), Some("dnsmasq (dnsmasq-2.85)"));
        let mut m = healthy();
        m.dns[0].server = "100.100.100.100".into();
        m.dns[0].label = Some("Tailscale MagicDNS".into());
        let hop = &hops(&m)[4];
        assert_eq!(hop.summary, "12.0 ms via Tailscale MagicDNS");
        assert_eq!(hop.target.as_deref(), Some("100.100.100.100 (Tailscale MagicDNS)"));
    }

    #[test]
    fn hijacked_dns_answer_warns() {
        let mut m = healthy();
        m.dns[0].outcome = Ok(DnsAnswer { rtt_ms: 5.0, addrs: vec!["10.0.0.1".parse().unwrap()], expected: false });
        assert_eq!(hops(&m)[4].status, Status::Warn);
    }

    #[test]
    fn captive_portal() {
        let mut m = healthy();
        m.web.portal = Some(Ok(HttpAnswer { status: 302, location: Some("http://login.hotspot.example/?mac=aa".into()), rtt_ms: 30.0 }));
        let hops = hops(&m);
        assert_eq!(hops[5].summary, "HTTP redirected to a login page");
        let v = diagnose(&hops);
        assert_eq!(v.blame.as_deref(), Some("web"));
        assert_eq!(v.severity, Severity::Down);
    }

    #[test]
    fn wifi_loss_degrades_gateway() {
        let mut m = healthy();
        m.gateway.rtts = rtts(6.0, 20, 3);
        let hops = hops(&m);
        assert_eq!(hops[1].status, Status::Warn);
        assert_eq!(hops[1].metrics.loss_pct, Some(15.0));
        let v = diagnose(&hops);
        assert_eq!(v.severity, Severity::Degraded);
        assert!(v.headline.contains("local network"), "{}", v.headline);
    }

    #[test]
    fn no_ipv6_is_not_a_failure() {
        let mut m = healthy();
        m.host.ipv6.clear();
        for t in m.internet.iter_mut().filter(|t| t.family == Family::V6) {
            t.outcome = TcpOutcome::NotApplicable("no IPv6 on this host".into());
        }
        let hops = hops(&m);
        assert_eq!(hops[0].status, Status::Pass);
        assert_eq!(hops[3].status, Status::Pass);
        assert_eq!(hops[3].evidence[1].value, "IPv4 only");
    }

    #[test]
    fn broken_ipv6_warns() {
        let mut m = healthy();
        for t in m.internet.iter_mut().filter(|t| t.family == Family::V6) {
            t.outcome = TcpOutcome::Attempts { rtts: vec![None; 5], error: Some("timed out".into()) };
        }
        let hop = &hops(&m)[3];
        assert_eq!(hop.status, Status::Warn);
        assert!(hop.summary.starts_with("IPv6 broken"), "{}", hop.summary);
    }

    #[test]
    fn wifi_normal_gateway_rtt_does_not_warn() {
        // Real reading: 9.7 ms median with spiky Wi-Fi jitter, no loss; ISP edge at 6.7 ms.
        let mut m = healthy();
        m.gateway.rtts = [3.1, 25.0, 4.2, 9.7, 31.0, 6.0, 9.9, 2.8, 12.0, 9.7].map(Some).to_vec();
        m.isp.rtts = vec![Some(6.7); 5];
        let hop = &hops(&m)[1];
        assert_eq!(hop.status, Status::Pass, "{hop:#?}");
        assert!(hop.evidence.iter().any(|e| e.key == "note"));
    }

    #[test]
    fn slow_gateway_warns_when_nothing_beyond_is_faster() {
        let mut m = healthy();
        m.gateway.rtts = vec![Some(45.0); 20];
        m.isp.rtts = vec![Some(52.0); 5];
        for t in &mut m.internet {
            t.outcome = TcpOutcome::Attempts { rtts: vec![Some(60.0); 5], error: None };
        }
        assert_eq!(hops(&m)[1].status, Status::Warn);
    }

    #[test]
    fn slow_gateway_warns_when_everything_beyond_is_slow_too() {
        // Live reading: gateway 156 ms, ISP edge 157 ms, internet 110 ms, against the gateway's
        // 30 ms limit. The internet answering faster than the router doesn't excuse it here:
        // 110 ms is still far over the limit, so the latency is local and inherited.
        let mut m = healthy();
        m.gateway.rtts = vec![Some(156.0); 20];
        m.isp.rtts = vec![Some(157.0); 5];
        for t in &mut m.internet {
            t.outcome = TcpOutcome::Attempts { rtts: vec![Some(110.0); 5], error: None };
        }
        let path = hops(&m);
        assert_eq!(path[1].status, Status::Warn, "{:#?}", path[1]);
        let verdict = wirewolf_core::diagnose::diagnose(&path);
        assert_eq!(verdict.blame.as_deref(), Some("gateway"));
        assert_eq!(verdict.headline, "Slow on your local network (Wi-Fi or LAN)");
        assert!(verdict.fix_hint.is_some());
    }

    #[test]
    fn gateway_loss_warns_even_if_downstream_is_faster() {
        let mut m = healthy();
        m.gateway.rtts = rtts(20.0, 20, 4);
        assert_eq!(hops(&m)[1].status, Status::Warn);
    }

    #[test]
    fn router_that_ignores_pings_passes_when_internet_works() {
        let mut m = healthy();
        m.gateway = GatewayProbe { method: PingMethod::Tcp, rtts: vec![None; 3] };
        assert_eq!(hops(&m)[1].status, Status::Pass);
    }

    #[test]
    fn gateway_down() {
        let mut m = healthy();
        m.gateway.rtts = vec![None; 20];
        m.isp.rtts = vec![None; 5];
        for t in &mut m.internet {
            t.outcome = TcpOutcome::Attempts { rtts: vec![None; 5], error: Some("no route to host".into()) };
        }
        let v = diagnose(&hops(&m));
        assert_eq!(v.blame.as_deref(), Some("gateway"));
    }

    #[test]
    fn no_interface() {
        let m = Measurements { host: HostInfo::default(), ..Default::default() };
        let v = diagnose(&hops(&m));
        assert_eq!(v.blame.as_deref(), Some("host"));
    }

    #[test]
    fn stats() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[1.0, 2.0]), Some(1.5));
        assert_eq!(jitter(&[Some(1.0), None, Some(3.0), Some(2.0)]), Some(1.5));
        assert_eq!(loss(&[Some(1.0), None]), Some(50.0));
    }
}
