//! The host's physical network ports: link state, negotiated speed, and error counters.
//!
//! On Linux this reads `/sys/class/net`, which works unprivileged and inside a container on the
//! host network. A port is physical when it has a `device` link; bridges, `veth`, `docker0`,
//! VPN tunnels, and loopback don't. Elsewhere it falls back to what `netdev` reports, without
//! counters.

use std::path::Path;

use wirewolf_core::model::Interface;

/// Addresses and flags for one interface as the OS reports them, keyed by name.
#[derive(Debug, Clone, Default)]
pub struct Addrs {
    pub name: String,
    /// CIDR strings, IPv4 then global IPv6.
    pub addresses: Vec<String>,
    pub link: bool,
    pub wireless: bool,
    pub speed_mbps: Option<u64>,
}

/// The physical ports on this host. `default` is the default-route interface name.
pub fn read(default: Option<&str>) -> Vec<Interface> {
    let addrs = os_addrs();
    let sys = Path::new("/sys/class/net");
    let ports = if sys.is_dir() { from_sysfs(sys, &addrs) } else { from_addrs(&addrs) };
    mark_default(ports, default)
}

fn os_addrs() -> Vec<Addrs> {
    netdev::get_interfaces()
        .into_iter()
        .map(|i| {
            let mut addresses: Vec<String> = i.ipv4.iter().map(|n| n.to_string()).collect();
            addresses.extend(i.ipv6.iter().filter(|n| crate::measure::is_global_v6(&n.addr())).map(|n| n.to_string()));
            Addrs {
                link: i.is_running(),
                wireless: i.if_type == netdev::prelude::InterfaceType::Wireless80211,
                speed_mbps: i.transmit_speed.filter(|s| *s > 0).map(|s| s / 1_000_000),
                addresses,
                name: i.name,
            }
        })
        .collect()
}

/// Ports from a `/sys/class/net` tree. Addresses come from `addrs`, taken from the bridge in front
/// of a port when the port itself has none (a Linux bridge `master`, or Synology's `ovs_ethN`).
pub fn from_sysfs(root: &Path, addrs: &[Addrs]) -> Vec<Interface> {
    let Ok(entries) = std::fs::read_dir(root) else { return vec![] };
    let mut names: Vec<String> = entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    let addresses_of = |name: &str| addrs.iter().find(|a| a.name == name).map(|a| a.addresses.clone()).unwrap_or_default();
    names
        .into_iter()
        .filter(|n| root.join(n).join("device").exists())
        .map(|name| {
            let dir = root.join(&name);
            let read = |file: &str| std::fs::read_to_string(dir.join(file)).ok().map(|s| s.trim().to_string());
            let num = |file: &str| read(file).and_then(|s| s.parse::<u64>().ok());
            let link = read("carrier").as_deref() == Some("1");
            let master = std::fs::read_link(dir.join("master")).ok().and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()));
            let mut addresses = addresses_of(&name);
            let mut via = None;
            if addresses.is_empty() {
                let ovs = format!("ovs_{name}");
                for bridge in [master, Some(ovs)].into_iter().flatten() {
                    let a = addresses_of(&bridge);
                    if !a.is_empty() {
                        addresses = a;
                        via = Some(bridge);
                        break;
                    }
                }
            }
            // speed reads -1 (or fails) with no link; some drivers report u32::MAX for unknown.
            let speed_mbps = num("speed").filter(|s| link && *s > 0 && *s < u64::from(u32::MAX));
            Interface {
                name,
                via,
                default_route: false,
                link,
                wireless: dir.join("wireless").exists() || dir.join("phy80211").exists(),
                speed_mbps,
                duplex: read("duplex").filter(|d| link && (d == "full" || d == "half")),
                addresses,
                rx_errors: num("statistics/rx_errors"),
                tx_errors: num("statistics/tx_errors"),
                rx_dropped: num("statistics/rx_dropped"),
                tx_dropped: num("statistics/tx_dropped"),
                carrier_changes: num("carrier_changes"),
            }
        })
        .collect()
}

/// Ports without sysfs (macOS): `en*` Ethernet and Wi-Fi interfaces, no counters.
fn from_addrs(addrs: &[Addrs]) -> Vec<Interface> {
    let mut ports: Vec<Interface> = netdev::get_interfaces()
        .into_iter()
        .filter(|i| {
            use netdev::prelude::InterfaceType as T;
            i.name.starts_with("en") && matches!(i.if_type, T::Ethernet | T::GigabitEthernet | T::FastEthernetT | T::Wireless80211) && i.mac_addr.is_some()
        })
        .filter_map(|i| addrs.iter().find(|a| a.name == i.name))
        .map(|a| Interface {
            name: a.name.clone(),
            via: None,
            default_route: false,
            link: a.link,
            wireless: a.wireless,
            speed_mbps: a.speed_mbps.filter(|_| a.link),
            duplex: None,
            addresses: a.addresses.clone(),
            rx_errors: None,
            tx_errors: None,
            rx_dropped: None,
            tx_dropped: None,
            carrier_changes: None,
        })
        .collect();
    ports.sort_by(|a, b| a.name.cmp(&b.name));
    ports
}

