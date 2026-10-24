//! Turns raw measurements into hops. Pure functions only, so the status rules are testable on
//! real-shaped input without touching the network.

use std::net::IpAddr;

use wirewolf_core::model::{Check, Evidence, Hop, HopKind, HopMetrics, Status};

/// Thresholds for "degraded". Loss is percent, times are milliseconds.
pub mod limits {
    pub const GATEWAY_LOSS_WARN: f64 = 10.0;
    pub const GATEWAY_RTT_WARN: f64 = 50.0;
    pub const GATEWAY_JITTER_WARN: f64 = 20.0;
    pub const INTERNET_LOSS_WARN: f64 = 10.0;
    pub const INTERNET_RTT_WARN: f64 = 150.0;
    pub const INTERNET_JITTER_WARN: f64 = 40.0;
    pub const ISP_LOSS_WARN: f64 = 25.0;
    pub const DNS_RTT_WARN: f64 = 250.0;
}

/// Everything one probe cycle measured.
#[derive(Debug, Clone, Default)]
pub struct Measurements {
    pub host: HostInfo,
    pub gateway: GatewayProbe,
    pub isp: IspProbe,
    pub internet: Vec<TcpTarget>,
    pub dns: Vec<DnsResult>,
    pub web: WebResult,
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
    pub outcome: Result<DnsAnswer, String>,
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
    pub portal: Option<Result<HttpAnswer, String>>,
    pub tls: Option<Result<TlsAnswer, String>>,
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
    let internet = internet(&m.internet);
    let internet_ok = matches!(internet.status, Status::Pass | Status::Warn);
    let mut hops = vec![host(&m.host), gateway(&m.gateway, &m.host, internet_ok), isp(&m.isp, internet_ok, &internet), internet, dns(&m.dns), web(&m.web)];
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

pub fn gateway(g: &GatewayProbe, h: &HostInfo, internet_ok: bool) -> Hop {
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
    let evidence = vec![ev("address", addr), ev("method", &method)];
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
    let degraded = lost >= limits::GATEWAY_LOSS_WARN
        || m.rtt_ms.is_some_and(|r| r > limits::GATEWAY_RTT_WARN)
        || m.jitter_ms.is_some_and(|j| j > limits::GATEWAY_JITTER_WARN);
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

pub fn isp(p: &IspProbe, internet_ok: bool, internet: &Hop) -> Hop {
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
    let internet_lossy = internet.metrics.loss_pct.is_some_and(|l| l >= limits::INTERNET_LOSS_WARN);
    let status = if lost >= limits::ISP_LOSS_WARN && internet_lossy { Status::Warn } else { Status::Pass };
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

pub fn internet(targets: &[TcpTarget]) -> Hop {
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
                } else if lost >= limits::INTERNET_LOSS_WARN {
                    Status::Warn
                } else {
                    Status::Pass
                };
                let detail = if any {
                    format!("TCP :443 to {} — {}/{} handshakes", t.addr, rtts.iter().flatten().count(), rtts.len())
                } else {
                    format!("{} — {}", t.addr, error.as_deref().unwrap_or("no handshake completed"))
                };
                checks.push(check(&name, status, detail, m.rtt_ms));
            }
        }
    }
    let best = checks.iter().filter_map(|c| c.rtt_ms).fold(None, |a: Option<f64>, r| Some(a.map_or(r, |a| a.min(r))));
    let m = HopMetrics {
        rtt_ms: best,
        jitter_ms: (!jitters.is_empty()).then(|| round1(jitters.iter().sum::<f64>() / jitters.len() as f64)),
        loss_pct: loss(&all).map(round1),
    };
    let any_ok = family_ok.contains(&Some(true));
    let broken_family = family_ok.iter().position(|f| *f == Some(false));
    let status = if checks.iter().all(|c| c.status == Status::NotApplicable) {
        Status::NotApplicable
    } else if !any_ok {
        Status::Fail
    } else if broken_family.is_some()
        || m.loss_pct.is_some_and(|l| l >= limits::INTERNET_LOSS_WARN)
        || m.rtt_ms.is_some_and(|r| r > limits::INTERNET_RTT_WARN)
        || m.jitter_ms.is_some_and(|j| j > limits::INTERNET_JITTER_WARN)
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
        evidence: vec![ev("method", format!("TCP :443 handshake × {} per target", per.unwrap_or(0))), ev("families", families)],
        checks,
    }
}

