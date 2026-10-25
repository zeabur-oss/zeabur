//! Preparing a node for a component that needs host packages, kernel modules and services:
//! Longhorn's iSCSI initiator, NFS client, `iscsid` and `iscsi_tcp`.
//!
//! Each step looks first and changes only what is missing, so running it again changes nothing.

use std::fs;
use std::path::Path;

use zeabur_installer_plan::storage::{
    MODULES_FILE, MULTIPATH_BLACKLIST, MULTIPATH_FILE, NodePrep, Package,
};

use crate::preflight::{Check, Code, Status, Topic};
use crate::{HostError, cmd, files, systemd};

/// Directories programs such as `iscsiadm` and `mount.nfs` install into.
const SBIN: &[&str] = &["/usr/sbin", "/sbin", "/usr/bin", "/bin"];

/// A family of distributions sharing a package manager and package names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Debian, Ubuntu: apt.
    Debian,
    /// RHEL, Rocky, Alma, Oracle Linux, Fedora: dnf.
    Rhel,
    /// SLES, openSUSE: zypper.
    Suse,
}

impl Family {
    /// The package's name in this family.
    #[must_use]
    pub const fn package(self, package: &Package) -> &'static str {
        match self {
            Self::Debian => package.debian,
            Self::Rhel => package.rhel,
            Self::Suse => package.suse,
        }
    }
}

/// The family from `/etc/os-release`'s `ID` and `ID_LIKE`.
#[must_use]
pub fn family(os_release: &str) -> Option<Family> {
    let mut ids = Vec::new();
    for line in os_release.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key == "ID" || key == "ID_LIKE" {
            ids.extend(
                value
                    .trim_matches('"')
                    .split_whitespace()
                    .map(str::to_ascii_lowercase),
            );
        }
    }
    ids.iter().find_map(|id| match id.as_str() {
        "debian" | "ubuntu" => Some(Family::Debian),
        "rhel" | "fedora" | "centos" | "rocky" | "almalinux" | "ol" => Some(Family::Rhel),
        "suse" | "sles" | "opensuse" | "opensuse-leap" | "opensuse-tumbleweed" => {
            Some(Family::Suse)
        }
        _ => None,
    })
}

fn has_binary(name: &str) -> bool {
    SBIN.iter().any(|dir| Path::new(dir).join(name).exists())
}

fn module_loaded(name: &str) -> bool {
    Path::new("/sys/module").join(name).exists()
}

fn multipathd_running() -> bool {
    systemd::is_active("multipathd")
}

/// How a missing item counts: before `apply` prepares the node it is only news; afterwards it is
/// broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// `plan` and `apply`'s preflight: `apply` installs what is missing.
    Before,
    /// `doctor`: something removed it, or `apply` never ran with this configuration.
    After,
}

/// Checks the node against `prep`.
#[must_use]
pub fn check(prep: &NodePrep, stage: Stage) -> Vec<Check> {
    let (missing, fix) = match stage {
        Stage::Before => (Status::Warn, "apply installs it"),
        Stage::After => (Status::Fail, "zeabur-installer apply installs it"),
    };
    let after = stage == Stage::After;
    let component = prep.component;
    let mut checks = Vec::new();
    for package in prep.packages {
        let binary = package.binary;
        checks.push(if has_binary(binary) {
            Check::new(
                Topic::NodePackages,
                Status::Pass,
                format!("{binary} is installed ({})", package.purpose),
                Code::PackageOk,
            )
            .values(&[binary])
        } else {
            Check::new(
                Topic::NodePackages,
                missing,
                format!("{component} needs {binary} ({}); {fix}", package.purpose),
                if after {
                    Code::PackageMissing
                } else {
                    Code::PackageToInstall
                },
            )
            .values(&[binary, component])
        });
    }
    for module in prep.modules {
        checks.push(if module_loaded(module) {
            Check::new(
                Topic::KernelModules,
                Status::Pass,
                format!("{module} is loaded"),
                Code::ModuleOk,
            )
            .values(&[module])
        } else {
            Check::new(
                Topic::KernelModules,
                missing,
                format!("{component} needs the {module} kernel module loaded; {fix}"),
                if after {
                    Code::ModuleMissing
                } else {
                    Code::ModuleToLoad
                },
            )
            .values(&[module, component])
        });
    }
    for service in prep.services {
        checks.push(if systemd::is_active(service) {
            Check::new(
                Topic::Services,
                Status::Pass,
                format!("{service} is running"),
                Code::ServiceOk,
            )
            .values(&[service])
        } else {
            Check::new(
                Topic::Services,
                missing,
                format!("{component} needs {service} running; {fix}"),
                if after {
                    Code::ServiceStopped
                } else {
                    Code::ServiceToStart
                },
            )
            .values(&[service, component])
        });
    }
    if multipathd_running() && !Path::new(MULTIPATH_FILE).exists() {
        checks.push(
            Check::new(
                Topic::Multipathd,
                missing,
                format!(
                    "multipathd is running and would claim {component}'s iSCSI disks; {fix} \
                     ({MULTIPATH_FILE})"
                ),
                if after {
                    Code::MultipathOpen
                } else {
                    Code::MultipathToBlock
                },
            )
            .values(&[component, MULTIPATH_FILE]),
        );
    }
    checks
}

