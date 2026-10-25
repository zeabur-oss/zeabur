#![doc = include_str!("../README.md")]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use zeabur_kube::ClusterFacts;

mod bundle {
    include!(concat!(env!("OUT_DIR"), "/bundle.rs"));
}
pub mod install;
pub mod installed;
pub mod measured;
pub mod versions;

pub use installed::{ComponentUpgrade, InstallSource, Installation};
pub use measured::{IdleUse, template_idle};

/// The templates of Zeabur Catalog this build carries.
pub const TEMPLATES: &[&str] = &["error-pages"];

/// The `manifests.yaml` of Zeabur Catalog's `error-pages` template, a Go template whose only field
/// is `{{ .Namespace }}`: the page server for a project's error pages and maintenance page.
pub const ERROR_PAGES_MANIFESTS: &str = include_str!(concat!(
    env!("OUT_DIR"),
    "/bundle/templates/error-pages/manifests.yaml"
));

/// The commit of Zeabur Catalog this build carries.
pub const CATALOGUE_COMMIT: &str = env!("ZEABUR_CATALOG_COMMIT");

/// One component of the catalogue: an upstream project that provides a capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    /// Its id, the file name in the catalogue (`traefik`).
    pub id: String,
    /// The capability it provides (`ingress`).
    pub capability: String,
    /// Human-readable name.
    pub display_name: String,
    /// The project's home page.
    pub homepage: String,
    /// Its licence (SPDX).
    pub license: String,
    /// Why the catalogue chose it.
    pub description: String,
    /// Whether the catalogue installs it by default, per profile (`single-node`, `multi-node`):
    /// `recommended` or `optional`.
    pub defaults: BTreeMap<String, String>,
    /// The distributions it supports (`k3s`, …); empty means all.
    pub distributions: Vec<String>,
    /// How to recognise an existing installation.
    #[serde(skip)]
    #[schema(ignore)]
    pub detection: Detection,
    /// The chart version the catalogue pins and tests, from its first release that names a
    /// chart this component is detected by.
    #[serde(skip)]
    #[schema(ignore)]
    pub tested_version: Option<String>,
}

/// A release of a catalogue file: only its chart and version are read.
#[derive(Debug, Deserialize)]
struct Release {
    #[serde(default)]
    chart: Option<String>,
    #[serde(default)]
    version: Option<String>,
}

/// The version of the first release whose chart (its last path segment) is one the component is
/// detected by.
fn tested_version(releases: &[Release], detection: &Detection) -> Option<String> {
    releases.iter().find_map(|release| {
        let chart = release.chart.as_deref()?;
        let name = chart.rsplit('/').next().unwrap_or(chart);
        detection
            .helm_charts
            .iter()
            .any(|c| c == name)
            .then(|| release.version.clone())
            .flatten()
    })
}

/// The catalogue's `detection` block: any one match means the capability exists.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    /// CustomResourceDefinition names.
    #[serde(default)]
    pub crds: Vec<String>,
    /// APIService names.
    #[serde(default)]
    pub api_services: Vec<String>,
    /// GatewayClass controller names.
    #[serde(default)]
    pub gateway_controllers: Vec<String>,
    /// IngressClass controller names.
    #[serde(default)]
    pub ingress_controllers: Vec<String>,
    /// Helm chart names.
    #[serde(default)]
    pub helm_charts: Vec<String>,
    /// k3s packaged add-ons (`HelmChart` objects in `kube-system`).
    #[serde(default)]
    pub k3s_addons: Vec<String>,
    /// Workloads carrying all of these labels.
    #[serde(default)]
    pub workloads: Vec<WorkloadMatch>,
}

/// A workload kind and the labels that identify it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct WorkloadMatch {
    /// `Deployment`, `StatefulSet` or `DaemonSet`.
    pub kind: String,
    /// Labels that must all be present with these values.
    pub labels: BTreeMap<String, String>,
}

/// The parts of a catalogue file Zeabur reads; the rest is ignored.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct File {
    metadata: Metadata,
    defaults: BTreeMap<String, String>,
    #[serde(default)]
    distributions: Vec<String>,
    detection: Detection,
    #[serde(default)]
    releases: Vec<Release>,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    name: String,
    description: String,
    license: String,
    homepage: String,
}