/// Flags the port carrying the default route (directly or through its bridge), listed first.
pub fn mark_default(mut ports: Vec<Interface>, default: Option<&str>) -> Vec<Interface> {
    for p in &mut ports {
        p.default_route = default.is_some_and(|d| p.name == d || p.via.as_deref() == Some(d));
    }
    ports.sort_by_key(|p| !p.default_route);
    ports
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway `/sys/class/net` tree.
    struct Tree(std::path::PathBuf);

    impl Tree {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("wirewolf-sysfs-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        /// A physical port. `files` are paths relative to the port directory.
        fn port(&self, name: &str, files: &[(&str, &str)]) -> &Self {
            let dir = self.0.join(name);
            std::fs::create_dir_all(dir.join("device")).unwrap();
            std::fs::create_dir_all(dir.join("statistics")).unwrap();
            for (file, value) in files {
                std::fs::write(dir.join(file), format!("{value}\n")).unwrap();
            }
            self
        }

        /// A virtual interface: no `device` link.
        fn virt(&self, name: &str) -> &Self {
            std::fs::create_dir_all(self.0.join(name)).unwrap();
            std::fs::write(self.0.join(name).join("carrier"), "1\n").unwrap();
            self
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn addrs(name: &str, a: &[&str]) -> Addrs {
        Addrs { name: name.into(), addresses: a.iter().map(|s| s.to_string()).collect(), ..Default::default() }
    }

    const UP_1G: &[(&str, &str)] = &[
        ("carrier", "1"),
        ("speed", "1000"),
        ("duplex", "full"),
        ("carrier_changes", "2"),
        ("statistics/rx_errors", "0"),
        ("statistics/tx_errors", "0"),
        ("statistics/rx_dropped", "17"),
        ("statistics/tx_dropped", "0"),
    ];
    // An unplugged port: the kernel reports carrier 0, speed -1, duplex unknown.
    const DOWN: &[(&str, &str)] = &[("carrier", "0"), ("speed", "-1"), ("duplex", "unknown"), ("carrier_changes", "1")];

    #[test]
    fn synology_nas_maps_ovs_bridge_to_its_port() {
        let t = Tree::new("nas");
        t.port("eth0", UP_1G).port("eth1", DOWN).port("eth2", DOWN).port("eth3", DOWN);
        t.virt("ovs_eth0").virt("ovs_eth1").virt("lo").virt("docker0").virt("veth1a2b3c");
        let a = [addrs("ovs_eth0", &["192.0.2.20/24", "2001:db8::20/64"]), addrs("lo", &["127.0.0.1/8"]), addrs("docker0", &["198.51.100.1/24"])];
        let ports = mark_default(from_sysfs(&t.0, &a), Some("ovs_eth0"));
        let names: Vec<_> = ports.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["eth0", "eth1", "eth2", "eth3"]);
        let eth0 = &ports[0];
        assert!(eth0.default_route && eth0.link);
        assert_eq!(eth0.via.as_deref(), Some("ovs_eth0"));
        assert_eq!(eth0.addresses, ["192.0.2.20/24", "2001:db8::20/64"]);
        assert_eq!((eth0.speed_mbps, eth0.duplex.as_deref()), (Some(1000), Some("full")));
        assert_eq!((eth0.rx_dropped, eth0.carrier_changes), (Some(17), Some(2)));
        let eth1 = &ports[1];
        assert!(!eth1.link && !eth1.default_route);
        assert_eq!((eth1.speed_mbps, eth1.duplex.as_deref(), eth1.via.as_deref()), (None, None, None));
        assert!(eth1.addresses.is_empty());
    }

    #[test]
    fn bridge_master_supplies_addresses() {
        let t = Tree::new("bridge");
        t.port("enp3s0", &[("carrier", "1"), ("speed", "100"), ("duplex", "half")]).virt("br0");
        std::os::unix::fs::symlink(t.0.join("br0"), t.0.join("enp3s0").join("master")).unwrap();
        let ports = mark_default(from_sysfs(&t.0, &[addrs("br0", &["192.0.2.5/24"])]), Some("br0"));
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].via.as_deref(), Some("br0"));
        assert!(ports[0].default_route);
        assert_eq!((ports[0].speed_mbps, ports[0].duplex.as_deref()), (Some(100), Some("half")));
        // Missing counter files read as unknown, not zero.
        assert_eq!(ports[0].rx_errors, None);
    }

    #[test]
    fn unknown_speed_is_none() {
        let t = Tree::new("unknown");
        t.port("eth0", &[("carrier", "1"), ("speed", "4294967295")]).port("wlan0", &[("carrier", "1")]);
        std::fs::create_dir_all(t.0.join("wlan0").join("wireless")).unwrap();
        let ports = from_sysfs(&t.0, &[]);
        assert_eq!(ports[0].speed_mbps, None);
        assert!(ports[1].wireless && !ports[0].wireless);
    }
}
