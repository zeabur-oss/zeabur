#![doc = include_str!("../README.md")]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use zeabur_catalog_format::common::{Distribution, Mode, Profile, WorkloadKind};
use zeabur_catalog_format::component::Release;
pub use zeabur_catalog_format::component::{Addon, DetectedWorkload, Detection};
use zeabur_kube::ClusterFacts;

mod bundle {
    include!(concat!(env!("OUT_DIR"), "/bundle.rs"));
}
pub mod api;
pub mod engine;
pub mod install;
pub mod installed;
mod kube_version;
pub mod measured;
pub mod templates;
pub mod versions;
pub mod zeabur;

pub use installed::{ComponentUpgrade, InstallSource, Installation};
pub use measured::{IdleUse, MeasuredPod, template_idle};
pub use templates::{Catalog, Dropped, RenderError, RenderOptions, Template, template_id};
/// The types of Zeabur Catalog's files.
pub use zeabur_catalog_format as format;

/// The commit of Zeabur Catalog this build carries.
pub const CATALOGUE_COMMIT: &str = env!("ZEABUR_CATALOG_COMMIT");

/// One component of the catalogue: an upstream project that provides a capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    /// Its id, the file name in the catalogue (`traefik`).
    pub id: String,
    /// The capability it provides (`ingress`), from `provides.capability`.
    pub capability: String,
    /// Human-readable name.
    pub display_name: String,
    /// The project's home page.
    pub homepage: String,
    /// Its licence (SPDX).
    pub license: String,
    /// What it gives the cluster, in one sentence, by language tag (`en`, `zh-TW`, `zh-CN`);
    /// `en` is always there.
    pub summary: BTreeMap<String, String>,
    /// Whether the catalogue installs it by default, per profile (`single-node`, `multi-node`):
    /// `recommended` or `optional`.
    pub defaults: BTreeMap<String, String>,
    /// The distributions it supports (`k3s`, …).
    pub distributions: Vec<String>,
    /// The Kubernetes versions it supports, as a Helm chart's `kubeVersion` range.
    #[serde(skip)]
    #[schema(ignore)]
    pub kube_version: String,
    /// How to recognise an existing installation.
    #[serde(skip)]
    #[schema(ignore)]
    pub detection: Detection,
    /// The chart version the catalogue pins and tests, from its first release that names a
    /// chart this component is detected by.
    #[serde(skip)]
    #[schema(ignore)]
    pub tested_version: Option<String>,
    /// What it uses that Zeabur cannot install yet, from the catalog release Zeabur read; absent
    /// when Zeabur installs it, or has not read the catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsupported: Option<engine::Unsupported>,
}

/// The version of the first chart release whose chart (its last path segment) is one the
/// component is detected by.
fn tested_version(releases: &[Release], detection: &Detection) -> Option<String> {
    releases
        .iter()
        .filter_map(Release::helm)
        .find_map(|release| {
            let name = release.chart.rsplit('/').next().unwrap_or(&release.chart);
            detection
                .helm_charts
                .iter()
                .any(|c| c == name)
                .then(|| release.version.clone())
        })
}

/// The name the catalogue gives `mode`; a mode Zeabur does not know counts as unsupported.
const fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Recommended => "recommended",
        Mode::Optional => "optional",
        Mode::Unsupported | Mode::Unknown => "unsupported",
    }
}

/// The Kubernetes kind of a workload the catalogue detects a component by.
const fn workload_kind(kind: WorkloadKind) -> Option<&'static str> {
    match kind {
        WorkloadKind::DaemonSet => Some("DaemonSet"),
        WorkloadKind::Deployment => Some("Deployment"),
        WorkloadKind::StatefulSet => Some("StatefulSet"),
        WorkloadKind::Unknown => None,
    }
}

