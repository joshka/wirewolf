//! User targets (`[[targets]]` in the config): parsing the check URLs, probing, and judging.

use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use tokio::net::TcpStream;
use wirewolf_core::config::{Limits, UserTarget, redact_url};
use wirewolf_core::model::{Status, Target};

use crate::assess::{loss, median, sanitize};
use crate::icmp::Icmp;
use crate::measure::{error_chain, lookup, ms, round1};

const ATTEMPTS: usize = 3;
const TIMEOUT: Duration = Duration::from_secs(2);

/// A parsed `[[targets]]` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    /// `target:` plus a slug of the name, unique among targets.
    pub id: String,
    pub name: String,
    /// The check URL with credentials removed, for display.
    pub shown: String,
    pub probe: Probe,
    pub critical: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Probe {
    Tcp { host: String, port: u16 },
    Http { url: String, verify_tls: bool },
    Ping { host: String },
    Dns { server: String, port: u16 },
}

impl Probe {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Tcp { .. } => "tcp",
            Self::Http { url, .. } if url.starts_with("https:") => "https",
            Self::Http { .. } => "http",
            Self::Ping { .. } => "ping",
            Self::Dns { .. } => "dns",
        }
    }
}

/// Parses every target, or explains the first one that's wrong.
pub fn parse_all(targets: &[UserTarget]) -> Result<Vec<Spec>, String> {
    let mut specs: Vec<Spec> = Vec::new();
    for t in targets {
        let probe = parse(&t.check, t.verify_tls).map_err(|e| format!("target {:?}: {e}", t.name))?;
        let base = format!("target:{}", slug(&t.name));
        let mut id = base.clone();
        let mut n = 2;
        while specs.iter().any(|s| s.id == id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        specs.push(Spec { id, name: t.name.clone(), shown: redact_url(&t.check), probe, critical: t.critical });
    }
    Ok(specs)
}

fn slug(name: &str) -> String {
    let s: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "target".into() } else { s }
}

pub fn parse(check: &str, verify_tls: bool) -> Result<Probe, String> {
    let Some((scheme, _)) = check.split_once("://") else {
        return Err(format!("{check:?} needs a scheme: tcp://, http://, https://, ping://, or dns://"));
    };
    let url = reqwest::Url::parse(check).map_err(|e| format!("{check:?}: {e}"))?;
    let host = url.host_str().filter(|h| !h.is_empty()).ok_or_else(|| format!("{check:?} has no host"))?;
    let host = host.trim_start_matches('[').trim_end_matches(']').to_string();
    if host.parse::<IpAddr>().is_ok_and(link_local) {
        return Err(format!("{check:?}: {LINK_LOCAL}"));
    }
    match scheme {
        "tcp" => Ok(Probe::Tcp { host, port: url.port().ok_or_else(|| format!("{check:?} needs a port, like tcp://nas.local:445"))? }),
        "http" | "https" => Ok(Probe::Http { url: check.to_string(), verify_tls }),
        "ping" | "icmp" => Ok(Probe::Ping { host }),
        "dns" => Ok(Probe::Dns { server: host, port: url.port().unwrap_or(53) }),
        other => Err(format!("unknown check {other}:// (expected tcp, http, https, ping, or dns)")),
    }
}

/// What one probe of a target saw.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// Repeated attempts (TCP or ICMP): RTTs, `None` for each failure, and the last error.
    Attempts { rtts: Vec<Option<f64>>, error: Option<String>, method: String },
    /// One HTTP request: status and time.
    Http(Result<(u16, f64), String>),
    /// One DNS lookup: time and the first address.
    Dns(Result<(f64, Option<IpAddr>), String>),
}

/// Long-lived clients the probes share.
pub struct Clients<'a> {
    pub icmp: Option<Icmp>,
    pub http: &'a reqwest::Client,
    pub http_insecure: &'a reqwest::Client,
    /// The name asked of `dns://` targets.
    pub dns_query: &'a str,
}

