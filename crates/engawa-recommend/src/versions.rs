//! Kubernetes versions: the support window of each minor release, and whether the cluster should
//! be upgraded.
//!
//! # End of life comes from a bundled table
//!
//! [`RELEASES`] is Kubernetes' own schedule, copied from `kubernetes/website`
//! `data/releases/schedule.yaml` and `eol.yaml` on [`DATA_AS_OF`] [k8s-schedule] [k8s-eol]. Each
//! minor gets about 14 months of patches: 12 months of standard support, then about two months
//! of "maintenance mode" with critical fixes only, ending on the 28th [k8s-releases]. The
//! dates are fixed months ahead, so the warning that matters most for security, "end of life
//! soon", stays correct without any network access. "Soon" is the start of maintenance mode:
//! the project's own two-month signal, in the spirit of GKE's end-of-support notifications
//! [gke-notifications].
//! A minor older than the table is past its end of life; one newer than the table is reported
//! as such ([`UpgradeState::NewerThanData`]), never as an error.
//!
//! # Newer versions come from the cluster
//!
//! Which patch or minor is newest is not in the table: it changes weekly. Engawa makes no
//! outbound request for it. When system-upgrade-controller is installed, its `Plan` objects
//! carry `status.latestVersion`, which the controller resolved from the channel the cluster
//! administrator chose, and that is the newer version Engawa reports. Without Plans, newer
//! versions are not known and nothing is said about them; end of life and version skew are facts
//! about what runs and are always reported.
//!
//! # Version skew
//!
//! The kubelet "must not be newer than kube-apiserver" and "may be up to three minor versions
//! older" [k8s-skew]: anything else is unsupported. Nodes on different minors within that window
//! are allowed, and reported as information, since it usually means an upgrade is half done.
//!
//! [k8s-schedule]: https://github.com/kubernetes/website/blob/main/data/releases/schedule.yaml
//! [k8s-eol]: https://github.com/kubernetes/website/blob/main/data/releases/eol.yaml
//! [k8s-releases]: https://kubernetes.io/releases/
//! [k8s-skew]: https://kubernetes.io/releases/version-skew-policy/
//! [gke-notifications]: https://docs.cloud.google.com/kubernetes-engine/docs/concepts/cluster-notifications

use std::cmp::Ordering;
use std::fmt;

use serde::Serialize;
use utoipa::ToSchema;

/// When [`RELEASES`] was copied from the Kubernetes schedule.
pub const DATA_AS_OF: &str = "2026-10-14";

/// One Kubernetes minor release and its support window. Dates are `YYYY-MM-DD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// `1.34`.
    pub minor: &'static str,
    /// Its first release.
    pub released: &'static str,
    /// From this day only critical fixes are made ("maintenance mode").
    pub maintenance_from: &'static str,
    /// The last day it gets patches.
    pub end_of_life: &'static str,
}

/// Kubernetes minors still supported or recently ended, newest first.
///
/// From `schedule.yaml` and `eol.yaml` (see the module documentation). 1.33's release and
/// maintenance dates follow the
/// same rule (maintenance two months before end of life, released April 2025) and are not
/// otherwise used: 1.33 is past its end of life.
pub const RELEASES: &[Release] = &[
    Release {
        minor: "1.37",
        released: "2026-08-26",
        maintenance_from: "2027-08-28",
        end_of_life: "2027-10-28",
    },
    Release {
        minor: "1.36",
        released: "2026-04-22",
        maintenance_from: "2027-04-28",
        end_of_life: "2027-06-28",
    },
    Release {
        minor: "1.35",
        released: "2025-12-17",
        maintenance_from: "2026-12-28",
        end_of_life: "2027-02-28",
    },
    Release {
        minor: "1.34",
        released: "2025-08-27",
        maintenance_from: "2026-08-27",
        end_of_life: "2026-10-27",
    },
    Release {
        minor: "1.33",
        released: "2025-04-23",
        maintenance_from: "2026-04-28",
        end_of_life: "2026-06-28",
    },
];

