//! The install record that `apply` keeps in `kube-system/zeabur-installer`, and what the next
//! `apply` changes against it.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::Value;

use crate::PlanError;
use crate::catalog::Index;
use crate::plan::{ComponentPlan, ReleasePlan};
use crate::select::RENAMED;

/// This installer's version.
pub const INSTALLER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The file in the installer's state directory that keeps the version of the installer that last
/// applied, for when the cluster is stopped and its record cannot be read.
pub const INSTALLER_FILE: &str = "installer-version";

/// What an earlier `apply` recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The version of the installer that wrote it.
    pub installer: String,
    /// The components it installed.
    pub components: BTreeSet<String>,
    /// The K3s, RKE2 or `KubeSolo` release.
    pub version: Option<String>,
    /// Chart versions by component and release name.
    pub charts: BTreeMap<(String, String), String>,
    /// The components with a `HelmChart` release, which `apply` uninstalls when they leave.
    pub helm: BTreeSet<String>,
}

impl Record {
    /// Reads the `data` of the `ConfigMap`. `None` when it has no component list.
    #[must_use]
    pub fn from_data(data: &Value) -> Option<Self> {
        let text = |key: &str| data.get(key).and_then(Value::as_str);
        let components = text("components")?
            .split(',')
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect();
        let plan: Value = text("plan")
            .and_then(|p| serde_json::from_str(p).ok())
            .unwrap_or_default();
        let mut charts = BTreeMap::new();
        let mut helm = BTreeSet::new();
        for component in plan
            .get("components")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(id) = component.get("id").and_then(Value::as_str) else {
                continue;
            };
            for release in component
                .get("releases")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if release.get("kind").and_then(Value::as_str) == Some("helmChart") {
                    helm.insert(id.to_owned());
                }
                if let (Some(name), Some(version)) = (
                    release.get("name").and_then(Value::as_str),
                    release
                        .pointer("/object/spec/version")
                        .and_then(Value::as_str),
                ) {
                    charts.insert((id.to_owned(), name.to_owned()), version.to_owned());
                }
            }
        }
        Some(Self {
            installer: text("installer").unwrap_or_default().to_owned(),
            components,
            version: text("version")
                .or_else(|| text("kubesolo"))
                .map(str::to_owned),
            charts,
            helm,
        })
    }

    /// Stops when a newer installer wrote the record.
    ///
    /// # Errors
    ///
    /// [`PlanError::NewerInstaller`].
    pub fn check_installer(&self) -> Result<(), PlanError> {
        check_installer(&self.installer)
    }
}

/// Stops when `recorded` is newer than this installer.
///
/// # Errors
///
/// [`PlanError::NewerInstaller`].
pub fn check_installer(recorded: &str) -> Result<(), PlanError> {
    if precedence(recorded, INSTALLER_VERSION) == Some(Ordering::Greater) {
        return Err(PlanError::NewerInstaller {
            recorded: recorded.to_owned(),
            own: INSTALLER_VERSION.to_owned(),
        });
    }
    Ok(())
}

/// What `apply` changes against the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    /// The version of the installer that wrote the record.
    pub installer: String,
    /// Components it installs that the record does not have.
    pub added: Vec<String>,
    /// Recorded components it no longer plans and uninstalls.
    pub removed: Vec<String>,
    /// Recorded components it no longer plans that have no chart to uninstall; what they
    /// installed keeps running.
    pub kept: Vec<String>,
    /// Recorded components the catalog renamed, which the new component takes over.
    pub replaced: Vec<Replaced>,
    /// Charts whose version changes.
    pub charts: Vec<ChartChange>,
    /// Default components of this catalog that the record does not have and `enable` does not
    /// name; `apply` leaves them out.
    pub offered: Vec<String>,
    /// The K3s, RKE2 or `KubeSolo` release, when it changes.
    pub version: Option<VersionChange>,
    /// Host files whose content changes; `apply` fills it.
    pub host_files: Vec<String>,
    /// `apply` restarts K3s, RKE2 or `KubeSolo`.
    pub restart: bool,
}

/// A recorded component that the component with its new id takes over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Replaced {
    /// The id in the record.
    pub from: String,
    /// The id in the catalog.
    pub to: String,
}

/// A chart whose version changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartChange {
    /// The component.
    pub component: String,
    /// The release.
    pub release: String,
    /// The recorded version.
    pub from: String,
    /// The planned version.
    pub to: String,
}

/// A release that changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VersionChange {
    /// The recorded release.
    pub from: String,
    /// The planned release.
    pub to: String,
}

