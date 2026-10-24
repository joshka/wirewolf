//! The live probes: each fills one part of [`Measurements`].

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hickory_resolver::Resolver;
use hickory_resolver::config::{NameServerConfig, ResolverConfig, ResolverOpts};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use tokio::net::TcpStream;

use crate::assess::*;
use crate::icmp::Icmp;

const GATEWAY_PINGS: u16 = 20;
const ISP_PINGS: u16 = 5;
const TCP_ATTEMPTS: usize = 5;
const TCP_TIMEOUT: Duration = Duration::from_secs(1);
const DNS_TIMEOUT: Duration = Duration::from_secs(3);
const WEB_TIMEOUT: Duration = Duration::from_secs(5);
/// Probed past the gateway for the ISP edge and the TTL sweep.
const TRACE_TARGET: Ipv4Addr = Ipv4Addr::new(1, 1, 1, 1);

pub const INTERNET_TARGETS: [(&str, IpAddr, IpAddr); 3] = [
    ("Cloudflare", IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), IpAddr::V6(Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1111))),
    ("Google", IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), IpAddr::V6(Ipv6Addr::new(0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888))),
    ("Quad9", IpAddr::V4(Ipv4Addr::new(9, 9, 9, 9)), IpAddr::V6(Ipv6Addr::new(0x2620, 0xfe, 0, 0, 0, 0, 0, 0xfe))),
];

pub const PUBLIC_RESOLVERS: [(&str, IpAddr); 2] =
    [("Cloudflare 1.1.1.1", IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))), ("Google 8.8.8.8", IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)))];

/// Long-lived probe state.
pub struct Prober {
    icmp: Option<Icmp>,
    http: reqwest::Client,
    tls: tokio_rustls::TlsConnector,
    public_ip: (Option<String>, Option<String>),
    public_ip_at: Option<Instant>,
}

impl Prober {
    pub fn new() -> anyhow::Result<Self> {
        let icmp = Icmp::detect();
        let http = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(WEB_TIMEOUT).user_agent(concat!("wirewolf/", env!("CARGO_PKG_VERSION"))).build()?;
        let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
            .with_safe_default_protocol_versions()?
            .with_root_certificates(roots)
            .with_no_client_auth();
        Ok(Self { icmp, http, tls: tokio_rustls::TlsConnector::from(Arc::new(config)), public_ip: (None, None), public_ip_at: None })
    }

    pub fn icmp(&self) -> Option<Icmp> {
        self.icmp
    }

    /// Runs one full cycle. `trace` asks for a TTL sweep (on an upstream fault).
    pub async fn cycle(&mut self, trace: bool) -> Measurements {
        let mut host = host_info(self.icmp);
        let refresh = self.public_ip_at.is_none_or(|t| t.elapsed() > Duration::from_secs(600) || self.public_ip.0.is_none());
        let has_v6 = !host.ipv6.is_empty();
        let gw = host.gateway;
        let (gateway, isp, internet, dns, web, public) = tokio::join!(
            gateway_probe(self.icmp, gw),
            isp_probe(self.icmp, trace),
            internet_probe(has_v6),
            dns_probe(),
            web_probe(&self.http, &self.tls),
            async {
                if refresh { Some(public_ips(&self.http, has_v6).await) } else { None }
            },
        );
        if let Some(public) = public {
            self.public_ip = public;
            self.public_ip_at = Some(Instant::now());
        }
        host.public_v4 = self.public_ip.0.clone();
        host.public_v6 = self.public_ip.1.clone();
        Measurements { host, gateway, isp, internet, dns, web }
    }
}

pub fn host_info(icmp: Option<Icmp>) -> HostInfo {
    let icmp = match icmp {
        Some(Icmp::Raw) => "raw socket",
        Some(Icmp::Dgram { ttl_exceeded: true }) => "unprivileged socket",
        Some(Icmp::Dgram { ttl_exceeded: false }) => "unprivileged socket (no TTL sweep)",
        None => "unavailable (TCP fallback)",
    }
    .to_string();
    let Ok(iface) = netdev::get_default_interface() else {
        return HostInfo { icmp, ..Default::default() };
    };
    let gateway = iface.gateway.as_ref().and_then(|g| g.ipv4.first().map(|a| IpAddr::V4(*a)).or(g.ipv6.first().map(|a| IpAddr::V6(*a))));
    let container_bridge = std::path::Path::new("/.dockerenv").exists()
        && matches!(gateway, Some(IpAddr::V4(a)) if a.octets()[0] == 172 && (16..32).contains(&a.octets()[1]));
    HostInfo {
        interface: Some(iface.name.clone()),
        up: iface.is_up(),
        ipv4: iface.ipv4.iter().map(|n| n.to_string()).collect(),
        ipv6: iface.ipv6.iter().filter(|n| is_global_v6(&n.addr())).map(|n| n.to_string()).collect(),
        mtu: iface.mtu,
        gateway,
        link_mbps: iface.transmit_speed.filter(|s| *s > 0).map(|s| s / 1_000_000),
        public_v4: None,
        public_v6: None,
        container_bridge,
        icmp,
    }
}