/// A Kubernetes version as a distribution builds it.
///
/// `v1.36.5+k3s1`, `v1.36.5-k3s1` (the form
/// system-upgrade-controller writes, as image tags cannot hold `+`), `v1.36.5+rke2r1`,
/// `v1.36.5`. The distribution's own build number orders builds of one upstream patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// Major.
    pub major: u32,
    /// Minor.
    pub minor: u32,
    /// Patch.
    pub patch: u32,
    /// The distribution's build (`k3s2` is 2), 0 when there is none.
    pub build: u32,
}

impl Version {
    /// Parses a version, or `None` when it is not `[v]MAJOR.MINOR[.PATCH][(+|-)BUILD]`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        let (core, build) = text
            .split_once(['+', '-'])
            .map_or((text, ""), |(core, build)| (core, build));
        let mut parts = core.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next().map_or(Some(0), |p| p.parse().ok())?;
        if parts.next().is_some() {
            return None;
        }
        let build = ["k3s", "rke2r"]
            .iter()
            .find_map(|prefix| build.strip_prefix(prefix))
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        Some(Self {
            major,
            minor,
            patch,
            build,
        })
    }

    /// `1.34`.
    #[must_use]
    pub fn minor_label(self) -> String {
        format!("{}.{}", self.major, self.minor)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// What the cluster's version calls for. An open set: clients must ignore values they do not
/// know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum UpgradeState {
    /// A newer build of the same minor exists.
    PatchAvailable,
    /// A newer supported minor exists; upgrade one minor at a time.
    MinorAvailable,
    /// The minor gets critical fixes only, and reaches its end of life soon.
    Maintenance,
    /// The minor gets no fixes any more.
    EndOfLife,
    /// A kubelet is newer than kube-apiserver, or more than three minors older.
    SkewUnsupported,
    /// Nodes run different minors, within the supported window.
    SkewMixed,
    /// system-upgrade-controller is upgrading nodes now.
    Upgrading,
    /// The minor is newer than Engawa's table.
    NewerThanData,
}

impl UpgradeState {
    /// How much it matters.
    #[must_use]
    pub const fn severity(self) -> Severity {
        match self {
            Self::EndOfLife | Self::SkewUnsupported => Severity::Danger,
            Self::Maintenance => Severity::Warning,
            Self::PatchAvailable | Self::MinorAvailable | Self::SkewMixed | Self::Upgrading => {
                Severity::Info
            }
            Self::NewerThanData => Severity::None,
        }
    }
}

/// How much a state matters, from nothing to act on to act now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// Nothing to do.
    None,
    /// Worth knowing.
    Info,
    /// Plan to act.
    Warning,
    /// Act now.
    Danger,
}

/// A node whose kubelet minor differs from kube-apiserver's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeSkew {
    /// The node.
    pub node: String,
    /// Its kubelet's version.
    pub kubelet_version: String,
    /// `true` when outside the supported window.
    pub unsupported: bool,
}

/// What a provider of node updates says about one upgrade it manages (system-upgrade-controller's
/// `Plan`: `status.latestVersion` and `status.applying`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlanStatus {
    /// The version the upgrade resolved to.
    pub latest_version: Option<String>,
    /// The nodes being upgraded now.
    pub applying: Vec<String>,
}

/// What [`assess`] needs.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    /// kube-apiserver's `gitVersion`.
    pub current: &'a str,
    /// Today, `YYYY-MM-DD` (UTC).
    pub today: &'a str,
    /// Each node and its kubelet's version; empty when nodes cannot be read.
    pub kubelets: &'a [(String, String)],
    /// The upgrades a node-updates provider manages; empty when there is none or they cannot be
    /// read.
    pub plans: &'a [PlanStatus],
}

/// The upgrade picture of the cluster's own version.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Assessment {
    /// `1.34`, when the version parses.
    pub minor: Option<String>,
    /// The table's row for that minor.
    pub release: Option<Release>,
    /// Every state that applies, most severe first.
    pub states: Vec<UpgradeState>,
    /// A newer build of the same minor, from a Plan.
    pub patch: Option<String>,
    /// The next minor's build, from a Plan.
    pub next_minor: Option<String>,
    /// Nodes whose kubelet minor differs from kube-apiserver's.
    pub skew: Vec<NodeSkew>,
    /// Nodes system-upgrade-controller is upgrading now.
    pub upgrading: Vec<String>,
}

