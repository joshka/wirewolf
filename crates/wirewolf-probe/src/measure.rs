//! The live probes: each fills one part of [`Measurements`].

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hickory_resolver::Resolver;
use hickory_resolver::config::{LookupIpStrategy, NameServerConfig, ResolverConfig, ResolverOpts};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use tokio::net::TcpStream;
use wirewolf_core::config::{Config, Ipv6Mode, RecordType};
use wirewolf_core::model::Target;

use crate::assess::*;
use crate::icmp::Icmp;
use crate::targets;

const GATEWAY_PINGS: u16 = 20;
const ISP_PINGS: u16 = 5;
const TCP_ATTEMPTS: usize = 5;
const TCP_TIMEOUT: Duration = Duration::from_secs(1);
const DNS_TIMEOUT: Duration = Duration::from_secs(3);
const WEB_TIMEOUT: Duration = Duration::from_secs(5);
/// Probed past the gateway for the ISP edge and the TTL sweep.
const TRACE_TARGET: Ipv4Addr = Ipv4Addr::new(1, 1, 1, 1);

/// How often to re-measure the path MTU; it rarely changes.
const MTU_EVERY: Duration = Duration::from_secs(600);

/// Long-lived probe state.
pub struct Prober {
    config: Arc<Config>,
    targets: Vec<targets::Spec>,
    icmp: Option<Icmp>,
    http: reqwest::Client,
    http_insecure: reqwest::Client,
    path_mtu: Option<PathMtu>,
    path_mtu_at: Option<Instant>,
    tls: tokio_rustls::TlsConnector,
    public_ip: (Option<String>, Option<String>),
    public_ip_at: Option<Instant>,
}

impl Prober {
    /// Fails if a `[[targets]]` check can't be parsed.
    pub fn new(config: Arc<Config>) -> anyhow::Result<Self> {
        let targets = targets::parse_all(&config.targets).map_err(anyhow::Error::msg)?;
        let icmp = Icmp::detect();
        let builder = || reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(WEB_TIMEOUT).user_agent(concat!("wirewolf/", env!("CARGO_PKG_VERSION")));
        let http = builder().build()?;
        let http_insecure = builder().danger_accept_invalid_certs(true).build()?;
        let cfg = config;
        let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
            .with_safe_default_protocol_versions()?
            .with_root_certificates(roots)
            .with_no_client_auth();
        Ok(Self {
            config: cfg,
            targets,
            icmp,
            http,
            http_insecure,
            path_mtu: None,
            path_mtu_at: None,
            tls: tokio_rustls::TlsConnector::from(Arc::new(config)),
            public_ip: (None, None),
            public_ip_at: None,
        })
    }

