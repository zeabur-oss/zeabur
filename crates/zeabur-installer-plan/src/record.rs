//! The install record that `apply` keeps in `kube-system/zeabur-installer`, and what the next
//! `apply` changes against it.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::Value;

use crate::PlanError;
use crate::catalog::Index;
use crate::plan::{ComponentPlan, IdentityPlan, ReleasePlan, ZeaburPlan};

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
    /// The `valuesContent` of each chart by component and release name.
    pub values: BTreeMap<(String, String), String>,
    /// The components with a `HelmChart` release, which `apply` uninstalls when they leave.
    pub helm: BTreeSet<String>,
    /// Zeabur Identity's issuer, `https://<identity>`.
    pub identity: Option<String>,
    /// Zeabur's URL.
    pub zeabur: Option<String>,
    /// Zeabur Identity's release; `None` from an older installer or a development source.
    pub identity_version: Option<String>,
    /// Zeabur's release; `None` from an older installer or a development source.
    pub zeabur_version: Option<String>,
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
        let mut values = BTreeMap::new();
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
                    if let Some(content) = release
                        .pointer("/object/spec/valuesContent")
                        .and_then(Value::as_str)
                    {
                        values.insert((id.to_owned(), name.to_owned()), content.to_owned());
                    }
                }
            }
        }
        Some(Self {
            installer: text("installer").unwrap_or_default().to_owned(),
            components,
            version: text("version").map(str::to_owned),
            charts,
            values,
            helm,
            identity: plan
                .pointer("/identity/issuer")
                .and_then(Value::as_str)
                .map(str::to_owned),
            zeabur: plan
                .pointer("/zeabur/url")
                .and_then(Value::as_str)
                .map(str::to_owned),
            identity_version: plan
                .pointer("/identity/version")
                .and_then(Value::as_str)
                .map(str::to_owned),
            zeabur_version: plan
                .pointer("/zeabur/version")
                .and_then(Value::as_str)
                .map(str::to_owned),
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
    /// Charts whose version or values change.
    pub charts: Vec<ChartChange>,
    /// The K3s, RKE2 or `KubeSolo` release, when it changes.
    pub version: Option<VersionChange>,
    /// Zeabur Identity's or Zeabur's URL, when the access domains change.
    pub urls: Vec<UrlChange>,
    /// Zeabur Identity's or Zeabur's release, when it changes.
    pub products: Vec<ProductChange>,
    /// Host files whose content changes; `apply` fills it.
    pub host_files: Vec<String>,
    /// `apply` restarts K3s, RKE2 or `KubeSolo`.
    pub restart: bool,
}

/// A chart whose version or values change.
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
    /// The values change too.
    pub values: bool,
}

/// A URL that changes: certificates are issued again and sign-in moves to the new one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UrlChange {
    /// What the URL is for, as `plan` names it.
    pub name: String,
    /// The recorded URL.
    pub from: String,
    /// The planned URL.
    pub to: String,
}

/// Zeabur Identity's or Zeabur's release that `apply` installs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProductChange {
    /// `Zeabur Identity` or `Zeabur`.
    pub name: String,
    /// The recorded release; `None` when an older installer wrote the record.
    pub from: Option<String>,
    /// The planned release.
    pub to: String,
}

/// What a plan installs, to compare with the record.
#[derive(Debug, Clone, Copy)]
pub struct Planned<'a> {
    /// The components.
    pub components: &'a [ComponentPlan],
    /// The K3s, RKE2 or `KubeSolo` release.
    pub version: &'a str,
    /// Zeabur Identity; `None` when the plan has none.
    pub identity: Option<&'a IdentityPlan>,
    /// Zeabur; `None` when the plan has none.
    pub zeabur: Option<&'a ZeaburPlan>,
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
    /// The changes `planned` makes against `record`.
    ///
    /// A left-out component counts as removed only when `apply` uninstalls it: it has a chart and
    /// `index` still knows it, as [`crate::remove::removals`] requires.
    #[must_use]
    pub fn new(record: &Record, index: &Index, planned: Planned<'_>) -> Self {
        let ids: BTreeSet<&str> = planned.components.iter().map(|c| c.id.as_str()).collect();
        let mut added = Vec::new();
        for component in planned.components {
            if !record.components.contains(&component.id) {
                added.push(component.id.clone());
            }
        }
        let mut removed = Vec::new();
        let mut kept = Vec::new();
        for id in &record.components {
            if ids.contains(id.as_str()) {
                continue;
            }
            if record.helm.contains(id) && index.component(id).is_some() {
                removed.push(id.clone());
            } else {
                kept.push(id.clone());
            }
        }
        let version = record
            .version
            .as_ref()
            .filter(|from| *from != planned.version)
            .map(|from| VersionChange {
                from: from.clone(),
                to: planned.version.to_owned(),
            });
        Self {
            installer: record.installer.clone(),
            added,
            removed,
            kept,
            charts: chart_changes(record, planned.components),
            restart: version.is_some(),
            version,
            urls: url_changes(record, planned),
            products: product_changes(record, planned),
            host_files: Vec::new(),
        }
    }

    /// Records the host files whose content changes.
    pub fn set_host_files(&mut self, paths: Vec<String>) {
        self.restart = self.version.is_some() || !paths.is_empty();
        self.host_files = paths;
    }
}