/// The table's row for `minor` (`1.34`).
#[must_use]
pub fn release(minor: &str) -> Option<Release> {
    RELEASES.iter().find(|r| r.minor == minor).copied()
}

/// The order of two minors (`1.34`), as [`Version::parse`] reads them.
fn minor_order(a: &str, b: &str) -> Ordering {
    Version::parse(a).cmp(&Version::parse(b))
}

/// The cluster's upgrade picture from its version, the table, the nodes and the Plans.
#[must_use]
pub fn assess(inputs: &Inputs<'_>) -> Assessment {
    let mut out = Assessment::default();
    let Some(current) = Version::parse(inputs.current) else {
        return out;
    };
    let minor = current.minor_label();
    out.release = release(&minor);
    match out.release {
        Some(row) if inputs.today > row.end_of_life => out.states.push(UpgradeState::EndOfLife),
        Some(row) if inputs.today >= row.maintenance_from => {
            out.states.push(UpgradeState::Maintenance);
        }
        Some(_) => {}
        None => match (RELEASES.first(), RELEASES.last()) {
            (Some(newest), _) if minor_order(&minor, newest.minor) == Ordering::Greater => {
                out.states.push(UpgradeState::NewerThanData);
            }
            (_, Some(oldest)) if minor_order(&minor, oldest.minor) == Ordering::Less => {
                out.states.push(UpgradeState::EndOfLife);
            }
            _ => {}
        },
    }
    out.minor = Some(minor);

    skew(current, inputs.kubelets, &mut out);
    out.upgrading = inputs
        .plans
        .iter()
        .flat_map(|p| p.applying.iter().cloned())
        .collect();
    out.upgrading.sort();
    out.upgrading.dedup();
    if !out.upgrading.is_empty() {
        out.states.push(UpgradeState::Upgrading);
    }

    newer(current, inputs.plans, &mut out);
    out.states
        .sort_by(|a, b| b.severity().cmp(&a.severity()).then(a.cmp(b)));
    out
}

fn skew(current: Version, kubelets: &[(String, String)], out: &mut Assessment) {
    let mut minors = std::collections::BTreeSet::new();
    let mut unsupported = false;
    for (node, version) in kubelets {
        let Some(kubelet) = Version::parse(version) else {
            continue;
        };
        minors.insert((kubelet.major, kubelet.minor));
        if (kubelet.major, kubelet.minor) == (current.major, current.minor) {
            continue;
        }
        let bad = kubelet.major != current.major
            || kubelet.minor > current.minor
            || current.minor - kubelet.minor > 3;
        unsupported |= bad;
        out.skew.push(NodeSkew {
            node: node.clone(),
            kubelet_version: version.clone(),
            unsupported: bad,
        });
    }
    if unsupported {
        out.states.push(UpgradeState::SkewUnsupported);
    } else if minors.len() > 1 {
        out.states.push(UpgradeState::SkewMixed);
    }
}