pub async fn probe(spec: &Spec, c: &Clients<'_>) -> Outcome {
    match &spec.probe {
        Probe::Tcp { host, port } => match resolve(host, *port).await {
            Ok(addr) => tcp_attempts(addr, &format!("TCP :{port}")).await,
            Err(e) => Outcome::Attempts { rtts: vec![None], error: Some(e), method: format!("TCP :{port}") },
        },
        Probe::Ping { host } => {
            let addr = match resolve(host, 0).await {
                Ok(a) => a,
                Err(e) => return Outcome::Attempts { rtts: vec![None], error: Some(e), method: "ping".into() },
            };
            if let (Some(icmp), IpAddr::V4(v4)) = (c.icmp, addr.ip()) {
                let res = tokio::task::spawn_blocking(move || icmp.burst(v4, 64, 5, Duration::from_millis(100), Duration::from_secs(1))).await;
                if let Ok(Ok(replies)) = res {
                    let rtts: Vec<Option<f64>> = replies.iter().map(|r| r.map(|r| ms(r.rtt))).collect();
                    let error = rtts.iter().all(Option::is_none).then(|| "no echo replies".to_string());
                    return Outcome::Attempts { rtts, error, method: "ICMP echo".into() };
                }
            }
            // No ICMP: any TCP answer, even a refusal, shows the host is up.
            let mut rtts = Vec::new();
            let mut error = None;
            for port in [80, 443, 22] {
                let start = Instant::now();
                match tokio::time::timeout(TIMEOUT, TcpStream::connect(SocketAddr::new(addr.ip(), port))).await {
                    Ok(Ok(_)) => rtts.push(Some(ms(start.elapsed()))),
                    Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => rtts.push(Some(ms(start.elapsed()))),
                    Ok(Err(e)) => {
                        error = Some(e.to_string());
                        rtts.push(None);
                    }
                    Err(_) => {
                        error = Some(format!("timed out after {}s", TIMEOUT.as_secs()));
                        rtts.push(None);
                    }
                }
            }
            Outcome::Attempts { rtts, error, method: "TCP connect (ICMP unavailable)".into() }
        }
        Probe::Http { url, verify_tls } => {
            let client = if *verify_tls { c.http } else { c.http_insecure };
            let start = Instant::now();
            Outcome::Http(match client.get(url).send().await {
                Ok(r) => Ok((r.status().as_u16(), round1(ms(start.elapsed())))),
                Err(e) => Err(error_chain(&e)),
            })
        }
        Probe::Dns { server, port } => {
            let addr = match resolve(server, *port).await {
                Ok(a) => a,
                Err(e) => return Outcome::Dns(Err(e)),
            };
            Outcome::Dns(lookup(addr, c.dns_query, false).await.map(|a| (a.rtt_ms, a.addrs.first().copied())))
        }
    }
}

async fn resolve(host: &str, port: u16) -> Result<SocketAddr, String> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return if link_local(ip) { Err(LINK_LOCAL.into()) } else { Ok(SocketAddr::new(ip, port)) };
    }
    let addrs = tokio::time::timeout(TIMEOUT, tokio::net::lookup_host((host, port)))
        .await
        .map_err(|_| format!("resolving {host} timed out"))?
        .map_err(|e| format!("can't resolve {host}: {e}"))?;
    let all: Vec<SocketAddr> = addrs.filter(|a| !link_local(a.ip())).collect();
    if all.is_empty() {
        return Err(format!("{host}: {LINK_LOCAL}"));
    }
    all.iter().find(|a| a.is_ipv4()).or(all.first()).copied().ok_or_else(|| format!("{host} has no addresses"))
}

const LINK_LOCAL: &str = "cloud metadata addresses (169.254.0.0/16) aren't probed";

/// Cloud metadata addresses: IPv4 link-local (169.254.0.0/16) and AWS's IPv6 endpoint. Probes
/// never go there, so a settings writer can't use targets to reach them. IPv6 link-local stays
/// allowed: routers are often only reachable at `fe80::1`.
pub fn link_local(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(a) => a.is_link_local(),
        IpAddr::V6(a) => a == std::net::Ipv6Addr::new(0xfd00, 0xec2, 0, 0, 0, 0, 0, 0x254),
    }
}

/// A DNS resolver for the probe HTTP clients that drops metadata addresses, so an
/// `http://` target or captive portal URL whose name resolves there isn't fetched.
pub struct NoLinkLocal;

impl reqwest::dns::Resolve for NoLinkLocal {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0)).await?.filter(|a| !link_local(a.ip())).collect();
            if addrs.is_empty() {
                return Err(format!("{host}: {LINK_LOCAL}").into());
            }
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

async fn tcp_attempts(addr: SocketAddr, method: &str) -> Outcome {
    let mut rtts = Vec::new();
    let mut error = None;
    for i in 0..ATTEMPTS {
        if i > 0 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let start = Instant::now();
        match tokio::time::timeout(TIMEOUT, TcpStream::connect(addr)).await {
            Ok(Ok(_)) => rtts.push(Some(ms(start.elapsed()))),
            Ok(Err(e)) => {
                error = Some(e.to_string());
                rtts.push(None);
            }
            Err(_) => {
                error = Some(format!("timed out after {}s", TIMEOUT.as_secs()));
                rtts.push(None);
            }
        }
    }
    Outcome::Attempts { rtts, error, method: format!("{method} to {}", addr.ip()) }
}