    pub fn icmp(&self) -> Option<Icmp> {
        self.icmp
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Runs one full cycle. `trace` asks for a TTL sweep (on an upstream fault).
    pub async fn cycle(&mut self, trace: bool) -> (Measurements, Vec<Target>) {
        let c = self.config.clone();
        let mut host = host_info(self.icmp);
        let refresh = self.public_ip_at.is_none_or(|t| t.elapsed() > Duration::from_secs(600) || self.public_ip.0.is_none());
        let has_v6 = match c.probe.ipv6 {
            Ipv6Mode::Auto => !host.ipv6.is_empty(),
            Ipv6Mode::On => true,
            Ipv6Mode::Off => false,
        };
        let mtu_due = c.probe.path_mtu && self.icmp.is_some() && self.path_mtu_at.is_none_or(|t| t.elapsed() > MTU_EVERY);
        let local_mtu = host.mtu;
        let gw = host.gateway;
        let clients = targets::Clients {
            icmp: self.icmp,
            http: &self.http,
            http_insecure: &self.http_insecure,
            dns_query: c.dns.queries.first().map_or(QUERY_NAME, |q| q.name.as_str()),
        };
        let user = futures_util::future::join_all(self.targets.iter().map(|spec| async {
            let outcome = targets::probe(spec, &clients).await;
            targets::assess(spec, &outcome, &c.thresholds.targets)
        }));
        let (gateway, isp, internet, dns, web, public, mtu, user) = tokio::join!(
            gateway_probe(self.icmp, gw),
            isp_probe(self.icmp, trace),
            internet_probe(&c, self.icmp, has_v6),
            dns_probe(&c, gw, has_v6),
            web_probe(&c, &self.http, &self.tls),
            async {
                if refresh { Some(public_ips(&self.http, has_v6).await) } else { None }
            },
            async {
                if mtu_due { Some(path_mtu(self.icmp, local_mtu).await) } else { None }
            },
            user,
        );
        if let Some(public) = public {
            self.public_ip = public;
            self.public_ip_at = Some(Instant::now());
        }
        if let Some(mtu) = mtu {
            self.path_mtu = mtu;
            self.path_mtu_at = Some(Instant::now());
        }
        host.public_v4 = self.public_ip.0.clone();
        host.public_v6 = self.public_ip.1.clone();
        host.path_mtu = self.path_mtu.clone();
        (Measurements { host, gateway, isp, internet, dns, web, thresholds: c.thresholds.clone() }, user)
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
        path_mtu: None,
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

async fn internet_probe(config: &Config, icmp: Option<Icmp>, has_v6: bool) -> Vec<TcpTarget> {
    let port = config.internet.port;
    let icmp = icmp.filter(|_| config.internet.icmp);
    let mut futures = Vec::new();
    for t in &config.internet.targets {
        for (family, ip) in [(Family::V4, t.v4.map(IpAddr::V4)), (Family::V6, t.v6.map(IpAddr::V6))] {
            let Some(ip) = ip else { continue };
            futures.push(async move {
                let addr = SocketAddr::new(ip, port);
                if family == Family::V6 && !has_v6 {
                    return TcpTarget { name: t.name.clone(), family, addr: addr.to_string(), outcome: TcpOutcome::NotApplicable("no IPv6 on this host".into()), icmp: None };
                }
                let ping = async {
                    let (Some(icmp), IpAddr::V4(v4)) = (icmp, ip) else { return None };
                    let res = tokio::task::spawn_blocking(move || icmp.burst(v4, 64, 5, Duration::from_millis(100), Duration::from_secs(1))).await;
                    match res {
                        Ok(Ok(replies)) => Some(replies.iter().map(|r| r.map(|r| ms(r.rtt))).collect()),
                        _ => None,
                    }
                };
                let (outcome, icmp) = tokio::join!(tcp_attempts(addr), ping);
                TcpTarget { name: t.name.clone(), family, addr: addr.to_string(), outcome, icmp }
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

async fn dns_probe(config: &Config, gateway: Option<IpAddr>, has_v6: bool) -> Vec<DnsResult> {
    let mut futures = Vec::new();
    if config.dns.system {
        let system = hickory_resolver::system_conf::read_system_conf().map(|(c, _)| c.name_servers().iter().map(|n| n.ip).collect::<Vec<_>>()).unwrap_or_default();
        let mut seen = Vec::new();
        for ip in system.into_iter().filter(|ip| !seen.contains(ip) && { seen.push(*ip); true }).take(2) {
            futures.push(resolve(config, "System resolver".to_string(), ip, true, gateway));
        }
    }
    for r in &config.dns.resolvers {
        if r.address.is_ipv6() && !has_v6 {
            continue;
        }
        futures.push(resolve(config, format!("{} {}", r.name, r.address), r.address, false, gateway));
    }
    futures_util::future::join_all(futures).await
}

async fn resolve(config: &Config, name: String, ip: IpAddr, system: bool, gateway: Option<IpAddr>) -> DnsResult {
    let known = known_resolver(ip, gateway);
    // Ask local resolvers what software they run; Pi-hole and dnsmasq say so in `version.bind`.
    // Well-known addresses are labeled by address (Tailscale forwards CHAOS queries upstream).
    let local = match ip {
        IpAddr::V4(v4) => v4.is_private(),
        IpAddr::V6(v6) => v6.is_unique_local(),
    };
    let addr = SocketAddr::new(ip, 53);
    let queries = &config.dns.queries;
    let (query, expect) = queries.first().map_or((QUERY_NAME.to_string(), QUERY_EXPECT.to_vec()), |q| (q.name.clone(), q.expect.clone()));
    let types = &config.dns.record_types;
    // Every (name, type) pair after the headline `A` of the first name.
    let extra = queries.iter().enumerate().flat_map(|(i, q)| types.iter().filter(move |t| !(i == 0 && **t == RecordType::A)).map(move |t| (q.name.clone(), *t)));
    let extra = futures_util::future::join_all(extra.map(|(name, t)| async move {
        let aaaa = t == RecordType::Aaaa;
        let outcome = lookup(addr, &name, aaaa).await.map(|a| a.rtt_ms);
        ExtraQuery { query: format!("{} {name}", if aaaa { "AAAA" } else { "A" }), outcome }
    }));
    let primary = async {
        lookup(addr, &query, false).await.map(|a| DnsAnswer { expected: expect.is_empty() || a.addrs.iter().any(|x| expect.contains(x)), ..a })
    };
    let (outcome, extra, software) = tokio::join!(primary, extra, async {
        if system && local && known.is_none_or(|k| k == "your router") { version_bind(ip).await } else { None }
    });
    let label = match (known, software.as_deref().and_then(resolver_software)) {
        (Some("your router"), Some(sw)) => Some(format!("your router, {sw}")),
        (Some(k), _) => Some(k.to_string()),
        (None, sw) => sw,
    };
    DnsResult { name, server: ip.to_string(), system, label, query, outcome, extra }
}

/// A CHAOS-class TXT query for `version.bind`, which many resolvers answer with their software.
async fn version_bind(ip: IpAddr) -> Option<String> {
    let socket = tokio::net::UdpSocket::bind(if ip.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }).await.ok()?;
    let id = fastrand::u16(..);
    let mut q = id.to_be_bytes().to_vec();
    q.extend([0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
    q.extend(b"\x07version\x04bind\x00");
    q.extend([0, 16, 0, 3]); // TXT, CH
    socket.send_to(&q, (ip, 53)).await.ok()?;
    let mut buf = [0u8; 512];
    let len = tokio::time::timeout(Duration::from_secs(1), socket.recv(&mut buf)).await.ok()?.ok()?;
    parse_txt_answer(&buf[..len], id)
}

/// Returns the first TXT string from a response to the query built in [`version_bind`].
fn parse_txt_answer(msg: &[u8], id: u16) -> Option<String> {
    if msg.len() < 12 || u16::from_be_bytes([msg[0], msg[1]]) != id || u16::from_be_bytes([msg[6], msg[7]]) == 0 {
        return None;
    }
    // Skip the question: our qname is 14 bytes, then type and class.
    let mut i = 12 + 14 + 4;
    // Answer name: a pointer (2 bytes) or labels.
    if *msg.get(i)? & 0xc0 == 0xc0 {
        i += 2;
    } else {
        while *msg.get(i)? != 0 {
            i += usize::from(msg[i]) + 1;
        }
        i += 1;
    }
    i += 10; // type, class, ttl, rdlength
    let n = usize::from(*msg.get(i)?);
    let txt = msg.get(i + 1..i + 1 + n)?;
    Some(String::from_utf8_lossy(txt).into_owned())
}


/// Resolves `name` (A, or AAAA) at one server. A name with no records of that type answers
/// with no addresses.
pub(crate) async fn lookup(server: SocketAddr, name: &str, aaaa: bool) -> Result<DnsAnswer, String> {
    let config = ResolverConfig::from_name_servers(vec![NameServerConfig::udp_and_tcp(server.ip())]);
    let mut opts = ResolverOpts::default();
    opts.timeout = DNS_TIMEOUT;
    opts.attempts = 1;
    opts.cache_size = 0;
    opts.ip_strategy = if aaaa { LookupIpStrategy::Ipv6Only } else { LookupIpStrategy::Ipv4Only };
    let resolver = Resolver::builder_with_config(config, TokioRuntimeProvider::default()).with_options(opts).build().map_err(|e| e.to_string())?;
    let start = Instant::now();
    let query = format!("{}.", name.trim_end_matches('.'));
    let lookup = async { resolver.lookup_ip(query.as_str()).await.map(|l| l.iter().collect::<Vec<_>>()) };
    match tokio::time::timeout(DNS_TIMEOUT + Duration::from_secs(1), lookup).await {
        Ok(Ok(addrs)) => Ok(DnsAnswer { rtt_ms: round1(ms(start.elapsed())), expected: true, addrs }),
        Ok(Err(e)) if e.is_no_records_found() => Ok(DnsAnswer { rtt_ms: round1(ms(start.elapsed())), expected: true, addrs: vec![] }),
        Ok(Err(e)) => Err(sanitize(&e.to_string())),
        Err(_) => Err(format!("timed out after {}s", DNS_TIMEOUT.as_secs())),
    }
}

async fn web_probe(config: &Config, http: &reqwest::Client, tls: &tokio_rustls::TlsConnector) -> WebResult {
    let portal_url = config.web.captive_portal.as_str();
    let portal = async {
        let start = Instant::now();
        match http.get(portal_url).send().await {
            Ok(r) => Ok(HttpAnswer {
                status: r.status().as_u16(),
                location: r.headers().get("location").and_then(|l| l.to_str().ok()).map(str::to_string),
                rtt_ms: round1(ms(start.elapsed())),
            }),
            Err(e) => Err(error_chain(&e)),
        }
    };
    let handshake = async {
        let host = config.web.tls_host.as_str();
        let stream = tokio::time::timeout(WEB_TIMEOUT, TcpStream::connect((host, 443))).await.map_err(|_| "connect timed out".to_string())?.map_err(|e| format!("connect to {host}: {e}"))?;
        let name = rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|e| e.to_string())?;
        let start = Instant::now();
        let stream = tokio::time::timeout(WEB_TIMEOUT, tls.connect(name, stream)).await.map_err(|_| "TLS handshake timed out".to_string())?.map_err(|e| format!("TLS to {host}: {e}"))?;
        let version = match stream.get_ref().1.protocol_version() {
            Some(rustls::ProtocolVersion::TLSv1_3) => "TLS 1.3".to_string(),
            Some(rustls::ProtocolVersion::TLSv1_2) => "TLS 1.2".to_string(),
            v => format!("{v:?}"),
        };
        Ok(TlsAnswer { rtt_ms: round1(ms(start.elapsed())), version, host: host.into() })
    };
    let sites = futures_util::future::join_all(config.web.sites.iter().map(|url| async move {
        let start = Instant::now();
        let outcome = match http.get(url).send().await {
            Ok(r) => Ok(HttpAnswer { status: r.status().as_u16(), location: None, rtt_ms: round1(ms(start.elapsed())) }),
            Err(e) => Err(error_chain(&e)),
        };
        SiteResult { url: url.clone(), outcome }
    }));
    let (portal, tls, sites) = tokio::join!(portal, handshake, sites);
    WebResult { portal_url: portal_url.to_string(), portal: Some(portal), tls: Some(tls), sites }
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

/// Finds the largest don't-fragment packet that reaches the internet, between 1280 and the local
/// MTU. `None` if nothing got through (ICMP filtered), which says nothing about the MTU.
async fn path_mtu(icmp: Option<Icmp>, local: Option<u32>) -> Option<PathMtu> {
    let icmp = icmp?;
    let target = TRACE_TARGET;
    let top = local.unwrap_or(1500).clamp(1280, 9000);
    tokio::task::spawn_blocking(move || {
        let fits = |size: u32| (0..2).any(|_| icmp.fits(target, size as usize, Duration::from_millis(500)).unwrap_or(false));
        if !fits(1280) {
            return None;
        }
        let (mut lo, mut hi) = (1280, top + 1);
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if fits(mid) { lo = mid } else { hi = mid }
        }
        Some(PathMtu { mtu: lo, target: target.to_string() })
    })
    .await
    .ok()
    .flatten()
}

pub(crate) fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

pub(crate) fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_bind_answer() {
        let mut m = vec![0x12, 0x34, 0x85, 0x80, 0, 1, 0, 1, 0, 0, 0, 0];
        m.extend(b"\x07version\x04bind\x00\x00\x10\x00\x03");
        m.extend([0xc0, 0x0c, 0, 16, 0, 3, 0, 0, 0, 0, 0, 13, 12]);
        m.extend(b"dnsmasq-2.85");
        assert_eq!(parse_txt_answer(&m, 0x1234).as_deref(), Some("dnsmasq-2.85"));
        assert_eq!(parse_txt_answer(&m, 0x9999), None);
    }
}