/// Patch or next minor, from the newest version a Plan resolved.
fn newer(current: Version, plans: &[PlanStatus], out: &mut Assessment) {
    let resolved = plans
        .iter()
        .filter_map(|p| p.latest_version.as_deref())
        .filter_map(|v| Some((Version::parse(v)?, v)))
        .max_by_key(|(v, _)| *v);
    if let Some((target, text)) = resolved
        && target > current
    {
        if (target.major, target.minor) == (current.major, current.minor) {
            out.patch = Some(text.to_owned());
            out.states.push(UpgradeState::PatchAvailable);
        } else {
            out.next_minor = Some(text.to_owned());
            out.states.push(UpgradeState::MinorAvailable);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(current: &str) -> Inputs<'_> {
        Inputs {
            current,
            today: "2026-10-14",
            kubelets: &[],
            plans: &[],
        }
    }

    #[test]
    fn versions_parse_in_every_distribution_spelling_and_order_by_build() {
        let k3s = Version::parse("v1.34.12+k3s1").unwrap();
        assert_eq!((k3s.minor, k3s.patch, k3s.build), (34, 12, 1));
        assert_eq!(Version::parse("v1.34.12-k3s1"), Some(k3s));
        assert!(Version::parse("v1.34.12+k3s2").unwrap() > k3s);
        assert_eq!(Version::parse("1.36.5+rke2r1").unwrap().build, 1);
        assert_eq!(Version::parse("v1.36").unwrap().patch, 0);
        assert_eq!(Version::parse("v1.33.4-eks-1").unwrap().build, 0);
        assert!(Version::parse("latest").is_none());
        assert!(Version::parse("v1.2.3.4").is_none());
    }

    #[test]
    fn the_table_is_newest_first_and_dates_are_ordered() {
        for pair in RELEASES.windows(2) {
            let [newer, older] = pair else { unreachable!() };
            assert_eq!(minor_order(newer.minor, older.minor), Ordering::Greater);
        }
        for row in RELEASES {
            assert!(row.released < row.maintenance_from && row.maintenance_from < row.end_of_life);
        }
        assert_eq!(release("1.34").unwrap().end_of_life, "2026-10-27");
    }

    #[test]
    fn end_of_life_comes_from_the_table() {
        let i = inputs("v1.34.12+k3s1");
        let a = assess(&i);
        assert_eq!(a.states, [UpgradeState::Maintenance]);
        assert!(a.patch.is_none() && a.next_minor.is_none());
        let after = assess(&Inputs {
            today: "2026-10-28",
            ..i
        });
        assert_eq!(after.states, [UpgradeState::EndOfLife]);
        assert_eq!(assess(&inputs("v1.31.4")).states, [UpgradeState::EndOfLife]);
        assert_eq!(
            assess(&inputs("v1.38.0")).states,
            [UpgradeState::NewerThanData]
        );
    }

    #[test]
    fn plans_give_the_newer_version_and_show_nodes_in_progress() {
        let plans = [PlanStatus {
            latest_version: Some("v1.36.5-k3s1".to_owned()),
            applying: vec!["node-2".to_owned()],
        }];
        let a = assess(&Inputs {
            plans: &plans,
            ..inputs("v1.35.9+k3s1")
        });
        assert_eq!(a.next_minor.as_deref(), Some("v1.36.5-k3s1"));
        assert!(a.patch.is_none());
        assert_eq!(a.upgrading, ["node-2"]);
        assert!(a.states.contains(&UpgradeState::Upgrading));
        assert!(a.states.contains(&UpgradeState::MinorAvailable));

        let patch = assess(&Inputs {
            plans: &plans,
            ..inputs("v1.36.4+k3s1")
        });
        assert_eq!(patch.patch.as_deref(), Some("v1.36.5-k3s1"));
        assert!(patch.states.contains(&UpgradeState::PatchAvailable));

        let current = assess(&Inputs {
            plans: &plans,
            ..inputs("v1.36.5+k3s1")
        });
        assert!(current.patch.is_none() && current.next_minor.is_none());
        assert!(!current.states.contains(&UpgradeState::PatchAvailable));
    }

    #[test]
    fn kubelets_newer_or_too_old_are_unsupported_and_mixed_minors_noted() {
        let nodes = |versions: &[&str]| -> Vec<(String, String)> {
            versions
                .iter()
                .enumerate()
                .map(|(i, v)| (format!("node-{i}"), (*v).to_owned()))
                .collect()
        };
        let mixed = nodes(&["v1.36.5+k3s1", "v1.35.9+k3s1"]);
        let a = assess(&Inputs {
            kubelets: &mixed,
            ..inputs("v1.36.5+k3s1")
        });
        assert_eq!(a.states, [UpgradeState::SkewMixed]);
        assert_eq!(a.skew.len(), 1);
        let newer = nodes(&["v1.37.0+k3s1"]);
        let b = assess(&Inputs {
            kubelets: &newer,
            ..inputs("v1.36.5+k3s1")
        });
        assert_eq!(b.states, [UpgradeState::SkewUnsupported]);
        let old = nodes(&["v1.32.0"]);
        let c = assess(&Inputs {
            kubelets: &old,
            ..inputs("v1.36.5")
        });
        assert!(c.skew[0].unsupported);
    }
}