fn is_global_v6(a: &Ipv6Addr) -> bool {
    let first = a.segments()[0];
    // 2000::/3 global unicast.
    (0x2000..0x4000).contains(&first)
}

async fn gateway_probe(icmp: Option<Icmp>, gateway: Option<IpAddr>) -> GatewayProbe {
    let Some(gateway) = gateway else { return GatewayProbe::default() };
    if let (Some(icmp), IpAddr::V4(v4)) = (icmp, gateway) {
        let res = tokio::task::spawn_blocking(move || icmp.burst(v4, 64, GATEWAY_PINGS, Duration::from_millis(50), Duration::from_secs(1))).await;
        match res {
            Ok(Ok(replies)) => return GatewayProbe { method: PingMethod::Icmp, rtts: replies.iter().map(|r| r.map(|r| ms(r.rtt))).collect() },
            Ok(Err(e)) => tracing::debug!("icmp to gateway failed, falling back to tcp: {e}"),
            Err(e) => tracing::warn!("icmp task: {e}"),
        }
    }
    // TCP fallback: a refused connection is as good an answer as an accepted one.
    let mut rtts = Vec::new();
    for port in [53, 80, 443, 53, 80] {
        let start = Instant::now();
        let r = tokio::time::timeout(TCP_TIMEOUT, TcpStream::connect((gateway, port))).await;
        rtts.push(match r {
            Ok(Ok(_)) => Some(ms(start.elapsed())),
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => Some(ms(start.elapsed())),
            _ => None,
        });
    }
    GatewayProbe { method: PingMethod::Tcp, rtts }
}

async fn isp_probe(icmp: Option<Icmp>, trace: bool) -> IspProbe {
    let Some(icmp) = icmp.filter(|i| i.can_trace()) else { return IspProbe::default() };
    let res = tokio::task::spawn_blocking(move || {
        let burst = icmp.burst(TRACE_TARGET, 2, ISP_PINGS, Duration::from_millis(100), Duration::from_secs(1))?;
        let sweep = if trace { Some(icmp.trace(TRACE_TARGET, 12, Duration::from_secs(1))?) } else { None };
        std::io::Result::Ok((burst, sweep))
    })
    .await;
    match res {
        Ok(Ok((burst, sweep))) => IspProbe {
            traceable: true,
            // A reply from the target itself means it's 2 hops away, which is fine too.
            edge: burst.iter().flatten().map(|r| r.from).next(),
            rtts: burst.iter().map(|r| r.map(|r| ms(r.rtt))).collect(),
            trace: sweep.map(|s| s.into_iter().map(|(ttl, r)| (ttl, r.map(|r| (r.from, round1(ms(r.rtt)))))).collect()),
        },
        Ok(Err(e)) => {
            tracing::debug!("isp probe failed: {e}");
            IspProbe::default()
        }
        Err(_) => IspProbe::default(),
    }
}

async fn internet_probe(has_v6: bool) -> Vec<TcpTarget> {
    let mut futures = Vec::new();
    for (name, v4, v6) in INTERNET_TARGETS {
        for (family, ip) in [(Family::V4, v4), (Family::V6, v6)] {
            futures.push(async move {
                let addr = SocketAddr::new(ip, 443);
                let outcome = if family == Family::V6 && !has_v6 {
                    TcpOutcome::NotApplicable("no IPv6 on this host".into())
                } else {
                    tcp_attempts(addr).await
                };
                TcpTarget { name: name.into(), family, addr: addr.to_string(), outcome }
            });
        }
    }
    futures_util::future::join_all(futures).await
}

async fn tcp_attempts(addr: SocketAddr) -> TcpOutcome {
    let mut rtts = Vec::new();
    let mut error = None;
    let mut unreachable = 0;
    for i in 0..TCP_ATTEMPTS {
        if i > 0 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let start = Instant::now();
        match tokio::time::timeout(TCP_TIMEOUT, TcpStream::connect(addr)).await {
            Ok(Ok(stream)) => {
                rtts.push(Some(ms(start.elapsed())));
                drop(stream);
            }
            Ok(Err(e)) => {
                if matches!(e.kind(), std::io::ErrorKind::NetworkUnreachable | std::io::ErrorKind::HostUnreachable) {
                    unreachable += 1;
                }
                error = Some(e.to_string());
                rtts.push(None);
            }
            Err(_) => {
                error = Some("timed out after 1s".into());
                rtts.push(None);
            }
        }
    }
    if addr.is_ipv6() && unreachable == TCP_ATTEMPTS {
        return TcpOutcome::NotApplicable("no IPv6 route".into());
    }
    TcpOutcome::Attempts { rtts, error }
}