/// Why the catalogue could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Zeabur Catalog, {id}: {message}")]
pub struct CatalogueError {
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
    let mut components = bundle::FILES
        .iter()
        .map(|(id, text)| {
            let error = |message: String| CatalogueError {
                id: (*id).to_owned(),
                message,
            };
            let file: format::component::Component =
                serde_saphyr::from_str(text).map_err(|e| error(e.to_string()))?;
            Ok(Component {
                id: (*id).to_owned(),
                capability: file
                    .capability()
                    .ok_or_else(|| error("it provides no capability".to_owned()))?
                    .to_owned(),
                display_name: file.metadata.name,
                homepage: file.metadata.homepage,
                license: file.metadata.license,
                summary: {
                    let mut summary: BTreeMap<String, String> = file
                        .metadata
                        .locales
                        .into_iter()
                        .map(|(lang, l)| (lang, l.summary.trim().to_owned()))
                        .collect();
                    summary.insert("en".to_owned(), file.metadata.summary.trim().to_owned());
                    summary
                },
                defaults: Profile::ALL
                    .into_iter()
                    .map(|p| {
                        (
                            p.as_str().to_owned(),
                            mode_name(file.defaults.mode(p)).to_owned(),
                        )
                    })
                    .collect(),
                distributions: file
                    .distributions
                    .iter()
                    .filter(|d| **d != Distribution::Unknown)
                    .map(|d| d.as_str().to_owned())
                    .collect(),
                kube_version: file.kube_version,
                tested_version: tested_version(&file.releases, &file.detection),
                detection: file.detection,
                unsupported: None,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    components.sort_by(|a, b| (&a.capability, &a.id).cmp(&(&b.capability, &b.id)));
    Ok(components)
}

/// Whether a capability is present on the cluster, and on what evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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
    /// For a missing capability, the component to install: one recommended on this cluster's
    /// profile, else the first that runs here; `None` when it exists or none runs here.
    pub install: Option<ComponentRef>,
}

/// A catalogue component, as an install link names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentRef {
    /// The component's id (`k8up`), what `zeaburctl components export` and the component plan
    /// take.
    pub id: String,
    /// Its human-readable name (`K8up`).
    pub name: String,
}

impl From<&Component> for ComponentRef {
    fn from(component: &Component) -> Self {
        Self {
            id: component.id.clone(),
            name: component.display_name.clone(),
        }
    }
}

/// The component `id` of the catalogue.
#[must_use]
pub fn component_ref(catalogue: &[Component], id: &str) -> Option<ComponentRef> {
    catalogue
        .iter()
        .find(|c| c.id == id)
        .map(ComponentRef::from)
}

/// How strongly the catalogue suggests a missing capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Suggestion {
    /// Installed by default on clusters of this shape.
    Recommended,
    /// Available, but not installed by default (heavier or for specific needs).
    Optional,
    /// No component runs here: on the cluster's distribution and Kubernetes, and on its profile.
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
    } else if facts.version.contains("+kubesolo") {
        Some("kubesolo")
    } else {
        None
    }
}

/// A component's `defaults` on a profile; a value Zeabur does not know counts as unsupported.
fn default_on<'a>(component: &'a Component, profile: &str) -> &'a str {
    component
        .defaults
        .get(profile)
        .map(String::as_str)
        .filter(|known| matches!(*known, "recommended" | "optional"))
        .unwrap_or("unsupported")
}

/// The components of a capability that can run here: on the cluster's distribution, when the
/// version names one the catalogue knows, on its Kubernetes, and not unsupported on its profile.
fn supported<'a>(components: &'a [Component], facts: &ClusterFacts) -> Vec<&'a Component> {
    let profile = profile(facts);
    let distribution = distribution(facts);
    components
        .iter()
        .filter(|c| {
            distribution.is_none_or(|d| c.distributions.iter().any(|x| x == d))
                && kube_version::admits(&c.kube_version, &facts.version).unwrap_or(false)
                && default_on(c, profile) != "unsupported"
        })
        .collect()
}

/// How strongly the catalogue suggests a missing capability, and the component to install for
/// it: a recommended one, else the first that runs here.
fn suggestion(
    components: &[Component],
    facts: &ClusterFacts,
) -> (Suggestion, Option<ComponentRef>) {
    let profile = profile(facts);
    let supported = supported(components, facts);
    let recommended = supported
        .iter()
        .find(|c| default_on(c, profile) == "recommended");
    let suggestion = match (recommended, supported.is_empty()) {
        (Some(_), _) => Suggestion::Recommended,
        (None, true) => Suggestion::Unsupported,
        (None, false) => Suggestion::Optional,
    };
    let install = recommended
        .or_else(|| supported.first())
        .map(|c| ComponentRef::from(*c));
    (suggestion, install)
}