/// The planned charts whose version or values differ from the recorded ones.
fn chart_changes(record: &Record, components: &[ComponentPlan]) -> Vec<ChartChange> {
    let mut charts = Vec::new();
    for component in components {
        for release in &component.releases {
            let ReleasePlan::HelmChart { name, object, .. } = release else {
                continue;
            };
            let key = (component.id.clone(), name.clone());
            let Some(from) = record.charts.get(&key) else {
                continue;
            };
            let Some(to) = object.pointer("/spec/version").and_then(Value::as_str) else {
                continue;
            };
            let values_changed = record.values.get(&key).is_some_and(|recorded| {
                object
                    .pointer("/spec/valuesContent")
                    .and_then(Value::as_str)
                    .is_some_and(|planned| planned != recorded)
            });
            if from != to || values_changed {
                charts.push(ChartChange {
                    component: component.id.clone(),
                    release: name.clone(),
                    from: from.clone(),
                    to: to.to_owned(),
                    values: values_changed,
                });
            }
        }
    }
    charts
}

/// Zeabur Identity's and Zeabur's URLs, when both the record and the plan have one and they
/// differ.
fn url_changes(record: &Record, planned: Planned<'_>) -> Vec<UrlChange> {
    let pairs = [
        (
            "Zeabur Identity",
            record.identity.as_deref(),
            planned.identity.map(|i| i.issuer.as_str()),
        ),
        (
            "Zeabur",
            record.zeabur.as_deref(),
            planned.zeabur.map(|z| z.url.as_str()),
        ),
    ];
    let mut urls = Vec::new();
    for (name, from, to) in pairs {
        if let (Some(from), Some(to)) = (from, to)
            && from != to
        {
            urls.push(UrlChange {
                name: name.to_owned(),
                from: from.to_owned(),
                to: to.to_owned(),
            });
        }
    }
    urls
}

/// Zeabur Identity's and Zeabur's releases, when the plan pins one the record doesn't have.
fn product_changes(record: &Record, planned: Planned<'_>) -> Vec<ProductChange> {
    let pairs = [
        (
            "Zeabur Identity",
            record.identity_version.as_ref(),
            planned.identity.and_then(|i| i.version.as_ref()),
        ),
        (
            "Zeabur",
            record.zeabur_version.as_ref(),
            planned.zeabur.and_then(|z| z.version.as_ref()),
        ),
    ];
    let mut products = Vec::new();
    for (name, from, to) in pairs {
        let Some(to) = to else {
            continue;
        };
        if from == Some(to) {
            continue;
        }
        products.push(ProductChange {
            name: name.to_owned(),
            from: from.cloned(),
            to: to.clone(),
        });
    }
    products
}

/// The precedence of two versions as Semantic Versioning 2.0.0 orders them
/// (<https://semver.org/#spec-item-11>); `None` when either is not one.
#[must_use]
pub fn precedence(a: &str, b: &str) -> Option<Ordering> {
    let a = semver::Version::parse(a).ok()?;
    let b = semver::Version::parse(b).ok()?;
    Some(a.cmp_precedence(&b))
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
            values: BTreeMap::new(),
            helm: BTreeSet::new(),
            identity: None,
            zeabur: None,
            identity_version: None,
            zeabur_version: None,
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