pub fn dns(results: &[DnsResult]) -> Hop {
    let mut checks = Vec::new();
    for r in results {
        let label = if r.system { format!("{} — ", r.server) } else { String::new() };
        checks.push(match &r.outcome {
            Ok(a) if a.expected => check(&r.name, Status::Pass, format!("{label}{} → {}", QUERY_NAME, list(&a.addrs)), Some(a.rtt_ms)),
            Ok(a) => check(&r.name, Status::Warn, format!("{label}unexpected answer {} (filtered or hijacked?)", list(&a.addrs)), Some(a.rtt_ms)),
            Err(e) => check(&r.name, Status::Fail, format!("{label}{e}"), None),
        });
    }
    let system: Vec<&DnsResult> = results.iter().filter(|r| r.system).collect();
    let public: Vec<&DnsResult> = results.iter().filter(|r| !r.system).collect();
    let sys_ok: Vec<&DnsAnswer> = system.iter().filter_map(|r| r.outcome.as_ref().ok()).collect();
    let pub_ok = public.iter().any(|r| r.outcome.is_ok());
    let sys_rtt = sys_ok.iter().map(|a| a.rtt_ms).fold(None, |m: Option<f64>, r| Some(m.map_or(r, |m| m.min(r))));
    let servers: Vec<&str> = system.iter().map(|r| r.server.as_str()).collect();
    let (status, summary) = if system.is_empty() {
        (Status::Fail, "no system resolver configured".to_string())
    } else if sys_ok.is_empty() && pub_ok {
        (Status::Fail, "system resolver failing; public resolvers fine".into())
    } else if sys_ok.is_empty() {
        (Status::Fail, "no resolver answers".into())
    } else if sys_ok.iter().any(|a| !a.expected) {
        (Status::Warn, "system resolver gives unexpected answers".into())
    } else if sys_rtt.is_some_and(|r| r > limits::DNS_RTT_WARN) {
        (Status::Warn, format!("slow: {:.1} ms via system resolver", sys_rtt.unwrap()))
    } else if !public.is_empty() && !pub_ok {
        (Status::Warn, format!("{:.1} ms via system resolver; public resolvers blocked", sys_rtt.unwrap()))
    } else {
        (Status::Pass, format!("{:.1} ms via system resolver", sys_rtt.unwrap()))
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
        evidence: vec![ev("resolvers", if servers.is_empty() { "none".into() } else { servers.join(", ") }), ev("query", format!("{QUERY_NAME} A"))],
        checks,
    }
}

/// Queried on every resolver; it must contain one of [`QUERY_EXPECT`].
pub const QUERY_NAME: &str = "one.one.one.one";
pub const QUERY_EXPECT: [IpAddr; 2] = [IpAddr::V4(std::net::Ipv4Addr::new(1, 1, 1, 1)), IpAddr::V4(std::net::Ipv4Addr::new(1, 0, 0, 1))];

fn list(addrs: &[IpAddr]) -> String {
    if addrs.is_empty() {
        return "no addresses".into();
    }
    addrs.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
}

pub fn web(w: &WebResult) -> Hop {
    let mut evidence = Vec::new();
    let portal = match &w.portal {
        None => check("Captive portal", Status::Pending, "not run", None),
        Some(Ok(a)) if a.status == 204 => {
            evidence.push(ev("http status", "204 No Content"));
            check("Captive portal", Status::Pass, "204 from cp.cloudflare.com", Some(a.rtt_ms))
        }
        Some(Ok(a)) => {
            evidence.push(ev("http status", a.status));
            let to = a.location.as_deref().map(|l| format!(" → {}", sanitize(l))).unwrap_or_default();
            if let Some(l) = &a.location {
                evidence.push(ev("location", sanitize(l)));
            }
            check("Captive portal", Status::Fail, format!("expected 204, got {}{to}", a.status), Some(a.rtt_ms))
        }
        Some(Err(e)) => check("Captive portal", Status::Fail, format!("{PORTAL_URL}: {e}"), None),
    };
    let tls = match &w.tls {
        None => check("TLS handshake", Status::Pending, "not run", None),
        Some(Ok(t)) => {
            evidence.push(ev("tls", format!("{}, certificate trusted", t.version)));
            check("TLS handshake", Status::Pass, format!("{} to {}", t.version, t.host), Some(t.rtt_ms))
        }
        Some(Err(e)) => check("TLS handshake", Status::Fail, e.as_str(), None),
    };
    let status = worst([portal.status, tls.status]);
    let intercepted = matches!(&w.portal, Some(Ok(a)) if a.status != 204);
    let summary = match status {
        Status::Pass => format!("HTTP in {:.0} ms, TLS in {:.0} ms", portal.rtt_ms.unwrap_or_default(), tls.rtt_ms.unwrap_or_default()),
        _ if intercepted && matches!(&w.portal, Some(Ok(a)) if a.location.is_some()) => "HTTP redirected to a login page".into(),
        _ if intercepted => "HTTP intercepted".into(),
        Status::Fail if portal.status == Status::Fail => "HTTP requests fail".into(),
        Status::Fail => "TLS handshake fails".into(),
        _ => "pending".into(),
    };
    Hop {
        id: "web".into(),
        kind: HopKind::Web,
        label: "Web".into(),
        target: Some("cp.cloudflare.com".into()),
        status,
        summary,
        metrics: HopMetrics { rtt_ms: portal.rtt_ms, jitter_ms: None, loss_pct: None },
        evidence,
        checks: vec![portal, tls],
    }
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
        TcpTarget { name: name.into(), family, addr: "1.1.1.1:443".into(), outcome: TcpOutcome::Attempts { rtts: r, error } }
    }

    fn dns_ok(name: &str, system: bool, rtt: f64) -> DnsResult {
        DnsResult {
            name: name.into(),
            server: "192.168.1.1".into(),
            system,
            outcome: Ok(DnsAnswer { rtt_ms: rtt, addrs: QUERY_EXPECT.to_vec(), expected: true }),
        }
    }

    fn web_ok() -> WebResult {
        WebResult {
            portal: Some(Ok(HttpAnswer { status: 204, location: None, rtt_ms: 22.0 })),
            tls: Some(Ok(TlsAnswer { rtt_ms: 15.0, version: "TLS 1.3".into(), host: "www.cloudflare.com".into() })),
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