/// Installs the missing packages, loads the modules now and at boot, starts the services and keeps
/// multipathd off the iSCSI disks. Returns what it changed, one line each.
///
/// # Errors
///
/// When the distribution is not one it knows and a package is missing, or a command fails.
pub fn prepare(prep: &NodePrep) -> Result<Vec<String>, HostError> {
    let mut changed = Vec::new();
    let missing: Vec<&Package> = prep
        .packages
        .iter()
        .filter(|p| !has_binary(p.binary))
        .collect();
    if !missing.is_empty() {
        let os_release = fs::read_to_string("/etc/os-release").unwrap_or_default();
        let Some(family) = family(&os_release) else {
            let names: Vec<String> = missing
                .iter()
                .map(|p| format!("{} (Debian: {}, RHEL: {})", p.binary, p.debian, p.rhel))
                .collect();
            return Err(HostError::Command {
                command: "installing packages".to_owned(),
                reason: format!(
                    "this distribution is not Debian, RHEL or SUSE-like; install {} yourself and \
                     run apply again",
                    names.join(", ")
                ),
            });
        };
        let names: Vec<&str> = missing.iter().map(|p| family.package(p)).collect();
        install(family, &names)?;
        changed.push(format!("installed {}", names.join(", ")));
    }

    let modules: String = prep.modules.iter().flat_map(|m| [*m, "\n"]).collect();
    if !modules.is_empty() && files::write(Path::new(MODULES_FILE), 0o644, modules.as_bytes())? {
        changed.push(format!("wrote {MODULES_FILE}"));
    }
    for module in prep.modules {
        if !module_loaded(module) {
            cmd::run("modprobe", &[module], None).map_err(|e| HostError::Command {
                command: format!("modprobe {module}"),
                reason: format!(
                    "{e}; on SUSE, kernel-default-base lacks it: install kernel-default and reboot"
                ),
            })?;
            changed.push(format!("loaded {module}"));
        }
    }
    for service in prep.services {
        if !systemd::is_active(service) || !systemd::is_enabled(service) {
            systemd::enable_now(service)?;
            changed.push(format!("started {service}"));
        }
    }
    if multipathd_running()
        && files::write(
            Path::new(MULTIPATH_FILE),
            0o644,
            MULTIPATH_BLACKLIST.as_bytes(),
        )?
    {
        systemd::restart("multipathd")?;
        changed.push(format!("wrote {MULTIPATH_FILE} and restarted multipathd"));
    }
    Ok(changed)
}

fn install(family: Family, names: &[&str]) -> Result<(), HostError> {
    match family {
        Family::Debian => {
            let env = [("DEBIAN_FRONTEND", "noninteractive")];
            cmd::run_with("apt-get", &["update", "-qq"], &env, None)?;
            let mut args = vec!["install", "-y", "-qq", "--no-install-recommends"];
            args.extend(names);
            cmd::run_with("apt-get", &args, &env, None)?;
        }
        Family::Rhel => {
            let mut args = vec!["install", "-y", "-q"];
            args.extend(names);
            cmd::run("dnf", &args, None)?;
        }
        Family::Suse => {
            let mut args = vec!["--non-interactive", "--quiet", "install"];
            args.extend(names);
            cmd::run("zypper", &args, None)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use zeabur_installer_plan::storage::LONGHORN_PREP;

    use super::*;

    #[test]
    fn families_from_os_release() {
        assert_eq!(
            family("NAME=\"Ubuntu\"\nID=ubuntu\nID_LIKE=debian\n"),
            Some(Family::Debian)
        );
        assert_eq!(
            family("ID=\"rocky\"\nID_LIKE=\"rhel centos fedora\"\n"),
            Some(Family::Rhel)
        );
        assert_eq!(
            family("ID=\"ol\"\nID_LIKE=\"fedora\"\n"),
            Some(Family::Rhel)
        );
        assert_eq!(
            family("ID=\"sles\"\nID_LIKE=\"suse\"\n"),
            Some(Family::Suse)
        );
        assert_eq!(
            family("ID=\"opensuse-leap\"\nID_LIKE=\"suse opensuse\"\n"),
            Some(Family::Suse)
        );
        assert_eq!(family("ID=alpine\n"), None);
        assert_eq!(family(""), None);
    }

    #[test]
    fn package_names_per_family() {
        let iscsi = &LONGHORN_PREP.packages[0];
        assert_eq!(Family::Debian.package(iscsi), "open-iscsi");
        assert_eq!(Family::Rhel.package(iscsi), "iscsi-initiator-utils");
        assert_eq!(Family::Suse.package(iscsi), "open-iscsi");
        let nfs = &LONGHORN_PREP.packages[1];
        assert_eq!(Family::Suse.package(nfs), "nfs-client");
    }

    #[test]
    fn missing_items_warn_before_apply_and_fail_after() {
        let before = check(&LONGHORN_PREP, Stage::Before);
        let after = check(&LONGHORN_PREP, Stage::After);
        assert_eq!(before.len(), after.len());
        for (b, a) in before.iter().zip(&after) {
            match b.status {
                Status::Pass => assert_eq!(a.status, Status::Pass),
                Status::Warn => assert_eq!(a.status, Status::Fail, "{}", a.detail),
                Status::Fail => panic!("{}", b.detail),
            }
        }
    }
}