/// Why the catalogue could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Zeabur Catalog, {capability}/{id}: {message}")]
pub struct CatalogueError {
    capability: String,
    id: String,
    message: String,
}

/// The components of the embedded catalogue, sorted by capability and id.
///
/// # Errors
///
/// When a file does not parse; a test reads them all, so a build that passes CI cannot fail
/// here.
pub fn catalogue() -> Result<Vec<Component>, CatalogueError> {
    bundle::FILES
        .iter()
        .map(|(capability, id, text)| {
            let file: File = serde_saphyr::from_str(text).map_err(|e| CatalogueError {
                capability: (*capability).to_owned(),
                id: (*id).to_owned(),
                message: e.to_string(),
            })?;
            Ok(Component {
                id: (*id).to_owned(),
                capability: (*capability).to_owned(),
                display_name: file.metadata.name,
                homepage: file.metadata.homepage,
                license: file.metadata.license,
                description: file.metadata.description.trim().to_owned(),
                defaults: file.defaults,
                distributions: file.distributions,
                tested_version: tested_version(&file.releases, &file.detection),
                detection: file.detection,
            })
        })
        .collect()
}

/// Whether a capability is present on the cluster, and on what evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    /// The capability (`ingress`, `certificates`, …).
    pub name: String,
    /// `true` when any component of it was found.
    pub existing: bool,
    /// What was found, one line per match (`IngressClass controller
    /// traefik.io/ingress-controller`).
    pub evidence: Vec<String>,
    /// For a missing capability, whether the catalogue recommends installing it on a cluster of
    /// this shape; `None` when it exists.
    pub suggestion: Option<Suggestion>,
    /// The catalogue's components for it: what to install when it is missing.
    pub components: Vec<Component>,
    /// The components found installed, with their versions and who manages them.
    pub installations: Vec<Installation>,
}

/// How strongly the catalogue suggests a missing capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Suggestion {
    /// Installed by default on clusters of this shape.
    Recommended,
    /// Available, but not installed by default (heavier or for specific needs).
    Optional,
    /// No component supports this cluster's distribution.
    Unsupported,
}

/// The catalogue profile of a cluster: `single-node` for one node (or when nodes cannot be
/// counted), `multi-node` otherwise.
#[must_use]
pub const fn profile(facts: &ClusterFacts) -> &'static str {
    match facts.nodes {
        Some(n) if n > 1 => "multi-node",
        Some(_) | None => "single-node",
    }
}

/// The distribution, as the catalogue names it, from kube-apiserver's version (`v1.36.5+k3s1`).
#[must_use]
pub fn distribution(facts: &ClusterFacts) -> Option<&'static str> {
    if facts.version.contains("+k3s") {
        Some("k3s")
    } else if facts.version.contains("+rke2") {
        Some("rke2")
    } else {
        None
    }
}

fn suggestion(components: &[Component], facts: &ClusterFacts) -> Suggestion {
    let profile = profile(facts);
    let distribution = distribution(facts);
    let supported: Vec<&Component> = components
        .iter()
        .filter(|c| {
            c.distributions.is_empty()
                || distribution.is_some_and(|d| c.distributions.iter().any(|x| x == d))
        })
        .collect();
    if supported.is_empty() {
        Suggestion::Unsupported
    } else if supported
        .iter()
        .any(|c| c.defaults.get(profile).is_some_and(|m| m == "recommended"))
    {
        Suggestion::Recommended
    } else {
        Suggestion::Optional
    }
}

/// Each capability of the catalogue: existing (with the evidence) or not.
///
/// Zeabur does not judge an existing installation against the catalogue's settings; it reports
/// that one is there. A missing capability lists what could be installed, and whether the
/// catalogue recommends it for a cluster of this shape (profile and distribution) or keeps it
/// optional, so a small cluster is not told to install everything. A probe the
/// person may not run finds nothing, so a capability can look missing to someone who cannot see
/// it; [`ClusterFacts::unreadable`] tells the interface to say so.
#[must_use]
pub fn detect(components: &[Component], facts: &ClusterFacts) -> Vec<Capability> {
    let mut capabilities: BTreeMap<String, Capability> = BTreeMap::new();
    for component in components {
        let capability = capabilities
            .entry(component.capability.clone())
            .or_insert_with(|| Capability {
                name: component.capability.clone(),
                existing: false,
                evidence: Vec::new(),
                suggestion: None,
                components: Vec::new(),
                installations: Vec::new(),
            });
        let evidence = evidence(&component.detection, facts);
        capability.existing |= !evidence.is_empty();
        if !evidence.is_empty() {
            capability.installations.push(installed::installation(
                component,
                facts,
                distribution(facts),
            ));
        }
        capability.evidence.extend(
            evidence
                .into_iter()
                .map(|e| format!("{}: {e}", component.display_name)),
        );
        capability.components.push(component.clone());
    }
    let mut capabilities: Vec<Capability> = capabilities.into_values().collect();
    for capability in &mut capabilities {
        if !capability.existing {
            capability.suggestion = Some(suggestion(&capability.components, facts));
        }
    }
    capabilities
}

