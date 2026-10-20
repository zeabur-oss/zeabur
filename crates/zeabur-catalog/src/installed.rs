//! What is installed of a component, which version, who manages it, and how it compares with the
//! version the catalogue tested.
//!
//! # Where a version is read
//!
//! | Install | Version from | Compared with the tested version? |
//! |---|---|---|
//! | k3s packaged add-on (a `HelmChart` in `kube-system` whose chart is served from `/static/charts/`) | the chart file's name | no: "managed by k3s" |
//! | k3s packaged manifest (metrics-server on k3s, with no Helm evidence) | not read | no: "managed by k3s" |
//! | k3s `HelmChart` from a repository | `spec.version` | yes |
//! | Helm (CLI or any tool that sets the label) | the workloads' `helm.sh/chart` label | yes |
//! | anything else (plain manifests, operators) | not read | no |
//!
//! **Add-ons k3s packages are never flagged.** k3s ships Traefik and metrics-server itself and
//! upgrades them with k3s [k3s-packaged]; its Traefik chart is a k3s build (`41.6.101+up41.6.1`)
//! that would otherwise look older than the catalogue's `41.7.1`. They are "managed by k3s",
//! and upgrading k3s is the way to upgrade them.
//!
//! **Helm release Secrets are never decoded.** The chart and its version are inside the encoded
//! release record; the Secret's `version` label is the release's revision, not the chart's version
//! [helm-secrets]. The `helm.sh/chart` label (`<chart>-<version>`) that charts put on their
//! workloads gives the version without reading any Secret. Image tags are not compared: an app
//! version (metrics-server `v0.9.0`) and a chart version (`3.14.0`) are different schemes.
//!
//! **The tested version is not "the latest".** It is the version the catalogue pins and tests;
//! the interface says "tested", never "latest".
//!
//! [k3s-packaged]: https://github.com/k3s-io/k3s/tree/main/manifests
//! [helm-secrets]: https://github.com/helm/helm/blob/main/pkg/storage/driver/util.go

use std::cmp::Ordering;

use serde::Serialize;
use utoipa::ToSchema;
use zeabur_kube::ClusterFacts;

use crate::Component;

/// How a component was installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum InstallSource {
    /// Packaged and upgraded by k3s.
    K3s,
    /// A k3s `HelmChart` object installing from a chart repository.
    HelmChart,
    /// A Helm release (found by its workloads' `helm.sh/chart` label).
    Helm,
    /// Anything else: plain manifests, an operator, a tool Zeabur does not read.
    Other,
}

/// The installed version against the tested one. An open set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentUpgrade {
    /// The tested version.
    Current,
    /// Behind by a patch.
    BehindPatch,
    /// Behind by a minor.
    BehindMinor,
    /// Behind by a major: read the upgrade notes first.
    BehindMajor,
    /// Newer than the tested version.
    Ahead,
    /// The versions could not be compared.
    Unknown,
}

/// One installed component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Installation {
    /// The catalogue component (`traefik`).
    pub component: String,
    /// Its name for people (`Traefik Proxy`).
    pub display_name: String,
    /// How it was installed.
    pub source: InstallSource,
    /// The installed chart version, when it can be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The tool that manages it and upgrades it (`k3s`): never flagged for an upgrade.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub managed_by: Option<String>,
    /// The chart version the catalogue tested, when it pins one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tested: Option<String>,
    /// The installed version against the tested one. Absent when it is managed by k3s or the
    /// version is not known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upgrade: Option<ComponentUpgrade>,
}

/// Charts k3s packages and upgrades itself, by catalogue id.
const K3S_PACKAGED: &[&str] = &["traefik", "metrics-server"];

/// `major.minor.patch` of a chart version (`v1.21.2`, `41.7.1`, `41.6.101+up41.6.1`).
fn semver(text: &str) -> Option<(u64, u64, u64)> {
    let text = text.trim();
    let text = text.strip_prefix('v').unwrap_or(text);
    let core = text.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    let patch = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    Some((major, minor, patch))
}

/// How `installed` compares with `tested`.
#[must_use]
pub fn compare(installed: &str, tested: &str) -> ComponentUpgrade {
    let (Some(a), Some(b)) = (semver(installed), semver(tested)) else {
        return ComponentUpgrade::Unknown;
    };
    match a.cmp(&b) {
        Ordering::Equal => ComponentUpgrade::Current,
        Ordering::Greater => ComponentUpgrade::Ahead,
        Ordering::Less if a.0 < b.0 => ComponentUpgrade::BehindMajor,
        Ordering::Less if a.1 < b.1 => ComponentUpgrade::BehindMinor,
        Ordering::Less => ComponentUpgrade::BehindPatch,
    }
}