/// Judges one outcome against the target limits.
pub fn assess(spec: &Spec, outcome: &Outcome, limits: &Limits) -> Target {
    let slow = |rtt: f64| rtt > limits.rtt_warn_ms;
    let (status, detail, rtt_ms, loss_pct) = match outcome {
        Outcome::Attempts { rtts, error, method } => {
            let ok: Vec<f64> = rtts.iter().flatten().copied().collect();
            let rtt = median(&ok).map(round1);
            let lost = loss(rtts).map(round1);
            match (rtt, lost) {
                (None, _) => (Status::Fail, format!("{method}: {}", error.as_deref().unwrap_or("no reply")), None, lost),
                (Some(r), Some(l)) if l >= limits.loss_warn_pct => (Status::Warn, format!("{method}: {r:.1} ms, {l:.0}% lost"), rtt, lost),
                (Some(r), _) if slow(r) => (Status::Warn, format!("{method}: slow, {r:.1} ms"), rtt, lost),
                (Some(r), _) => (Status::Pass, format!("{method}: {r:.1} ms"), rtt, lost),
            }
        }
        Outcome::Http(Ok((code, rtt))) if *code >= 500 => (Status::Warn, format!("server error {code} in {rtt:.0} ms"), Some(*rtt), None),
        Outcome::Http(Ok((code, rtt))) if slow(*rtt) => (Status::Warn, format!("{code}, slow: {rtt:.0} ms"), Some(*rtt), None),
        Outcome::Http(Ok((code, rtt))) => (Status::Pass, format!("{code} in {rtt:.0} ms"), Some(*rtt), None),
        Outcome::Http(Err(e)) => (Status::Fail, e.clone(), None, None),
        Outcome::Dns(Ok((rtt, addr))) => {
            let answer = addr.map_or("no addresses".to_string(), |a| a.to_string());
            (if slow(*rtt) { Status::Warn } else { Status::Pass }, format!("answers {answer} in {rtt:.1} ms"), Some(*rtt), None)
        }
        Outcome::Dns(Err(e)) => (Status::Fail, e.clone(), None, None),
    };
    Target {
        id: spec.id.clone(),
        name: spec.name.clone(),
        check: spec.shown.clone(),
        kind: spec.probe.kind().into(),
        critical: spec.critical,
        status,
        detail: sanitize(&detail),
        rtt_ms,
        loss_pct,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str, check: &str) -> UserTarget {
        UserTarget { name: name.into(), check: check.into(), critical: false, verify_tls: true }
    }

    #[test]
    fn link_local_hosts_are_refused() {
        for check in ["http://169.254.169.254/latest/meta-data/", "tcp://[fd00:ec2::254]:80", "ping://169.254.1.1"] {
            assert!(parse(check, true).unwrap_err().contains("metadata"), "{check}");
        }
        assert!(link_local("::ffff:169.254.169.254".parse().unwrap()));
        assert!(!link_local("192.0.2.1".parse().unwrap()));
        assert!(parse("http://192.0.2.1/", true).is_ok());
        assert!(parse("ping://[fe80::1]", true).is_ok(), "IPv6 routers are often link-local");
    }

    #[test]
    fn parses_checks() {
        assert_eq!(parse("tcp://nas.local:445", true), Ok(Probe::Tcp { host: "nas.local".into(), port: 445 }));
        assert_eq!(parse("ping://10.0.0.5", true), Ok(Probe::Ping { host: "10.0.0.5".into() }));
        assert_eq!(parse("ping://[fe80::1]", true), Ok(Probe::Ping { host: "fe80::1".into() }));
        assert_eq!(parse("dns://pihole.local", true), Ok(Probe::Dns { server: "pihole.local".into(), port: 53 }));
        assert!(matches!(parse("https://router.local", false), Ok(Probe::Http { verify_tls: false, .. })));
        assert!(parse("tcp://nas.local", true).unwrap_err().contains("port"));
        assert!(parse("nas.local:445", true).unwrap_err().contains("scheme"));
        assert!(parse("ftp://nas.local", true).unwrap_err().contains("unknown"));
    }

    #[test]
    fn ids_are_unique_slugs() {
        let specs = parse_all(&[t("Router admin", "https://a"), t("Router admin", "https://b"), t("NAS!", "ping://n")]).unwrap();
        let ids: Vec<&str> = specs.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["target:router-admin", "target:router-admin-2", "target:nas"]);
    }

    #[test]
    fn judges_outcomes() {
        let spec = parse_all(&[t("NAS", "tcp://nas.local:445")]).unwrap().remove(0);
        let limits = Limits::default();
        let ok = Outcome::Attempts { rtts: vec![Some(1.0), Some(2.0), Some(1.5)], error: None, method: "TCP :445".into() };
        assert_eq!(assess(&spec, &ok, &limits).status, Status::Pass);
        let lossy = Outcome::Attempts { rtts: vec![Some(1.0), None, Some(1.5)], error: None, method: "TCP :445".into() };
        assert_eq!(assess(&spec, &lossy, &limits).status, Status::Warn);
        let down = Outcome::Attempts { rtts: vec![None; 3], error: Some("connection refused".into()), method: "TCP :445".into() };
        let target = assess(&spec, &down, &limits);
        assert_eq!(target.status, Status::Fail);
        assert!(target.detail.contains("refused"));
        assert_eq!(assess(&spec, &Outcome::Http(Ok((401, 12.0))), &limits).status, Status::Pass);
        assert_eq!(assess(&spec, &Outcome::Http(Ok((503, 12.0))), &limits).status, Status::Warn);
    }
}