/// Everything in `facts` that matches `detection`.
fn evidence(detection: &Detection, facts: &ClusterFacts) -> Vec<String> {
    let mut found = Vec::new();
    for crd in detection.crds.iter().filter(|c| facts.crds.contains(*c)) {
        found.push(format!("CustomResourceDefinition {crd}"));
    }
    for service in detection
        .api_services
        .iter()
        .filter(|s| facts.api_services.contains(*s))
    {
        found.push(format!("APIService {service}"));
    }
    for controller in detection
        .gateway_controllers
        .iter()
        .filter(|c| facts.gateway_controllers.contains(*c))
    {
        found.push(format!("GatewayClass controller {controller}"));
    }
    for controller in detection
        .ingress_controllers
        .iter()
        .filter(|c| facts.ingress_controllers.contains(*c))
    {
        found.push(format!("IngressClass controller {controller}"));
    }
    for chart in &detection.helm_charts {
        // Release Secrets are never decoded, so the chart behind a release is unknown. A chart
        // is recognised by a release named after it (as `helm install <chart> <repo>/<chart>`
        // names it), or by the `helm.sh/chart` label (`<chart>-<version>`, the version
        // optionally prefixed with `v`) its workloads carry, which also finds releases named
        // otherwise.
        if let Some(release) = facts.helm_releases.iter().find(|r| {
            r.rsplit_once('/')
                .is_some_and(|(_, name)| name == chart.as_str())
        }) {
            found.push(format!("Helm release {release}"));
        } else if facts.workload_labels.iter().any(|(_, labels)| {
            labels
                .get("helm.sh/chart")
                .and_then(|label| label.strip_prefix(chart.as_str()))
                .and_then(|rest| rest.strip_prefix('-'))
                .map(|version| version.strip_prefix('v').unwrap_or(version))
                .is_some_and(|version| version.starts_with(|c: char| c.is_ascii_digit()))
        }) {
            found.push(format!("Helm chart {chart}"));
        }
    }
    for addon in &detection.k3s_addons {
        let key = format!("kube-system/{addon}");
        if facts.k3s_helm_charts.contains_key(&key) {
            found.push(format!("k3s add-on {addon}"));
        }
    }
    for wanted in &detection.workloads {
        if facts.workload_labels.iter().any(|(kind, labels)| {
            *kind == wanted.kind
                && wanted
                    .labels
                    .iter()
                    .all(|(k, v)| labels.get(k).is_some_and(|value| value == v))
        }) {
            let selector: Vec<String> = wanted
                .labels
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            found.push(format!("{} with {}", wanted.kind, selector.join(",")));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalogue_file_parses_and_has_a_detection_rule() {
        let components = catalogue().unwrap();
        // Every component of the release's index is embedded.
        let index: serde_json::Value = serde_json::from_slice(
            &std::fs::read(concat!(env!("ZEABUR_CATALOG_RELEASE"), "/index.json")).unwrap(),
        )
        .unwrap();
        let mut listed: Vec<&str> = index["components"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap())
            .collect();
        let mut embedded: Vec<String> = components
            .iter()
            .map(|c| format!("{}/{}", c.capability, c.id))
            .collect();
        listed.sort_unstable();
        embedded.sort();
        assert_eq!(embedded, listed);
        for component in &components {
            let d = &component.detection;
            let rules = d.crds.len()
                + d.api_services.len()
                + d.gateway_controllers.len()
                + d.ingress_controllers.len()
                + d.helm_charts.len()
                + d.k3s_addons.len()
                + d.workloads.len();
            assert!(
                rules > 0,
                "{}/{} has no detection rule",
                component.capability,
                component.id
            );
        }
    }

    fn k3s_defaults() -> ClusterFacts {
        let mut facts = ClusterFacts {
            version: "v1.36.5+k3s1".to_owned(),
            nodes: Some(1),
            ..ClusterFacts::default()
        };
        facts
            .ingress_controllers
            .insert("traefik.io/ingress-controller".to_owned());
        facts
            .k3s_helm_charts
            .insert("kube-system/traefik".to_owned(), "traefik".to_owned());
        facts
            .api_services
            .insert("v1beta1.metrics.k8s.io".to_owned());
        facts.workload_labels.push((
            "Deployment".to_owned(),
            BTreeMap::from([("k8s-app".to_owned(), "metrics-server".to_owned())]),
        ));
        facts
    }

    #[test]
    fn a_fresh_k3s_has_ingress_but_no_certificates() {
        let capabilities = detect(&catalogue().unwrap(), &k3s_defaults());
        let existing: Vec<&str> = capabilities
            .iter()
            .filter(|c| c.existing)
            .map(|c| c.name.as_str())
            .collect();
        assert!(existing.contains(&"ingress"), "{existing:?}");
        let ingress = capabilities.iter().find(|c| c.name == "ingress").unwrap();
        assert!(
            ingress
                .evidence
                .iter()
                .any(|e| e.contains("k3s add-on traefik"))
        );
        let certificates = capabilities
            .iter()
            .find(|c| c.name == "certificates")
            .unwrap();
        assert!(!certificates.existing);
        assert_eq!(certificates.suggestion, Some(Suggestion::Recommended));
        assert_eq!(certificates.components[0].id, "cert-manager");
        assert_eq!(ingress.suggestion, None);
        let backup = capabilities
            .iter()
            .find(|c| c.name == "volume-backup")
            .unwrap();
        assert_eq!(backup.suggestion, Some(Suggestion::Optional));
    }

    #[test]
    fn node_updates_are_unsupported_outside_k3s() {
        let mut facts = ClusterFacts {
            version: "v1.36.2".to_owned(),
            nodes: Some(3),
            ..ClusterFacts::default()
        };
        let find = |facts: &ClusterFacts| {
            detect(&catalogue().unwrap(), facts)
                .into_iter()
                .find(|c| c.name == "node-updates")
                .and_then(|c| c.suggestion)
        };
        assert_eq!(find(&facts), Some(Suggestion::Unsupported));
        facts.version = "v1.36.5+k3s1".to_owned();
        assert_ne!(find(&facts), Some(Suggestion::Unsupported));
    }

    #[test]
    fn a_helm_release_counts_by_its_release_name_only() {
        let detection = Detection {
            helm_charts: vec!["cert-manager".to_owned()],
            ..Detection::default()
        };
        let mut facts = ClusterFacts::default();
        facts
            .helm_releases
            .insert("cert-manager/cert-manager".to_owned());
        assert_eq!(
            evidence(&detection, &facts),
            ["Helm release cert-manager/cert-manager"]
        );
        // A release named otherwise says nothing about its chart, which is never decoded.
        let mut renamed = ClusterFacts::default();
        renamed.helm_releases.insert("security/certs".to_owned());
        assert_eq!(evidence(&detection, &renamed), Vec::<String>::new());
    }

    #[test]
    fn a_chart_is_recognised_by_its_workload_label_without_reading_secrets() {
        let mut facts = ClusterFacts::default();
        facts.workload_labels.push((
            "Deployment".to_owned(),
            BTreeMap::from([(
                "helm.sh/chart".to_owned(),
                "cert-manager-v1.19.1".to_owned(),
            )]),
        ));
        let detection = Detection {
            helm_charts: vec!["cert-manager".to_owned()],
            ..Detection::default()
        };
        assert_eq!(evidence(&detection, &facts), ["Helm chart cert-manager"]);
        // A different chart that shares the prefix does not count.
        let mut other = ClusterFacts::default();
        other.workload_labels.push((
            "Deployment".to_owned(),
            BTreeMap::from([(
                "helm.sh/chart".to_owned(),
                "cert-manager-webhook-1.0.0".to_owned(),
            )]),
        ));
        assert_eq!(evidence(&detection, &other), Vec::<String>::new());
    }
}
