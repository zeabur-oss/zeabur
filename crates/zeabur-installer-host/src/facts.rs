//! Facts about the host the plan depends on.

use std::fs;

use zeabur_installer_plan::plan::Facts;

use crate::HostError;

/// Reads the architecture, host name and memory.
///
/// # Errors
///
/// When `/proc` cannot be read.
pub fn read() -> Result<Facts, HostError> {
    let hostname = fs::read_to_string("/proc/sys/kernel/hostname")
        .map_err(HostError::io("/proc/sys/kernel/hostname"))?;
    let meminfo = fs::read_to_string("/proc/meminfo").map_err(HostError::io("/proc/meminfo"))?;
    Ok(Facts {
        arch: arch(std::env::consts::ARCH).to_owned(),
        hostname: hostname.trim().to_owned(),
        memory_mib: memory_mib(&meminfo).ok_or_else(|| HostError::Parse {
            what: "/proc/meminfo".to_owned(),
            reason: "no MemTotal".to_owned(),
        })?,
        node_ips: node_ips(),
    })
}

/// A global address of the host, as `ip -o addr` shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostAddress {
    /// The interface.
    pub interface: String,
    /// The address, without its prefix length.
    pub address: String,
    /// An IPv6 privacy address, which the kernel replaces about once a day (RFC 8981).
    pub temporary: bool,
    /// Past its preferred lifetime, or not yet usable (tentative, failed duplicate detection).
    pub unusable: bool,
    /// Configured by the system, not learnt from router advertisements or DHCP.
    pub permanent: bool,
}

impl HostAddress {
    /// An IPv6 address.
    #[must_use]
    pub fn is_v6(&self) -> bool {
        self.address.contains(':')
    }
}

/// The host's global addresses with their flags.
#[must_use]
pub fn host_addresses() -> Vec<HostAddress> {
    parse_addresses(
        &crate::cmd::run("ip", &["-o", "addr", "show", "scope", "global"], None)
            .unwrap_or_default(),
    )
}

fn parse_addresses(text: &str) -> Vec<HostAddress> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let interface = words.nth(1)?.trim_end_matches(':').to_owned();
            if !matches!(words.next()?, "inet" | "inet6") {
                return None;
            }
            let address = words.next()?.split('/').next()?.to_owned();
            let flags: Vec<&str> = words
                .take_while(|w| *w != "\\" && *w != "valid_lft")
                .collect();
            let has = |f: &str| flags.contains(&f);
            Some(HostAddress {
                interface,
                address,
                temporary: has("temporary"),
                unusable: has("deprecated") || has("tentative") || has("dadfailed"),
                permanent: !has("dynamic"),
            })
        })
        .collect()
}

/// The interface of the first default route, `-4` or `-6`.
fn default_interface(family: &str) -> Option<String> {
    let routes = crate::cmd::run("ip", &[family, "-o", "route", "show", "default"], None).ok()?;
    route_interface(&routes)
}

fn route_interface(routes: &str) -> Option<String> {
    let line = routes.lines().next()?;
    let mut words = line.split_whitespace();
    words.find(|w| *w == "dev")?;
    words.next().map(str::to_owned)
}

/// The addresses RKE2 registers the node with, never a temporary IPv6 address.
///
/// These are the IPv4 and the IPv6 address of the interfaces with default routes, as RKE2 picks
/// them itself. RKE2 takes the first IPv6 address it finds, which is often the temporary one; it
/// changes within a day, and the node's address in the network policies goes stale. A configured
/// address comes first, then one from SLAAC (EUI-64 or stable-privacy), which keeps its value.
#[must_use]
pub fn node_ips() -> Vec<String> {
    choose_node_ips(
        &host_addresses(),
        default_interface("-4").as_deref(),
        default_interface("-6").as_deref(),
    )
}