/// Each capability of the catalogue: existing (with the evidence) or not.
///
/// Zeabur reports that an installation is there, without judging it against the catalogue's
/// settings. A missing capability lists what could be installed, and whether the catalogue
/// recommends it for a cluster of this shape or keeps it optional. A probe the person may not run
/// finds nothing, so [`ClusterFacts::unreadable`] tells the interface to say so.
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
                install: None,
            });
        let running = running(&component.detection, facts);
        let mut evidence = crds(&component.detection, facts);
        if !evidence.is_empty() || !running.is_empty() {
            capability.existing = true;
            capability.installations.push(Installation {
                running: !running.is_empty(),
                ..installed::installation(component, facts, distribution(facts))
            });
        }
        evidence.extend(running);
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
            let (suggestion, install) = suggestion(&capability.components, facts);
            capability.suggestion = Some(suggestion);
            capability.install = install;
        }
    }
    capabilities
}

/// The CRDs in `facts` that `detection` names.
fn crds(detection: &Detection, facts: &ClusterFacts) -> Vec<String> {
    detection
        .crds
        .iter()
        .filter(|c| facts.crds.contains(*c))
        .map(|crd| format!("CustomResourceDefinition {crd}"))
        .collect()
}

/// Everything else in `facts` that matches `detection`: what goes away with the component.
fn running(detection: &Detection, facts: &ClusterFacts) -> Vec<String> {
    let mut found: Vec<String> = detection
        .api_services
        .iter()
        .filter(|s| facts.api_services.contains(*s))
        .map(|service| format!("APIService {service}"))
        .collect();
    found.extend(
        detection
            .gateway_controllers
            .iter()
            .filter(|c| facts.gateway_controllers.contains(*c))
            .map(|controller| format!("GatewayClass controller {controller}")),
    );
    found.extend(
        detection
            .ingress_controllers
            .iter()
            .filter(|c| facts.ingress_controllers.contains(*c))
            .map(|controller| format!("IngressClass controller {controller}")),
    );
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
    found.extend(
        detection
            .addons_on(Distribution::K3s)
            .filter(|addon| {
                facts
                    .k3s_helm_charts
                    .contains_key(&format!("kube-system/{addon}"))
            })
            .map(|addon| format!("k3s add-on {addon}")),
    );
    found.extend(
        detection
            .addons_on(Distribution::Rke2)
            .filter(|addon| {
                facts
                    .rke2_packaged_charts
                    .contains_key(&format!("kube-system/{addon}"))
            })
            .map(|addon| format!("RKE2 add-on {addon}")),
    );
    for wanted in &detection.workloads {
        let Some(wanted_kind) = workload_kind(wanted.kind) else {
            continue;
        };
        if facts.workload_labels.iter().any(|(kind, labels)| {
            kind == wanted_kind
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
            found.push(format!("{wanted_kind} with {}", selector.join(",")));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The types that read the catalog come from the release whose bundle this build embeds:
    /// Cargo.lock pins zeabur-catalog-format to the commit of the flake's `catalog` input.
    #[test]
    fn the_format_crate_is_from_the_embedded_catalog_commit() {
        let lock = include_str!("../../../Cargo.lock");
        let source = lock
            .split("[[package]]")
            .find(|p| p.contains("\nname = \"zeabur-catalog-format\"\n"))
            .and_then(|p| p.lines().find_map(|l| l.strip_prefix("source = ")))
            .unwrap();
        assert!(
            source.ends_with(&format!("#{CATALOGUE_COMMIT}\"")),
            "{source} is not commit {CATALOGUE_COMMIT}"
        );
    }

    #[test]
    fn every_catalogue_file_parses_and_has_a_detection_rule() {
        let components = catalogue().unwrap();
        // The capability comes from `provides`, not from a folder.
        let traefik = components.iter().find(|c| c.id == "traefik").unwrap();
        assert_eq!(traefik.capability, "ingress");
        for component in &components {
            let d = &component.detection;
            let rules = d.crds.len()
                + d.api_services.len()
                + d.gateway_controllers.len()
                + d.ingress_controllers.len()
                + d.helm_charts.len()
                + d.addons.len()
                + d.workloads.len();
            assert!(rules > 0, "{} has no detection rule", component.id);
            assert!(
                rules > d.crds.len(),
                "{} is found only by CRDs, which outlive it",
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

    fn suggestion_of(facts: &ClusterFacts, capability: &str) -> Option<Suggestion> {
        detect(&catalogue().unwrap(), facts)
            .into_iter()
            .find(|c| c.name == capability)
            .and_then(|c| c.suggestion)
    }

    #[test]
    fn node_updates_are_unsupported_on_kubesolo() {
        let mut facts = ClusterFacts {
            version: "v1.35.7+kubesolo-v1.2.3".to_owned(),
            nodes: Some(1),
            ..ClusterFacts::default()
        };
        assert_eq!(distribution(&facts), Some("kubesolo"));
        assert_eq!(
            suggestion_of(&facts, "node-updates"),
            Some(Suggestion::Unsupported)
        );
        assert_eq!(
            suggestion_of(&facts, "certificates"),
            Some(Suggestion::Recommended)
        );
        facts.version = "v1.36.5+k3s1".to_owned();
        assert_ne!(
            suggestion_of(&facts, "node-updates"),
            Some(Suggestion::Unsupported)
        );
    }

    #[test]
    fn a_distribution_the_catalogue_does_not_name_is_not_held_against_a_component() {
        let facts = ClusterFacts {
            version: "v1.36.2".to_owned(),
            nodes: Some(3),
            ..ClusterFacts::default()
        };
        assert_eq!(distribution(&facts), None);
        for capability in ["certificates", "node-updates"] {
            assert_ne!(
                suggestion_of(&facts, capability),
                Some(Suggestion::Unsupported),
                "{capability}"
            );
        }
    }

    fn component(defaults: &str, kube_version: &str) -> Component {
        Component {
            id: "cert-manager".to_owned(),
            capability: "certificates".to_owned(),
            display_name: "cert-manager".to_owned(),
            homepage: String::new(),
            license: String::new(),
            summary: BTreeMap::new(),
            defaults: BTreeMap::from([("single-node".to_owned(), defaults.to_owned())]),
            distributions: vec!["k3s".to_owned()],
            kube_version: kube_version.to_owned(),
            detection: Detection::default(),
            tested_version: None,
            unsupported: None,
        }
    }

    #[test]
    fn a_component_outside_its_kube_version_or_with_an_unknown_default_is_not_suggested() {
        let facts = ClusterFacts {
            version: "v1.36.5+k3s1".to_owned(),
            nodes: Some(1),
            ..ClusterFacts::default()
        };
        let one = |c: Component| suggestion(&[c], &facts).0;
        assert_eq!(
            one(component("recommended", ">=1.34.0-0")),
            Suggestion::Recommended
        );
        assert_eq!(
            one(component("optional", ">=1.34.0-0")),
            Suggestion::Optional
        );
        assert_eq!(
            one(component("recommended", ">=1.37.0-0")),
            Suggestion::Unsupported
        );
        assert_eq!(
            one(component("required", ">=1.34.0-0")),
            Suggestion::Unsupported
        );
    }

    #[test]
    fn a_missing_capability_installs_a_recommended_component_that_runs_here() {
        let facts = ClusterFacts {
            version: "v1.36.5+k3s1".to_owned(),
            nodes: Some(1),
            ..ClusterFacts::default()
        };
        let named = |id: &str, defaults, kube_version| Component {
            id: id.to_owned(),
            ..component(defaults, kube_version)
        };
        let offered = |components: &[Component]| suggestion(components, &facts).1.map(|c| c.id);
        let optional = named("optional", "optional", ">=1.34.0-0");
        let recommended = named("recommended", "recommended", ">=1.34.0-0");
        let too_new = named("too-new", "recommended", ">=1.37.0-0");
        assert_eq!(
            offered(&[optional.clone(), recommended]).as_deref(),
            Some("recommended")
        );
        assert_eq!(
            offered(&[too_new.clone(), optional]).as_deref(),
            Some("optional")
        );
        assert_eq!(offered(&[too_new]), None);
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
            running(&detection, &facts),
            ["Helm release cert-manager/cert-manager"]
        );
        // A release named otherwise says nothing about its chart, which is never decoded.
        let mut renamed = ClusterFacts::default();
        renamed.helm_releases.insert("security/certs".to_owned());
        assert_eq!(running(&detection, &renamed), Vec::<String>::new());
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
        assert_eq!(running(&detection, &facts), ["Helm chart cert-manager"]);
        // A different chart that shares the prefix does not count.
        let mut other = ClusterFacts::default();
        other.workload_labels.push((
            "Deployment".to_owned(),
            BTreeMap::from([(
                "helm.sh/chart".to_owned(),
                "cert-manager-webhook-1.0.0".to_owned(),
            )]),
        ));
        assert_eq!(running(&detection, &other), Vec::<String>::new());
    }
}
