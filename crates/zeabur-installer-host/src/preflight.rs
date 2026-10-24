//! Checks before anything is changed.

use std::fs;
use std::net::TcpListener;
use std::path::Path;

use serde::Serialize;
use zeabur_installer_plan::rke2::node_ports;

use crate::cmd;

/// How a check came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// Fine.
    Pass,
    /// Worth reading; does not stop `apply`.
    Warn,
    /// Stops `apply`.
    Fail,
}

/// One check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    /// What was checked.
    pub name: &'static str,
    /// The outcome.
    pub status: Status,
    /// What was found and what to do.
    pub detail: String,
    /// The outcome as a stable key, such as `ports.busy`, for an interface that words it itself.
    pub code: &'static str,
    /// The values the detail names, in order: paths, sizes, ports.
    pub values: Vec<String>,
}

/// Whether a disk is encrypted, as far as the block devices tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encryption {
    /// A dm-crypt device is under it.
    Encrypted,
    /// No dm-crypt device is under it.
    Plain,
    /// The devices could not be read.
    Unknown,
}

/// What the checks need to know.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// RKE2 from an earlier `apply` is running, so its ports are expected to be in use.
    pub rke2_running: bool,
    /// Fail on a disk that is not encrypted.
    pub require_encrypted_disk: bool,
    /// An RKE2 server, which also listens on the API and supervisor ports; otherwise an agent.
    pub server: bool,
}

/// Files left by other Kubernetes distributions.
const OTHERS: [(&str, &str); 5] = [
    ("/usr/local/bin/k3s", "k3s"),
    ("/etc/rancher/k3s", "k3s"),
    ("/etc/kubernetes/admin.conf", "kubeadm"),
    ("/var/lib/k0s", "k0s"),
    ("/snap/bin/microk8s", "MicroK8s"),
];

const DATA_DIR: &str = "/var/lib/rancher";
const DISK_NEEDED_GIB: u64 = 10;
const DISK_ROOMY_GIB: u64 = 30;

/// Runs every check.
#[must_use]
pub fn run(options: Options) -> Vec<Check> {
    let mut checks = vec![
        root(),
        systemd(),
        time_sync(),
        swap(),
        other_distributions(),
    ];
    if !options.rke2_running {
        checks.push(ports(options.server));
    }
    checks.push(disk_space());
    checks.push(encryption(options.require_encrypted_disk));
    checks
}

fn check(name: &'static str, status: Status, detail: impl Into<String>) -> Check {
    Check::new(name, status, detail)
}

impl Check {
    pub(crate) fn new(name: &'static str, status: Status, detail: impl Into<String>) -> Self {
        Self {
            name,
            status,
            detail: detail.into(),
            code: "",
            values: Vec::new(),
        }
    }

    pub(crate) fn code(mut self, code: &'static str, values: &[&str]) -> Self {
        self.code = code;
        self.values = values.iter().map(|&v| v.to_owned()).collect();
        self
    }
}

/// Whether this process runs as root.
#[must_use]
pub fn is_root() -> bool {
    effective_uid(&fs::read_to_string("/proc/self/status").unwrap_or_default()) == Some(0)
}

fn root() -> Check {
    if is_root() {
        check("root", Status::Pass, "running as root").code("root.ok", &[])
    } else {
        check("root", Status::Fail, "run apply as root (sudo)").code("root.no", &[])
    }
}

fn effective_uid(status: &str) -> Option<u32> {
    let line = status.lines().find(|l| l.starts_with("Uid:"))?;
    line.split_whitespace().nth(2)?.parse().ok()
}

fn systemd() -> Check {
    if Path::new("/run/systemd/system").is_dir() {
        check("systemd", Status::Pass, "systemd is the init system").code("systemd.ok", &[])
    } else {
        check(
            "systemd",
            Status::Fail,
            "RKE2 runs as a systemd service; this host does not run systemd",
        )
        .code("systemd.no", &[])
    }
}

fn time_sync() -> Check {
    match cmd::run(
        "timedatectl",
        &["show", "--property=NTPSynchronized", "--value"],
        None,
    ) {
        Ok(out) if out.trim() == "yes" => check("time", Status::Pass, "the clock is synchronised").code("time.ok", &[]),
        Ok(_) => check(
            "time",
            Status::Warn,
            "the clock is not synchronised; certificates and tokens fail when it drifts (timedatectl set-ntp true)",
        )
        .code("time.drift", &[]),
        Err(_) => check(
            "time",
            Status::Warn,
            "could not ask timedatectl whether the clock is synchronised",
        )
        .code("time.unknown", &[]),
    }
}

fn swap() -> Check {
    match fs::read_to_string("/proc/swaps") {
        Ok(swaps) if swaps.lines().skip(1).any(|l| !l.trim().is_empty()) => check(
            "swap",
            Status::Warn,
            "swap is on; the kubelet expects it off (swapoff -a, and remove it from /etc/fstab)",
        )
        .code("swap.on", &[]),
        Ok(_) => check("swap", Status::Pass, "swap is off").code("swap.off", &[]),
        Err(_) => {
            check("swap", Status::Warn, "could not read /proc/swaps").code("swap.unknown", &[])
        }
    }
}

fn other_distributions() -> Check {
    let found: Vec<String> = OTHERS
        .iter()
        .filter(|(path, _)| Path::new(path).exists())
        .map(|(path, name)| format!("{name} ({path})"))
        .collect();
    if found.is_empty() {
        check("other Kubernetes", Status::Pass, "none found").code("others.none", &[])
    } else {
        check(
            "other Kubernetes",
            Status::Fail,
            format!(
                "found {}; uninstall it first, two distributions fight over the same ports and iptables",
                found.join(", ")
            ),
        )
        .code("others.found", &[&found.join(", ")])
    }
}