/// [`node_ips`] from what it reads.
#[must_use]
pub fn choose_node_ips(
    addresses: &[HostAddress],
    v4_interface: Option<&str>,
    v6_interface: Option<&str>,
) -> Vec<String> {
    let usable = |a: &&HostAddress, v6: bool, interface: &str| {
        a.is_v6() == v6 && a.interface == interface && !a.unusable && !a.temporary
    };
    let v4 = v4_interface.and_then(|i| addresses.iter().find(|a| usable(a, false, i)));
    let v6 = v6_interface.and_then(|i| {
        let stable: Vec<&HostAddress> = addresses.iter().filter(|a| usable(a, true, i)).collect();
        stable
            .iter()
            .find(|a| a.permanent)
            .or_else(|| stable.first())
            .copied()
    });
    v4.into_iter()
        .chain(v6)
        .map(|a| a.address.clone())
        .collect()
}

/// Rust's architecture name as RKE2 spells it.
#[must_use]
pub fn arch(rust: &str) -> &str {
    match rust {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

fn memory_mib(meminfo: &str) -> Option<u64> {
    let line = meminfo.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib / 1024)
}

/// The host's global addresses, IPv4 first, from `ip`; empty when it cannot run.
#[must_use]
pub fn addresses() -> Vec<String> {
    let mut found: Vec<String> =
        crate::cmd::run("ip", &["-o", "addr", "show", "scope", "global"], None)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                line.split_whitespace()
                    .nth(3)?
                    .split('/')
                    .next()
                    .map(str::to_owned)
            })
            .collect();
    found.sort_by_key(|a| a.contains(':'));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const IP_ADDR: &str = "\
2: enp0s5    inet 192.0.2.121/24 brd 192.0.2.255 scope global dynamic noprefixroute enp0s5\\       valid_lft 1107sec preferred_lft 1107sec
11: flannel.1    inet 10.42.5.0/32 scope global flannel.1\\       valid_lft forever preferred_lft forever
2: enp0s5    inet6 fdb2:2c26:f4e4:0:9b2d:5a24:581b:f432/64 scope global temporary dynamic \\       valid_lft 172109sec preferred_lft 85390sec
2: enp0s5    inet6 fdb2:2c26:f4e4:0:3ad8:c781:797:77a5/64 scope global dynamic mngtmpaddr noprefixroute \\       valid_lft 2591869sec preferred_lft 604669sec
";

    #[test]
    fn node_ips_leave_temporary_addresses_out() {
        let addresses = parse_addresses(IP_ADDR);
        assert_eq!(addresses.len(), 4);
        assert_eq!(addresses[1].interface, "flannel.1");
        assert!(addresses[2].temporary && !addresses[3].temporary);
        assert_eq!(
            choose_node_ips(&addresses, Some("enp0s5"), Some("enp0s5")),
            ["192.0.2.121", "fdb2:2c26:f4e4:0:3ad8:c781:797:77a5"]
        );
        assert_eq!(
            choose_node_ips(&addresses, Some("enp0s5"), None),
            ["192.0.2.121"]
        );
        let only_temporary: Vec<HostAddress> = addresses
            .iter()
            .filter(|a| !a.address.ends_with("77a5"))
            .cloned()
            .collect();
        assert_eq!(
            choose_node_ips(&only_temporary, Some("enp0s5"), Some("enp0s5")),
            ["192.0.2.121"]
        );
    }

    #[test]
    fn a_configured_ipv6_address_comes_first() {
        let addresses = parse_addresses(
            "2: eth0    inet6 2001:db8::aa/64 scope global dynamic mngtmpaddr \\       valid_lft 1sec preferred_lft 1sec\n\
             2: eth0    inet6 2001:db8::5/64 scope global \\       valid_lft forever preferred_lft forever\n\
             2: eth0    inet6 2001:db8::6/64 scope global deprecated \\       valid_lft forever preferred_lft 0sec\n",
        );
        assert_eq!(
            choose_node_ips(&addresses, None, Some("eth0")),
            ["2001:db8::5"]
        );
        assert_eq!(
            route_interface("default via fe80::1 dev eth0 proto ra metric 100 pref medium\n"),
            Some("eth0".to_owned())
        );
        assert_eq!(route_interface(""), None);
    }

    #[test]
    fn meminfo() {
        assert_eq!(
            memory_mib("MemTotal:        3995216 kB\nMemFree: 1 kB\n"),
            Some(3901)
        );
        assert_eq!(memory_mib("MemFree: 1 kB\n"), None);
        assert_eq!(arch("aarch64"), "arm64");
    }
}
