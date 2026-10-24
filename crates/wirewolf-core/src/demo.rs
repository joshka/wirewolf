//! A scripted driver that publishes realistic data into a [`Hub`] without touching the network.
//!
//! Used by `wirewolf serve --demo`, by UI development, and by screenshots. Each scenario shapes
//! the live path; 24 hours of history (samples, speed runs, and a few past incidents) is
//! backfilled at startup so every view has something to show.

use std::time::Duration;

use jiff::{Timestamp, ToSpan};

use crate::diagnose::{diagnose, with_targets};
use crate::hub::{Command, Hub};
use crate::model::*;

/// Live update interval.
const TICK: Duration = Duration::from_secs(2);

pub fn scenarios() -> Vec<Scenario> {
    let s = |id: &str, name: &str, description: &str| Scenario {
        id: id.into(),
        name: name.into(),
        description: description.into(),
    };
    vec![
        s("healthy", "Healthy", "Fiber connection, everything passing."),
        s("wifi_loss", "Wi-Fi loss", "The local link drops 8–15% of packets."),
        s("isp_outage", "ISP outage", "The router answers but the provider's network is down."),
        s("dns_down", "DNS down", "The internet answers by IP but the resolver times out."),
        s("captive_portal", "Captive portal", "HTTP is intercepted by a login page."),
        s("bufferbloat", "Bufferbloat", "Latency spikes under load; speed tests grade poorly."),
        s("slow_link", "Slow link", "The host's Ethernet port negotiated 100 Mbps instead of gigabit."),
        s("flapping", "Flapping", "Alternates between healthy and an ISP outage every 40s."),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Healthy,
    WifiLoss,
    IspOutage,
    DnsDown,
    CaptivePortal,
    Bufferbloat,
    SlowLink,
}

fn kind_at(scenario: &str, elapsed: Duration) -> Kind {
    match scenario {
        "wifi_loss" => Kind::WifiLoss,
        "isp_outage" => Kind::IspOutage,
        "dns_down" => Kind::DnsDown,
        "captive_portal" => Kind::CaptivePortal,
        "bufferbloat" => Kind::Bufferbloat,
        "slow_link" => Kind::SlowLink,
        "flapping" if (elapsed.as_secs() / 40) % 2 == 1 => Kind::IspOutage,
        _ => Kind::Healthy,
    }
}

pub fn meta(scenario: &str) -> Meta {
    Meta {
        version: env!("CARGO_PKG_VERSION").into(),
        api_version: API_VERSION.into(),
        mode: "demo".into(),
        host: "wirewolf-demo".into(),
        scenarios: scenarios(),
        scenario: Some(scenario.into()),
    }
}

/// Backfills history, then publishes live data until the task is dropped.
pub async fn run(hub: Hub, scenario: String) {
    let mut scenario = scenario;
    let mut commands = hub.take_commands().expect("demo driver needs the command receiver");
    hub.set_config(config().view());
    backfill(&hub, Timestamp::now());
    let mut started = tokio::time::Instant::now();
    let mut ticker = tokio::time::interval(TICK);
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let kind = kind_at(&scenario, started.elapsed());
                publish(&hub, Timestamp::now(), kind);
            }
            Some(command) = commands.recv() => match command {
                Command::SetScenario(id) if scenarios().iter().any(|s| s.id == id) => {
                    scenario = id;
                    started = tokio::time::Instant::now();
                    hub.update_meta(|m| m.scenario = Some(scenario.clone()));
                    let kind = kind_at(&scenario, Duration::ZERO);
                    publish(&hub, Timestamp::now(), kind);
                }
                Command::SetScenario(_) => {}
                Command::RunSpeedTest if !hub.speed_running() => {
                    let kind = kind_at(&scenario, started.elapsed());
                    tokio::spawn(speed_test(hub.clone(), kind));
                }
                Command::RunSpeedTest | Command::ApplyConfig(_) => {}
            }
        }
    }
}

fn publish(hub: &Hub, at: Timestamp, kind: Kind) {
    let hops = hops(kind);
    for hop in &hops {
        hub.push_sample(sample(at, hop));
    }
    let targets = targets(kind);
    for t in &targets {
        hub.push_sample(target_sample(at, t));
    }
    let verdict = with_targets(diagnose(&hops), &targets);
    hub.publish_snapshot(Snapshot { at, verdict, hops, open_incident: None, targets });
}