fn ports(server: bool) -> Check {
    let ports = node_ports(server);
    let busy: Vec<&(u16, &str)> = ports
        .iter()
        .filter(|(port, _)| TcpListener::bind(("0.0.0.0", *port)).is_err())
        .collect();
    if busy.is_empty() {
        let numbers: Vec<String> = ports.iter().map(|(port, _)| port.to_string()).collect();
        check(
            "ports",
            Status::Pass,
            format!("{} are free", and_list(&numbers)),
        )
        .code("ports.free", &[])
    } else {
        let named: Vec<String> = busy
            .iter()
            .map(|(port, what)| format!("{port} ({what})"))
            .collect();
        let numbers: Vec<String> = busy.iter().map(|(port, _)| port.to_string()).collect();
        check(
            "ports",
            Status::Fail,
            format!("in use: {}; `ss -ltnp` shows by what", named.join(", ")),
        )
        .code("ports.busy", &[&numbers.join(", ")])
    }
}

/// `a, b and c`.
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn disk_space() -> Check {
    let target = if Path::new(DATA_DIR).exists() {
        DATA_DIR
    } else {
        "/var/lib"
    };
    let Some(kib) = cmd::run("df", &["-Pk", target], None)
        .ok()
        .and_then(|out| available_kib(&out))
    else {
        return check(
            "disk space",
            Status::Warn,
            format!("could not read the free space of {target}"),
        )
        .code("disk.unknown", &[target]);
    };
    let gib = gib(kib);
    if kib < DISK_NEEDED_GIB << 20 {
        check(
            "disk space",
            Status::Fail,
            format!("{gib} GiB free under {target}; RKE2's images and data need at least {DISK_NEEDED_GIB} GiB"),
        )
        .code("disk.low", &[&gib, target, &DISK_NEEDED_GIB.to_string()])
    } else if kib < DISK_ROOMY_GIB << 20 {
        check(
            "disk space",
            Status::Warn,
            format!(
                "{gib} GiB free under {target}; {DISK_ROOMY_GIB} GiB or more leaves room for logs, metrics and apps"
            ),
        )
        .code("disk.tight", &[&gib, target, &DISK_ROOMY_GIB.to_string()])
    } else {
        check(
            "disk space",
            Status::Pass,
            format!("{gib} GiB free under {target}"),
        )
        .code("disk.ok", &[&gib, target])
    }
}

/// KiB as GiB with one decimal, rounded down.
fn gib(kib: u64) -> String {
    let tenths = kib * 10 / (1 << 20);
    format!("{}.{}", tenths / 10, tenths % 10)
}

fn available_kib(df: &str) -> Option<u64> {
    df.lines().nth(1)?.split_whitespace().nth(3)?.parse().ok()
}

fn encryption(required: bool) -> Check {
    let target = if Path::new(DATA_DIR).exists() {
        DATA_DIR
    } else {
        "/var/lib"
    };
    let found = disk_encryption(target);
    match (found, required) {
        (Encryption::Encrypted, _) => check(
            "disk encryption",
            Status::Pass,
            format!("{target} is on a dm-crypt device"),
        )
        .code("crypt.yes", &[target]),
        (Encryption::Plain, true) => check(
            "disk encryption",
            Status::Fail,
            format!("{target} is not on an encrypted device, and host.requireEncryptedDisk is set"),
        )
        .code("crypt.requiredPlain", &[target]),
        (Encryption::Unknown, true) => check(
            "disk encryption",
            Status::Fail,
            format!(
                "could not tell whether {target} is encrypted, and host.requireEncryptedDisk is set"
            ),
        )
        .code("crypt.requiredUnknown", &[target]),
        (Encryption::Plain, false) => check(
            "disk encryption",
            Status::Warn,
            format!(
                "{target} is not on an encrypted device; Secrets are encrypted in etcd, volumes are not"
            ),
        )
        .code("crypt.no", &[target]),
        (Encryption::Unknown, false) => check(
            "disk encryption",
            Status::Warn,
            format!("could not tell whether {target} is encrypted"),
        )
        .code("crypt.unknown", &[target]),
    }
}

/// Whether the device holding `path` has a dm-crypt device beneath it.
#[must_use]
pub fn disk_encryption(path: &str) -> Encryption {
    let Ok(source) = cmd::run(
        "findmnt",
        &["--noheadings", "--output", "SOURCE", "--target", path],
        None,
    ) else {
        return Encryption::Unknown;
    };
    let source = source.trim();
    // Btrfs subvolumes read as `/dev/sda2[/@]`.
    let device = source.split('[').next().unwrap_or(source);
    match cmd::run(
        "lsblk",
        &["--noheadings", "--inverse", "--output", "TYPE", device],
        None,
    ) {
        Ok(types) if types.split_whitespace().any(|t| t == "crypt") => Encryption::Encrypted,
        Ok(_) => Encryption::Plain,
        Err(_) => Encryption::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing() {
        assert_eq!(effective_uid("Name:\tx\nUid:\t1000\t0\t0\t0\n"), Some(0));
        assert_eq!(effective_uid("Uid:\t0\t1000\t1000\t1000\n"), Some(1000));
        let df = "Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/sda2 61255492 9032 49094708 16% /\n";
        assert_eq!(available_kib(df), Some(49_094_708));
        assert_eq!(gib(3_250_586), "3.1");
        assert_eq!(gib(10 << 20), "10.0");
        let ports = |server| -> Vec<String> {
            node_ports(server)
                .iter()
                .map(|(p, _)| p.to_string())
                .collect()
        };
        assert_eq!(and_list(&ports(false)), "80, 443 and 10250");
        assert_eq!(and_list(&ports(true)), "80, 443, 6443, 9345 and 10250");
    }
}