/// The version in a k3s packaged chart's URL:
/// `https://%{KUBERNETES_API}%/static/charts/traefik-41.6.101+up41.6.1.tgz` → `41.6.101+up41.6.1`.
fn packaged_chart_version(url: &str, name: &str) -> Option<String> {
    let file = url.rsplit('/').next()?;
    let base = file.strip_suffix(".tgz")?;
    let version = base.strip_prefix(name)?.strip_prefix('-')?;
    (!version.is_empty()).then(|| version.to_owned())
}

/// The installation of `component`, which detection found; `distribution` is the catalogue's
/// name for the cluster's distribution.
#[must_use]
pub fn installation(
    component: &Component,
    facts: &ClusterFacts,
    distribution: Option<&str>,
) -> Installation {
    let mut found = Installation {
        component: component.id.clone(),
        display_name: component.display_name.clone(),
        source: InstallSource::Other,
        version: None,
        managed_by: None,
        tested: component.tested_version.clone(),
        upgrade: None,
    };
    for addon in &component.detection.k3s_addons {
        let key = format!("kube-system/{addon}");
        let Some(chart) = facts.k3s_helm_charts.get(&key) else {
            continue;
        };
        if chart.contains("/static/charts/") {
            found.source = InstallSource::K3s;
            found.managed_by = Some("k3s".to_owned());
            found.version = packaged_chart_version(chart, addon);
            return found;
        }
        found.source = InstallSource::HelmChart;
        found.version = facts.k3s_helm_chart_versions.get(&key).cloned();
    }
    if found.version.is_none() {
        for chart in &component.detection.helm_charts {
            let version = facts.workload_labels.iter().find_map(|(_, labels)| {
                let rest = labels.get("helm.sh/chart")?.strip_prefix(chart.as_str())?;
                let version = rest.strip_prefix('-')?;
                semver(version).map(|_| version.to_owned())
            });
            if version.is_some() {
                found.source = InstallSource::Helm;
                found.version = version;
                break;
            }
        }
    }
    if found.version.is_none()
        && found.source == InstallSource::Other
        && distribution == Some("k3s")
        && K3S_PACKAGED.contains(&component.id.as_str())
    {
        found.source = InstallSource::K3s;
        found.managed_by = Some("k3s".to_owned());
        return found;
    }
    found.upgrade = match (&found.version, &found.tested) {
        (Some(installed), Some(tested)) => Some(compare(installed, tested)),
        (Some(_), None) | (None, Some(_) | None) => None,
    };
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue;

    fn component(id: &str) -> Component {
        catalogue()
            .unwrap()
            .into_iter()
            .find(|c| c.id == id)
            .unwrap()
    }

    #[test]
    fn versions_compare_by_major_minor_and_patch() {
        assert_eq!(compare("v1.21.2", "v1.21.2"), ComponentUpgrade::Current);
        assert_eq!(compare("v1.19.1", "v1.21.2"), ComponentUpgrade::BehindMinor);
        assert_eq!(compare("40.0.0", "41.7.1"), ComponentUpgrade::BehindMajor);
        assert_eq!(compare("41.7.0", "41.7.1"), ComponentUpgrade::BehindPatch);
        assert_eq!(compare("42.0.0", "41.7.1"), ComponentUpgrade::Ahead);
        assert_eq!(compare("main", "41.7.1"), ComponentUpgrade::Unknown);
    }

    #[test]
    fn k3s_packaged_traefik_is_managed_by_k3s_and_never_flagged() {
        let mut facts = ClusterFacts {
            version: "v1.36.5+k3s1".to_owned(),
            ..ClusterFacts::default()
        };
        facts.k3s_helm_charts.insert(
            "kube-system/traefik".to_owned(),
            "https://%{KUBERNETES_API}%/static/charts/traefik-41.6.101+up41.6.1.tgz".to_owned(),
        );
        let found = installation(&component("traefik"), &facts, Some("k3s"));
        assert_eq!(found.source, InstallSource::K3s);
        assert_eq!(found.managed_by.as_deref(), Some("k3s"));
        assert_eq!(found.version.as_deref(), Some("41.6.101+up41.6.1"));
        assert_eq!(found.upgrade, None);
        let metrics = installation(&component("metrics-server"), &facts, Some("k3s"));
        assert_eq!(metrics.managed_by.as_deref(), Some("k3s"));
        assert_eq!(metrics.upgrade, None);
    }

    #[test]
    fn a_repository_helm_chart_is_compared() {
        let mut facts = ClusterFacts::default();
        facts
            .k3s_helm_charts
            .insert("kube-system/traefik".to_owned(), "traefik".to_owned());
        facts
            .k3s_helm_chart_versions
            .insert("kube-system/traefik".to_owned(), "41.7.1".to_owned());
        let found = installation(&component("traefik"), &facts, Some("k3s"));
        assert_eq!(found.source, InstallSource::HelmChart);
        assert_eq!(found.upgrade, Some(ComponentUpgrade::Current));
    }
}