async fn dns_probe() -> Vec<DnsResult> {
    let mut futures = Vec::new();
    let system = hickory_resolver::system_conf::read_system_conf().map(|(c, _)| c.name_servers().iter().map(|n| n.ip).collect::<Vec<_>>()).unwrap_or_default();
    let mut seen = Vec::new();
    for ip in system.into_iter().filter(|ip| !seen.contains(ip) && { seen.push(*ip); true }).take(2) {
        futures.push(resolve("System resolver".to_string(), ip, true));
    }
    for (name, ip) in PUBLIC_RESOLVERS {
        futures.push(resolve(name.to_string(), ip, false));
    }
    futures_util::future::join_all(futures).await
}

async fn resolve(name: String, ip: IpAddr, system: bool) -> DnsResult {
    let outcome = lookup(ip).await;
    DnsResult { name, server: ip.to_string(), system, outcome }
}

async fn lookup(ip: IpAddr) -> Result<DnsAnswer, String> {
    let config = ResolverConfig::from_name_servers(vec![NameServerConfig::udp_and_tcp(ip)]);
    let mut opts = ResolverOpts::default();
    opts.timeout = DNS_TIMEOUT;
    opts.attempts = 1;
    opts.cache_size = 0;
    opts.ip_strategy = hickory_resolver::config::LookupIpStrategy::Ipv4Only;
    let resolver = Resolver::builder_with_config(config, TokioRuntimeProvider::default()).with_options(opts).build().map_err(|e| e.to_string())?;
    let start = Instant::now();
    let query = format!("{QUERY_NAME}.");
    match tokio::time::timeout(DNS_TIMEOUT + Duration::from_secs(1), resolver.lookup_ip(query.as_str())).await {
        Ok(Ok(answer)) => {
            let addrs: Vec<IpAddr> = answer.iter().collect();
            Ok(DnsAnswer { rtt_ms: round1(ms(start.elapsed())), expected: addrs.iter().any(|a| QUERY_EXPECT.contains(a)), addrs })
        }
        Ok(Err(e)) => Err(sanitize(&e.to_string())),
        Err(_) => Err(format!("timed out after {}s", DNS_TIMEOUT.as_secs())),
    }
}

async fn web_probe(http: &reqwest::Client, tls: &tokio_rustls::TlsConnector) -> WebResult {
    let portal = async {
        let start = Instant::now();
        match http.get(PORTAL_URL).send().await {
            Ok(r) => Ok(HttpAnswer {
                status: r.status().as_u16(),
                location: r.headers().get("location").and_then(|l| l.to_str().ok()).map(str::to_string),
                rtt_ms: round1(ms(start.elapsed())),
            }),
            Err(e) => Err(error_chain(&e)),
        }
    };
    let handshake = async {
        let host = "www.cloudflare.com";
        let stream = tokio::time::timeout(WEB_TIMEOUT, TcpStream::connect((host, 443))).await.map_err(|_| "connect timed out".to_string())?.map_err(|e| format!("connect to {host}: {e}"))?;
        let name = rustls::pki_types::ServerName::try_from(host).map_err(|e| e.to_string())?;
        let start = Instant::now();
        let stream = tokio::time::timeout(WEB_TIMEOUT, tls.connect(name, stream)).await.map_err(|_| "TLS handshake timed out".to_string())?.map_err(|e| format!("TLS to {host}: {e}"))?;
        let version = match stream.get_ref().1.protocol_version() {
            Some(rustls::ProtocolVersion::TLSv1_3) => "TLS 1.3".to_string(),
            Some(rustls::ProtocolVersion::TLSv1_2) => "TLS 1.2".to_string(),
            v => format!("{v:?}"),
        };
        Ok(TlsAnswer { rtt_ms: round1(ms(start.elapsed())), version, host: host.into() })
    };
    let (portal, tls) = tokio::join!(portal, handshake);
    WebResult { portal: Some(portal), tls: Some(tls) }
}

/// Public addresses from Cloudflare's trace endpoint, over v4 and (if present) v6.
async fn public_ips(http: &reqwest::Client, has_v6: bool) -> (Option<String>, Option<String>) {
    let get = |url: &'static str| async move {
        let body = http.get(url).send().await.ok()?.text().await.ok()?;
        body.lines().find_map(|l| l.strip_prefix("ip=")).map(str::to_string)
    };
    let v6 = async {
        if has_v6 { get("https://[2606:4700:4700::1111]/cdn-cgi/trace").await } else { None }
    };
    tokio::join!(get("https://1.1.1.1/cdn-cgi/trace"), v6)
}

pub fn error_chain(e: &dyn std::error::Error) -> String {
    let mut s = e.to_string();
    let mut source = e.source();
    while let Some(inner) = source {
        s = format!("{s}: {inner}");
        source = inner.source();
    }
    sanitize(&s)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}