/// The snapshot a scenario shows at its start, without publishing it. For tests and examples.
pub fn snapshot(scenario: &str, at: Timestamp) -> Snapshot {
    let kind = kind_at(scenario, Duration::ZERO);
    let (hops, targets) = (hops(kind), targets(kind));
    let verdict = with_targets(diagnose(&hops), &targets);
    Snapshot { at, verdict, hops, open_incident: None, targets }
}

/// The demo's config: the defaults plus a few user targets, as if from a file.
pub fn config() -> crate::config::Loaded {
    use crate::config::{Config, Loaded, Source, UserTarget};
    let t = |name: &str, check: &str, critical: bool| UserTarget { name: name.into(), check: check.into(), critical, verify_tls: true };
    let config = Config {
        targets: vec![
            t("NAS", "tcp://nas.local:445", true),
            UserTarget { verify_tls: false, ..t("Router admin", "https://router.local", false) },
            t("Printer", "ping://10.0.0.5", false),
            t("Pi-hole", "dns://pihole.local", false),
        ],
        ..Config::default()
    };
    let sources = [("targets".to_string(), Source::File)].into();
    Loaded { config, path: Some("wirewolf.toml (demo)".into()), sources }
}

fn targets(kind: Kind) -> Vec<Target> {
    let lossy = kind == Kind::WifiLoss;
    let t = |id: &str, name: &str, check: &str, kind: &str, critical: bool, base: f64| {
        let loss = if lossy { [0.0, 0.0, 20.0, 40.0][fastrand::usize(..4)] } else { 0.0 };
        let rtt = round1(jitter(if lossy { base * 4.0 } else { base }, base / 3.0));
        let status = if loss >= 10.0 { Status::Warn } else { Status::Pass };
        let detail = match kind {
            "tcp" => format!("TCP :445 in {rtt:.1} ms"),
            "https" => format!("200 OK in {rtt:.0} ms"),
            "ping" if loss > 0.0 => format!("{rtt:.1} ms, {loss:.0}% loss"),
            "ping" => format!("{rtt:.1} ms, no loss"),
            _ => format!("one.one.one.one → 1.1.1.1 in {rtt:.1} ms"),
        };
        Target {
            id: id.into(),
            name: name.into(),
            check: check.into(),
            kind: kind.into(),
            critical,
            status,
            detail,
            rtt_ms: Some(rtt),
            loss_pct: Some(loss),
        }
    };
    vec![
        t("target:nas", "NAS", "tcp://nas.local:445", "tcp", true, 1.6),
        t("target:router-admin", "Router admin", "https://router.local", "https", false, 9.0),
        t("target:printer", "Printer", "ping://10.0.0.5", "ping", false, 4.0),
        t("target:pi-hole", "Pi-hole", "dns://pihole.local", "dns", false, 2.2),
    ]
}

fn target_sample(at: Timestamp, t: &Target) -> Sample {
    Sample { at, hop: t.id.clone(), status: t.status, rtt_ms: t.rtt_ms, jitter_ms: None, loss_pct: t.loss_pct }
}

fn sample(at: Timestamp, hop: &Hop) -> Sample {
    Sample {
        at,
        hop: hop.id.clone(),
        status: hop.status,
        rtt_ms: hop.metrics.rtt_ms,
        jitter_ms: hop.metrics.jitter_ms,
        loss_pct: hop.metrics.loss_pct,
    }
}