impl Changes {
    /// The changes of a plan with `components`, `offered` left out and the `version` release.
    ///
    /// A left-out component counts as removed only when `apply` uninstalls it: it has a chart and
    /// `index` still knows it, as [`crate::remove::removals`] requires.
    #[must_use]
    pub fn new(
        record: &Record,
        index: &Index,
        components: &[ComponentPlan],
        offered: &[String],
        version: &str,
    ) -> Self {
        let planned: BTreeSet<&str> = components.iter().map(|c| c.id.as_str()).collect();
        let replaced: Vec<Replaced> = RENAMED
            .iter()
            .filter(|(from, to)| {
                record.components.contains(*from)
                    && !record.components.contains(*to)
                    && planned.contains(to)
            })
            .map(|(from, to)| Replaced {
                from: (*from).to_owned(),
                to: (*to).to_owned(),
            })
            .collect();
        let mut charts = Vec::new();
        for component in components {
            for release in &component.releases {
                let ReleasePlan::HelmChart { name, object, .. } = release else {
                    continue;
                };
                let key = (component.id.clone(), name.clone());
                if let (Some(from), Some(to)) = (
                    record.charts.get(&key),
                    object.pointer("/spec/version").and_then(Value::as_str),
                ) && from != to
                {
                    charts.push(ChartChange {
                        component: component.id.clone(),
                        release: name.clone(),
                        from: from.clone(),
                        to: to.to_owned(),
                    });
                }
            }
        }
        let (removed, kept) = record
            .components
            .iter()
            .filter(|id| !planned.contains(id.as_str()))
            .filter(|id| !crate::select::renamed(id).is_some_and(|new| planned.contains(new)))
            .cloned()
            .partition(|id| record.helm.contains(id) && index.component(id).is_some());
        let version = record
            .version
            .as_ref()
            .filter(|from| *from != version)
            .map(|from| VersionChange {
                from: from.clone(),
                to: version.to_owned(),
            });
        Self {
            installer: record.installer.clone(),
            added: components
                .iter()
                .filter(|c| !record.components.contains(&c.id))
                .filter(|c| !replaced.iter().any(|r| r.to == c.id))
                .map(|c| c.id.clone())
                .collect(),
            removed,
            kept,
            replaced,
            charts,
            offered: offered.to_vec(),
            restart: version.is_some(),
            version,
            host_files: Vec::new(),
        }
    }

    /// Records the host files whose content changes.
    pub fn set_host_files(&mut self, paths: Vec<String>) {
        self.restart = self.version.is_some() || !paths.is_empty();
        self.host_files = paths;
    }
}

/// The precedence of two versions as Semantic Versioning 2.0.0 orders them
/// (<https://semver.org/#spec-item-11>); `None` when either is not one.
#[must_use]
pub fn precedence(a: &str, b: &str) -> Option<Ordering> {
    let (a_core, a_pre) = parts(a)?;
    let (b_core, b_pre) = parts(b)?;
    Some(a_core.cmp(&b_core).then_with(|| {
        match (a_pre.is_empty(), b_pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => a_pre
                .iter()
                .zip(&b_pre)
                .map(|(x, y)| identifier(x, y))
                .find(|o| o.is_ne())
                .unwrap_or_else(|| a_pre.len().cmp(&b_pre.len())),
        }
    }))
}

fn parts(version: &str) -> Option<([u64; 3], Vec<&str>)> {
    let version = version.split_once('+').map_or(version, |(v, _)| v);
    let (core, pre) = version.split_once('-').unwrap_or((version, ""));
    let mut numbers = core.split('.').map(|n| n.parse::<u64>().ok());
    let core = [numbers.next()??, numbers.next()??, numbers.next()??];
    if numbers.next().is_some() {
        return None;
    }
    let pre = if pre.is_empty() {
        Vec::new()
    } else {
        pre.split('.').collect()
    };
    Some((core, pre))
}

fn identifier(a: &str, b: &str) -> Ordering {
    match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Ok(_)) => Ordering::Greater,
        (Err(_), Err(_)) => a.cmp(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_order_as_semver_says() {
        let ordered = [
            "0.1.1",
            "0.1.2-alpha",
            "0.1.2-alpha.1",
            "0.1.2-alpha.beta",
            "0.1.2-beta.2",
            "0.1.2-beta.11",
            "0.1.2-rc.1",
            "0.1.2",
            "0.2.0",
            "1.0.0",
        ];
        for pair in ordered.windows(2) {
            let [a, b] = pair else { unreachable!() };
            assert_eq!(precedence(a, b), Some(Ordering::Less), "{a} < {b}");
            assert_eq!(precedence(b, a), Some(Ordering::Greater), "{b} > {a}");
        }
        assert_eq!(precedence("0.1.2+build.5", "0.1.2"), Some(Ordering::Equal));
        assert_eq!(precedence("", "0.1.2"), None);
    }

    #[test]
    fn a_record_from_a_newer_installer_names_the_version_to_use() {
        let record = Record {
            installer: "99.0.0".to_owned(),
            components: BTreeSet::new(),
            version: None,
            charts: BTreeMap::new(),
            helm: BTreeSet::new(),
        };
        let error = record.check_installer().unwrap_err().to_string();
        assert!(
            error.contains("zeabur-installer 99.0.0 or newer"),
            "{error}"
        );
        let own = Record {
            installer: INSTALLER_VERSION.to_owned(),
            ..record
        };
        own.check_installer().unwrap();
    }
}