fn jitter(base: f64, spread: f64) -> f64 {
    (base + (fastrand::f64() - 0.5) * 2.0 * spread).max(0.1)
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn ev(key: &str, value: impl ToString) -> Evidence {
    Evidence { key: key.into(), value: value.to_string() }
}

fn check(name: &str, status: Status, detail: impl Into<String>, rtt_ms: Option<f64>) -> Check {
    Check { name: name.into(), status, detail: detail.into(), rtt_ms }
}

fn summary(m: &HopMetrics) -> String {
    match (m.rtt_ms, m.loss_pct) {
        (Some(rtt), Some(loss)) if loss >= 0.5 => format!("{rtt:.1} ms, {loss:.0}% loss"),
        (Some(rtt), _) => format!("{rtt:.1} ms, no loss"),
        (None, Some(loss)) => format!("{loss:.0}% loss"),
        (None, None) => "no response".into(),
    }
}

/// Builds the path for a scenario, with noise.
fn hops(kind: Kind) -> Vec<Hop> {
    let bloat = kind == Kind::Bufferbloat;
    let wifi_loss = if kind == Kind::WifiLoss { round1(jitter(11.5, 3.5)) } else { 0.0 };

    // Host: a four-port machine with one cable plugged in.
    let slow = kind == Kind::SlowLink;
    let port = |name: &str, link: bool| Interface {
        name: name.into(),
        via: None,
        default_route: false,
        link,
        wireless: false,
        speed_mbps: link.then_some(if slow { 100 } else { 1000 }),
        duplex: link.then(|| "full".into()),
        addresses: vec![],
        rx_errors: Some(0),
        tx_errors: Some(0),
        rx_dropped: Some(if link { 112 } else { 0 }),
        tx_dropped: Some(0),
        carrier_changes: Some(if link { 2 } else { 0 }),
    };
    let mut checks = vec![
        check("Interface up", Status::Pass, if slow { "eth0, 100 Mbps" } else { "eth0, 1 Gbps" }, None),
        check("IPv4 address", Status::Pass, "192.168.1.42/24 via DHCP", None),
        check("IPv6 address", Status::Pass, "2001:db8:4f2a::42/64", None),
    ];
    if slow {
        checks.push(check("eth0 speed", Status::Warn, "eth0 linked at 100 Mbps", None));
    }
    let host = Hop {
        id: "host".into(),
        kind: HopKind::Host,
        label: "This host".into(),
        target: Some("eth0 · 192.168.1.42/24".into()),
        status: if slow { Status::Warn } else { Status::Pass },
        summary: if slow { "eth0 linked at 100 Mbps".into() } else { "eth0 up, 192.168.1.42".into() },
        metrics: HopMetrics::default(),
        evidence: vec![
            ev("interface", "eth0"),
            ev("ipv4", "192.168.1.42/24"),
            ev("ipv6", "2001:db8:4f2a::42/64"),
            ev("mtu", 1500),
            ev("public ip", "203.0.113.77"),
            ev("cgnat", "no"),
        ],
        checks,
        interfaces: vec![
            Interface { default_route: true, addresses: vec!["192.168.1.42/24".into(), "2001:db8:4f2a::42/64".into()], ..port("eth0", true) },
            port("eth1", false),
            port("eth2", false),
            port("eth3", false),
        ],
    };

    // Gateway
    let gw_rtt = round1(jitter(if wifi_loss > 0.0 { 6.0 } else { 1.4 }, if wifi_loss > 0.0 { 3.0 } else { 0.4 }));
    let gw_metrics = HopMetrics {
        rtt_ms: Some(gw_rtt),
        jitter_ms: Some(round1(jitter(if wifi_loss > 0.0 { 4.0 } else { 0.2 }, 0.1))),
        loss_pct: Some(wifi_loss),
    };
    let gateway = Hop {
        id: "gateway".into(),
        kind: HopKind::Gateway,
        label: "Gateway".into(),
        target: Some("192.168.1.1".into()),
        status: if wifi_loss >= 2.0 { Status::Warn } else { Status::Pass },
        summary: summary(&gw_metrics),
        metrics: gw_metrics,
        evidence: vec![
            ev("address", "192.168.1.1"),
            ev("mac vendor", "Ubiquiti"),
            ev("method", "ICMP echo × 20"),
        ],
        checks: vec![check("ICMP echo", if wifi_loss >= 2.0 { Status::Warn } else { Status::Pass }, format!("20 sent, {:.0}% lost", wifi_loss), Some(gw_rtt))],
        interfaces: vec![],
    };

    let isp_down = kind == Kind::IspOutage;

    // ISP edge
    let isp_metrics = if isp_down {
        HopMetrics { rtt_ms: None, jitter_ms: None, loss_pct: Some(100.0) }
    } else {
        HopMetrics {
            rtt_ms: Some(round1(gw_rtt + jitter(if bloat { 38.0 } else { 3.1 }, if bloat { 30.0 } else { 0.6 }))),
            jitter_ms: Some(round1(jitter(if bloat { 22.0 } else { 0.4 }, 0.2))),
            loss_pct: Some(wifi_loss),
        }
    };
    let isp = Hop {
        id: "isp".into(),
        kind: HopKind::Isp,
        label: "ISP".into(),
        target: Some("100.64.0.1 (AS64500 Example Fiber)".into()),
        status: if isp_down { Status::Fail } else if bloat { Status::Warn } else { Status::Pass },
        summary: summary(&isp_metrics),
        metrics: isp_metrics,
        evidence: if isp_down {
            vec![
                ev("method", "ICMP TTL sweep"),
                ev("hop 1", "192.168.1.1 · 1.2 ms"),
                ev("hop 2", "* * * (no reply)"),
                ev("hop 3", "* * * (no reply)"),
            ]
        } else {
            vec![ev("method", "ICMP TTL 2"), ev("asn", "AS64500 Example Fiber"), ev("pop", "SYD")]
        },
        checks: vec![check(
            "First ISP hop",
            if isp_down { Status::Fail } else { Status::Pass },
            if isp_down { "no reply from hop 2 within 4s" } else { "100.64.0.1 answers" },
            isp_metrics.rtt_ms,
        )],
        interfaces: vec![],
    };

    // Internet
    let downstream_skip = isp_down;
    let targets = [("Cloudflare", "1.1.1.1:443", 0.0), ("Google", "8.8.8.8:443", 1.3), ("Quad9", "9.9.9.9:443", 2.1)];
    let inet_base = isp.metrics.rtt_ms.unwrap_or(0.0) + 4.5;
    let inet_checks: Vec<Check> = targets
        .iter()
        .flat_map(|(name, addr, extra)| {
            let rtt = round1(jitter(inet_base + extra, if bloat { 25.0 } else { 0.8 }));
            let v6 = round1(rtt + 0.6);
            [
                if downstream_skip {
                    check(&format!("{name} v4"), Status::Skip, format!("{addr} — skipped, ISP down"), None)
                } else {
                    check(&format!("{name} v4"), Status::Pass, format!("TCP handshake to {addr}"), Some(rtt))
                },
                if downstream_skip {
                    check(&format!("{name} v6"), Status::Skip, "skipped, ISP down", None)
                } else {
                    check(&format!("{name} v6"), Status::Pass, "TCP handshake over IPv6", Some(v6))
                },
            ]
        })
        .collect();
    let inet_metrics = if downstream_skip {
        HopMetrics::default()
    } else {
        let best = inet_checks.iter().filter_map(|c| c.rtt_ms).fold(f64::MAX, f64::min);
        HopMetrics {
            rtt_ms: Some(best),
            jitter_ms: Some(round1(jitter(if bloat { 18.0 } else { 0.5 }, 0.3))),
            loss_pct: Some(wifi_loss),
        }
    };
    let internet = Hop {
        id: "internet".into(),
        kind: HopKind::Internet,
        label: "Internet".into(),
        target: Some("Cloudflare · Google · Quad9".into()),
        status: if downstream_skip { Status::Skip } else if bloat { Status::Warn } else { Status::Pass },
        summary: if downstream_skip { "skipped — ISP is down".into() } else { summary(&inet_metrics) },
        metrics: inet_metrics,
        evidence: vec![ev("method", "TCP :443 handshake × 5 per target"), ev("families", "IPv4 + IPv6")],
        checks: inet_checks,
        interfaces: vec![],
    };

    // DNS
    let dns_down = kind == Kind::DnsDown;
    let dns_rtt = round1(jitter(if bloat { 45.0 } else { 11.0 }, 3.0));
    let dns_checks = if downstream_skip {
        vec![
            check("System resolver", Status::Skip, "192.168.1.1 — skipped", None),
            check("Cloudflare 1.1.1.1", Status::Skip, "skipped", None),
            check("Google 8.8.8.8", Status::Skip, "skipped", None),
        ]
    } else if dns_down {
        vec![
            check("System resolver", Status::Fail, "192.168.1.1 — timed out after 4s (3 tries)", None),
            check("Cloudflare 1.1.1.1", Status::Pass, "example.com → 93.184.215.14", Some(dns_rtt)),
            check("Google 8.8.8.8", Status::Pass, "example.com → 93.184.215.14", Some(round1(dns_rtt + 2.0))),
        ]
    } else {
        vec![
            check("System resolver", Status::Pass, "192.168.1.1 — example.com → 93.184.215.14", Some(round1(dns_rtt + 1.2))),
            check("Cloudflare 1.1.1.1", Status::Pass, "example.com → 93.184.215.14", Some(dns_rtt)),
            check("Google 8.8.8.8", Status::Pass, "example.com → 93.184.215.14", Some(round1(dns_rtt + 2.0))),
        ]
    };
    let dns_metrics = if downstream_skip || dns_down {
        HopMetrics { rtt_ms: None, jitter_ms: None, loss_pct: if dns_down { Some(100.0) } else { None } }
    } else {
        HopMetrics { rtt_ms: Some(round1(dns_rtt + 1.2)), jitter_ms: Some(round1(jitter(1.0, 0.5))), loss_pct: Some(0.0) }
    };
    let dns = Hop {
        id: "dns".into(),
        kind: HopKind::Dns,
        label: "DNS".into(),
        target: Some("192.168.1.1".into()),
        status: if downstream_skip { Status::Skip } else if dns_down { Status::Fail } else if bloat { Status::Warn } else { Status::Pass },
        summary: if downstream_skip {
            "skipped — ISP is down".into()
        } else if dns_down {
            "system resolver timing out; public resolvers fine".into()
        } else {
            format!("{:.1} ms via system resolver", dns_rtt + 1.2)
        },
        metrics: dns_metrics,
        evidence: vec![ev("resolvers", "192.168.1.1 (from /etc/resolv.conf)"), ev("query", "example.com A/AAAA")],
        checks: dns_checks,
        interfaces: vec![],
    };

    // Web
    let portal = kind == Kind::CaptivePortal;
    let web_skip = downstream_skip || dns_down;
    let web_rtt = round1(jitter(if bloat { 120.0 } else { 38.0 }, 8.0));
    let web = Hop {
        id: "web".into(),
        kind: HopKind::Web,
        label: "Web".into(),
        target: Some("cp.cloudflare.com".into()),
        status: if web_skip { Status::Skip } else if portal { Status::Fail } else { Status::Pass },
        summary: if web_skip {
            "skipped — no name resolution".into()
        } else if portal {
            "HTTP redirected to a login page".into()
        } else {
            format!("HTTPS in {web_rtt:.0} ms")
        },
        metrics: HopMetrics { rtt_ms: (!web_skip).then_some(web_rtt), jitter_ms: None, loss_pct: None },
        evidence: if portal {
            vec![ev("http status", "302 Found"), ev("location", "http://login.hotspot.example/?mac=…")]
        } else {
            vec![ev("http status", "204 No Content"), ev("tls", "TLS 1.3, cert valid 61 days")]
        },
        checks: vec![
            if web_skip {
                check("Captive portal", Status::Skip, "skipped", None)
            } else if portal {
                check("Captive portal", Status::Fail, "expected 204, got 302 → login.hotspot.example", Some(web_rtt))
            } else {
                check("Captive portal", Status::Pass, "204 from cp.cloudflare.com", Some(web_rtt))
            },
            if web_skip || portal {
                check("TLS handshake", Status::Skip, "skipped", None)
            } else {
                check("TLS handshake", Status::Pass, "TLS 1.3 to www.cloudflare.com", Some(round1(web_rtt * 0.7)))
            },
        ],
        interfaces: vec![],
    };

    vec![host, gateway, isp, internet, dns, web]
}

fn speed_result(kind: Kind, id: String, started: Timestamp) -> SpeedRun {
    let bloat = kind == Kind::Bufferbloat;
    let idle = round1(jitter(9.0, 1.0));
    let loaded = round1(idle + if bloat { jitter(240.0, 60.0) } else { jitter(6.0, 3.0) });
    let slow = kind == Kind::SlowLink;
    let down = round1(jitter(if slow { 93.0 } else if bloat { 310.0 } else { 920.0 }, if slow { 2.0 } else { 30.0 }));
    let up = round1(jitter(if slow { 91.0 } else if bloat { 45.0 } else { 880.0 }, if slow { 2.0 } else { 25.0 }));
    SpeedRun {
        id,
        started,
        state: SpeedState::Complete,
        backend: "demo".into(),
        server: Some("Cloudflare SYD".into()),
        down_mbps: Some(down),
        up_mbps: Some(up),
        idle_latency_ms: Some(idle),
        loaded_latency_ms: Some(loaded),
        bloat: Some(BloatGrade::from_increase_ms(loaded - idle)),
        // About 4s of transfer in each direction.
        bytes_used: Some(((down + up) * 1_000_000.0 / 8.0 * 4.0) as u64),
        progress: Some(1.0),
        error: None,
    }
}

async fn speed_test(hub: Hub, kind: Kind) {
    let started = Timestamp::now();
    let id = format!("speed-{}", started.as_millisecond());
    let finished = speed_result(kind, id.clone(), started);
    if kind == Kind::IspOutage {
        hub.upsert_speed(SpeedRun {
            state: SpeedState::Failed,
            down_mbps: None,
            up_mbps: None,
            idle_latency_ms: None,
            loaded_latency_ms: None,
            bloat: None,
            bytes_used: Some(0),
            progress: None,
            error: Some("could not reach speed.cloudflare.com".into()),
            ..finished
        });
        return;
    }
    // 10s: latency, then download ramp, then upload ramp.
    for step in 0..=20u32 {
        let progress = f64::from(step) / 20.0;
        let down_phase = ((progress - 0.1) / 0.45).clamp(0.0, 1.0);
        let up_phase = ((progress - 0.55) / 0.45).clamp(0.0, 1.0);
        hub.upsert_speed(SpeedRun {
            state: SpeedState::Running,
            down_mbps: (down_phase > 0.0).then(|| round1(finished.down_mbps.unwrap() * down_phase.sqrt())),
            up_mbps: (up_phase > 0.0).then(|| round1(finished.up_mbps.unwrap() * up_phase.sqrt())),
            loaded_latency_ms: (down_phase > 0.0).then_some(finished.loaded_latency_ms.unwrap()),
            bloat: None,
            bytes_used: Some((finished.bytes_used.unwrap() as f64 * progress) as u64),
            progress: Some(progress),
            ..finished.clone()
        });
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    hub.upsert_speed(finished);
}

/// Fills 24 hours of history ending at `now`: one sample per hop per minute, hourly speed runs,
/// and three past incidents.
fn backfill(hub: &Hub, now: Timestamp) {
    let start = now - 24.hours();
    // (start minute offset, duration minutes, kind)
    let episodes = [(4 * 60 + 12, 25, Kind::WifiLoss), (15 * 60 + 40, 6, Kind::IspOutage), (21 * 60 + 5, 3, Kind::DnsDown)];
    let kind_at_minute = |m: i64| {
        episodes
            .iter()
            .find(|(s, d, _)| m >= *s && m < s + d)
            .map(|(_, _, k)| *k)
            // A congested evening: bufferbloat from 19:00 to 21:00 relative to start.
            .unwrap_or(if (19 * 60..21 * 60).contains(&m) { Kind::Bufferbloat } else { Kind::Healthy })
    };

    let mut samples = Vec::new();
    let mut incidents = Vec::new();
    let mut open: Option<Incident> = None;
    for minute in 0..24 * 60 {
        let at = start + minute.minutes();
        let kind = kind_at_minute(minute);
        let hops = hops(kind);
        samples.extend(hops.iter().map(|h| sample(at, h)));
        samples.extend(targets(kind).iter().map(|t| target_sample(at, t)));
        let verdict = diagnose(&hops);
        let bad = matches!(verdict.severity, Severity::Down | Severity::Degraded) && kind != Kind::Bufferbloat;
        match (open.take(), bad) {
            (None, true) => {
                let id = format!("inc-past-{}", incidents.len() + 1);
                open = Some(Incident {
                    id: id.clone(),
                    started: at,
                    ended: None,
                    severity: verdict.severity,
                    headline: verdict.headline.clone(),
                    blame: verdict.blame.clone(),
                    evidence: Snapshot { at, verdict, hops, open_incident: Some(id), targets: targets(kind) },
                });
            }
            (Some(incident), true) => open = Some(incident),
            (Some(mut incident), false) => {
                incident.ended = Some(at);
                incidents.push(incident);
            }
            (None, false) => {}
        }
    }
    incidents.extend(open);

    let speed = (0..24)
        .map(|h| {
            let at = start + (h * 60 + 30).minutes();
            speed_result(kind_at_minute(h * 60 + 30), format!("speed-past-{h}"), at)
        })
        .collect();

    hub.load_history(samples, speed, incidents);
}
