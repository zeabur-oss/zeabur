//! What installing a catalog component writes, and whether what it wrote is ready.
//!
//! An install is a list of [`Step`]s, one per release, in the catalog's install order
//! [installers]: the components the asked one needs and does not find on the cluster, then it.
//! A step holds the objects to server-side apply: the release's namespace with its Pod Security
//! label, then a K3s/RKE2 `HelmChart` for a chart [k3s-helm] or the objects of an inline release.
//! A step waits for the one before it, because a release often uses the CRDs of an earlier one.
//!
//! Readiness follows kstatus [kstatus] through `zeabur_model::readiness`. A `HelmChart` has no
//! `observedGeneration`, and helm-controller replaces its Job whenever the chart changes, so a
//! chart is ready when the Job its status names was created at or after Zeabur's last write of
//! the chart and has completed [helm-controller].
//!
//! [installers]: https://github.com/zeabur-oss/catalog/blob/main/docs/reference/installers.mdx
//! [k3s-helm]: https://docs.k3s.io/helm
//! [kstatus]: https://github.com/kubernetes-sigs/cli-utils/blob/v0.37.2/pkg/kstatus/README.md
//! [helm-controller]: https://github.com/k3s-io/helm-controller/blob/v0.16.27/pkg/controllers/chart/chart.go

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeabur_kube::DynamicObject;
use zeabur_kube::k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use zeabur_kube::k8s_openapi::api::batch::v1::Job;
use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, Pod};
use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::k8s_openapi::{Metadata, NamespaceResourceScope, Resource};
use zeabur_model::PodSecurityLevel;
use zeabur_model::readiness::{self, Reason, Status};

use crate::api::{ComponentFile, Health, Release, ReleaseType, Snapshot};
use crate::install::{merge, parse_values};

/// The label that marks what Zeabur installed, with [`MANAGED_BY_ZEABUR`].
pub const MANAGED_BY: &str = "app.kubernetes.io/managed-by";
/// Its value.
pub const MANAGED_BY_ZEABUR: &str = "zeabur";
/// The label naming the component an object belongs to.
pub const COMPONENT: &str = "catalog.zeabur.com/component";
/// The label naming the release an object belongs to.
pub const RELEASE: &str = "catalog.zeabur.com/release";
/// The annotation naming the component file's `sourceDigest` an object was written from.
pub const SOURCE_DIGEST: &str = "catalog.zeabur.com/source-digest";
/// The annotation naming the catalog release an object was written from.
pub const CATALOG_VERSION: &str = "catalog.zeabur.com/version";
/// Where `HelmChart`s go: the namespace K3s's and RKE2's helm-controller watch.
pub const CHART_NAMESPACE: &str = "kube-system";
/// The field manager Zeabur writes as.
pub const FIELD_MANAGER: &str = "zeabur";

/// The cluster an install is for.
#[derive(Debug, Clone, Copy)]
pub struct Target<'a> {
    /// `single-node` or `multi-node`.
    pub profile: &'a str,
    /// `k3s`, `rke2` or `kubesolo`; `None` for any other cluster.
    pub distribution: Option<&'a str>,
    /// The nodes' platform, `linux/amd64`.
    pub platform: &'a str,
    /// The capabilities the cluster has.
    pub capabilities: &'a BTreeSet<String>,
    /// The components the cluster has.
    pub components: &'a BTreeSet<String>,
}

/// One release to apply.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// The component.
    pub component: String,
    /// The release's name in the component file.
    pub release: String,
    /// The objects to apply, in order.
    pub objects: Vec<Desired>,
}

/// One object to apply.
#[derive(Debug, Clone, PartialEq)]
pub struct Desired {
    /// The object; `types` is always set.
    pub object: DynamicObject,
    /// The namespace it goes in when its kind is namespaced and it names none.
    pub namespace: String,
}

/// What to know before installing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// The catalog's e2e has not passed the component on this distribution and platform.
    Untested {
        /// The component.
        component: String,
    },
    /// The component's namespaces run pods with more than the `restricted` Pod Security level.
    Privileged {
        /// The component.
        component: String,
        /// `baseline` or `privileged`.
        level: PodSecurityLevel,
    },
}

/// An install.
#[derive(Debug, Clone, PartialEq)]
pub struct Install {
    /// The components it installs, in order: the ones needed, then the one asked for.
    pub components: Vec<String>,
    /// The releases, in order.
    pub steps: Vec<Step>,
    /// What to know.
    pub warnings: Vec<Warning>,
}

/// Why a component cannot be installed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// The catalog has no such component.
    #[error("Zeabur Catalog has no component {0}")]
    Unknown(String),
    /// Zeabur cannot install it yet.
    #[error("Zeabur cannot install {component} yet: it {reason}")]
    Unsupported {
        /// The component.
        component: String,
        /// What it has.
        reason: String,
    },
    /// The catalog does not offer it here.
    #[error("Zeabur Catalog does not offer {component} on {place}")]
    NotHere {
        /// The component.
        component: String,
        /// The profile or distribution.
        place: String,
    },
    /// No component provides a capability it needs.
    #[error("no component of Zeabur Catalog provides {0}")]
    NoProvider(String),
}

/// How to install `id` on `target`.
///
/// # Errors
///
/// The component or one it needs is unknown, not offered here, or uses what Zeabur cannot
/// install yet.
pub fn install(snapshot: &Snapshot, id: &str, target: &Target<'_>) -> Result<Install, Refusal> {
    let mut order = Vec::new();
    install_order(snapshot, id, target, &mut order, &mut BTreeSet::new())?;
    let mut steps = Vec::new();
    let mut warnings = Vec::new();
    for component in &order {
        let file = snapshot
            .components
            .get(component)
            .ok_or_else(|| Refusal::Unknown(component.clone()))?;
        check(file, target)?;
        if let Some(distribution) = target.distribution
            && !file.tested_on(&snapshot.index, distribution, target.platform)
        {
            warnings.push(Warning::Untested {
                component: component.clone(),
            });
        }
        let level = file.declaration.security.pod_security;
        if level != PodSecurityLevel::Restricted {
            warnings.push(Warning::Privileged {
                component: component.clone(),
                level,
            });
        }
        steps.extend(steps_of(
            file,
            &snapshot.index.catalog.version,
            target.profile,
        )?);
    }
    Ok(Install {
        components: order,
        steps,
        warnings,
    })
}

/// Appends what installing `id` installs: what it needs and the cluster lacks, then `id`.
fn install_order(
    snapshot: &Snapshot,
    id: &str,
    target: &Target<'_>,
    order: &mut Vec<String>,
    visiting: &mut BTreeSet<String>,
) -> Result<(), Refusal> {
    if order.iter().any(|c| c == id) || !visiting.insert(id.to_owned()) {
        return Ok(());
    }
    let file = snapshot
        .components
        .get(id)
        .ok_or_else(|| Refusal::Unknown(id.to_owned()))?;
    for reference in &file.declaration.requires {
        let needed = match (&reference.component, &reference.capability) {
            (Some(component), _) if target.components.contains(component) => continue,
            (Some(component), _) => component.clone(),
            (None, Some(capability)) if target.capabilities.contains(capability) => continue,
            (None, Some(capability)) => snapshot
                .provider(capability, target.profile)
                .map(|c| c.id.clone())
                .ok_or_else(|| Refusal::NoProvider(capability.clone()))?,
            (None, None) => continue,
        };
        install_order(snapshot, &needed, target, order, visiting)?;
    }
    order.push(id.to_owned());
    Ok(())
}

/// Refuses what the catalog does not offer here and what Zeabur cannot install yet.
fn check(file: &ComponentFile, target: &Target<'_>) -> Result<(), Refusal> {
    let d = &file.declaration;
    let unsupported = |reason: &str| Refusal::Unsupported {
        component: file.id.clone(),
        reason: reason.to_owned(),
    };
    match d.defaults.get(target.profile).map(String::as_str) {
        Some("recommended" | "optional") => {}
        _ => {
            return Err(Refusal::NotHere {
                component: file.id.clone(),
                place: format!("a {} cluster", target.profile),
            });
        }
    }
    if let Some(distribution) = target.distribution
        && !d.distributions.iter().any(|x| x == distribution)
    {
        return Err(Refusal::NotHere {
            component: file.id.clone(),
            place: distribution.to_owned(),
        });
    }
    if !d.inputs.is_empty() {
        return Err(unsupported("asks for values"));
    }
    if !d.contributes.is_empty() {
        return Err(unsupported("adds values to other components"));
    }
    if d.releases
        .iter()
        .any(|r| r.kind != ReleaseType::Helm && r.kind != ReleaseType::Inline)
    {
        return Err(unsupported("installs manifests from URLs"));
    }
    Ok(())
}

/// The steps of one component alone, which a removal deletes in reverse.
///
/// # Errors
///
/// A release does not read.
pub fn steps_of(
    file: &ComponentFile,
    catalog_version: &str,
    profile: &str,
) -> Result<Vec<Step>, Refusal> {
    let broken = |message: String| Refusal::Unsupported {
        component: file.id.clone(),
        reason: format!("does not read: {message}"),
    };
    let mut namespaces = BTreeSet::new();
    let mut steps = Vec::new();
    for release in &file.declaration.releases {
        let mut objects = Vec::new();
        if namespaces.insert(release.namespace.clone()) {
            let level = file.declaration.security.pod_security;
            objects.push(Desired {
                object: namespace(&release.namespace, level).map_err(broken)?,
                namespace: String::new(),
            });
        }
        let made = match release.kind {
            ReleaseType::Helm => vec![helm_chart(release, profile).map_err(broken)?],
            ReleaseType::Inline => inline(release).map_err(broken)?,
            ReleaseType::Manifest | ReleaseType::Unknown => Vec::new(),
        };
        for mut object in made {
            mark(&mut object.metadata, file, release, catalog_version);
            objects.push(Desired {
                object,
                namespace: release.namespace.clone(),
            });
        }
        steps.push(Step {
            component: file.id.clone(),
            release: release.name.clone(),
            objects,
        });
    }
    Ok(steps)
}

/// `object` as a [`DynamicObject`], with its `apiVersion` and `kind`.
fn dynamic<K: Resource + Serialize>(object: &K) -> Result<DynamicObject, String> {
    let mut value = serde_json::to_value(object).map_err(|e| e.to_string())?;
    if let Some(fields) = value.as_object_mut() {
        fields.insert("apiVersion".to_owned(), K::API_VERSION.into());
        fields.insert("kind".to_owned(), K::KIND.into());
    }
    serde_json::from_value(value).map_err(|e| e.to_string())
}

/// A namespace with its Pod Security level [pss].
///
/// [pss]: https://kubernetes.io/docs/concepts/security/pod-security-admission/#pod-security-admission-labels-for-namespaces
fn namespace(name: &str, level: PodSecurityLevel) -> Result<DynamicObject, String> {
    dynamic(&Namespace {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            labels: Some(BTreeMap::from([(
                PodSecurityLevel::ENFORCE.to_owned(),
                level.as_str().to_owned(),
            )])),
            ..ObjectMeta::default()
        },
        ..Namespace::default()
    })
}

/// A chart release as a `HelmChart`, with the release's values and the profile's merged over
/// them. `failurePolicy: abort` keeps a failed install or upgrade as it is, for a person to see,
/// rather than helm-controller's default of uninstalling and installing again.
fn helm_chart(release: &Release, profile: &str) -> Result<DynamicObject, String> {
    let chart = release
        .chart
        .clone()
        .ok_or("a chart release names no chart")?;
    let version = release
        .version
        .clone()
        .ok_or("a chart release names no version")?;
    let mut values = parse_values(release.values.as_deref())?;
    if let Some(over) = release.profiles.get(profile) {
        merge(&mut values, parse_values(Some(over))?);
    }
    let values_content = if values.as_object().is_none_or(|v| !v.is_empty()) {
        Some(serde_saphyr::to_string(&values).map_err(|e| e.to_string())?)
    } else {
        None
    };
    dynamic(&HelmChart {
        metadata: ObjectMeta {
            name: Some(release.helm_name().to_owned()),
            namespace: Some(CHART_NAMESPACE.to_owned()),
            ..ObjectMeta::default()
        },
        spec: HelmChartSpec {
            chart,
            repo: release.repo.clone(),
            version: Some(version),
            target_namespace: Some(release.namespace.clone()),
            failure_policy: Some(FailurePolicy::Abort),
            values_content,
        },
        status: None,
    })
}

/// The objects of an inline release.
fn inline(release: &Release) -> Result<Vec<DynamicObject>, String> {
    let text = release.manifests.as_deref().unwrap_or_default();
    let documents: Vec<Value> = serde_saphyr::from_multiple(text).map_err(|e| e.to_string())?;
    let mut objects = Vec::new();
    for document in documents.into_iter().filter(|d| !d.is_null()) {
        let object: DynamicObject = serde_json::from_value(document).map_err(|e| e.to_string())?;
        if object.types.is_none() || object.metadata.name.is_none() {
            return Err("an inline object has no kind or name".to_owned());
        }
        objects.push(object);
    }
    Ok(objects)
}

/// Labels and annotates an object as the component's.
fn mark(metadata: &mut ObjectMeta, file: &ComponentFile, release: &Release, catalog_version: &str) {
    metadata.labels.get_or_insert_default().extend([
        (MANAGED_BY.to_owned(), MANAGED_BY_ZEABUR.to_owned()),
        (COMPONENT.to_owned(), file.id.clone()),
        (RELEASE.to_owned(), release.name.clone()),
    ]);
    metadata.annotations.get_or_insert_default().extend([
        (SOURCE_DIGEST.to_owned(), file.source_digest.clone()),
        (CATALOG_VERSION.to_owned(), catalog_version.to_owned()),
    ]);
}

/// The installed components that need `id`, which keep it from being removed.
#[must_use]
pub fn needed_by<'a>(
    snapshot: &'a Snapshot,
    id: &str,
    installed: &BTreeSet<String>,
) -> Vec<&'a str> {
    snapshot
        .needed_by(id)
        .into_iter()
        .filter(|c| *c != id && installed.contains(*c))
        .collect()
}

/// How far one step, or one check, has come.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// Not written yet.
    Pending,
    /// Written; the controller works on it.
    Waiting,
    /// Done.
    Ready,
    /// It will not get ready without a change: why.
    Failed(String),
}

/// A K3s/RKE2 `HelmChart`, with the fields Zeabur writes and the status readiness reads
/// [helm-controller-types].
///
/// [helm-controller-types]: https://github.com/k3s-io/helm-controller/blob/v0.16.27/pkg/apis/helm.cattle.io/v1/types.go
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HelmChart {
    /// Its metadata; `managedFields` tells when Zeabur last wrote it.
    #[serde(default)]
    pub metadata: ObjectMeta,
    /// What to install.
    #[serde(default)]
    pub spec: HelmChartSpec,
    /// Its status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<HelmChartStatus>,
}

impl Resource for HelmChart {
    const API_VERSION: &'static str = "helm.cattle.io/v1";
    const GROUP: &'static str = "helm.cattle.io";
    const KIND: &'static str = "HelmChart";
    const VERSION: &'static str = "v1";
    const URL_PATH_SEGMENT: &'static str = "helmcharts";
    type Scope = NamespaceResourceScope;
}

impl Metadata for HelmChart {
    type Ty = ObjectMeta;

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

/// `HelmChart.spec`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelmChartSpec {
    /// The chart's name in `repo`, or an OCI reference.
    #[serde(default)]
    pub chart: String,
    /// The chart repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// The chart version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The namespace of the release.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_namespace: Option<String>,
    /// What helm-controller does when an install or upgrade fails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_policy: Option<FailurePolicy>,
    /// The values, as YAML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values_content: Option<String>,
}

/// `HelmChart.spec.failurePolicy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FailurePolicy {
    /// Uninstall and install again, helm-controller's default.
    Reinstall,
    /// Keep the failed release as it is.
    Abort,
}

/// `HelmChart.status`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelmChartStatus {
    /// The Job that installs or upgrades it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_name: Option<String>,
    /// `JobCreated` and `Failed`.
    #[serde(default)]
    pub conditions: Vec<Condition>,
}

/// A status condition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Condition {
    /// Its type.
    #[serde(rename = "type")]
    pub kind: String,
    /// `True`, `False` or `Unknown`.
    pub status: String,
    /// Why, in words.
    #[serde(default)]
    pub message: Option<String>,
}

/// When Zeabur last wrote the object.
fn written_at(metadata: &ObjectMeta) -> Option<Timestamp> {
    metadata
        .managed_fields
        .iter()
        .flatten()
        .filter(|m| m.manager.as_deref() == Some(FIELD_MANAGER))
        .filter_map(|m| m.time.as_ref().map(|t| t.0))
        .max()
}

/// Whether a `HelmChart` is installed as last written: its `Failed` condition is not `True`, and
/// `job` (the Job its status names) was created at or after Zeabur's last write and completed.
#[must_use]
pub fn chart_progress(chart: &HelmChart, job: Option<&Job>) -> Progress {
    let conditions = chart
        .status
        .as_ref()
        .map_or(&[][..], |s| s.conditions.as_slice());
    if let Some(failed) = conditions
        .iter()
        .find(|c| c.kind == "Failed" && c.status == "True")
    {
        return Progress::Failed(failed.message.clone().unwrap_or_default());
    }
    let Some(job) = job else {
        return Progress::Waiting;
    };
    let created = job.metadata.creation_timestamp.as_ref().map(|t| t.0);
    let current = match (written_at(&chart.metadata), created) {
        (Some(written), Some(created)) => created >= truncate(written),
        (None, Some(_)) => true,
        (_, None) => false,
    };
    if !current {
        return Progress::Waiting;
    }
    let state = readiness::job(job);
    match (state.status, state.reason) {
        (Status::Current, Reason::JobComplete) => Progress::Ready,
        (Status::Failed, _) => Progress::Failed(format!(
            "the Helm Job {} failed",
            job.metadata.name.as_deref().unwrap_or_default()
        )),
        _ => Progress::Waiting,
    }
}

/// Kubernetes keeps timestamps to the second, so a Job created in the second of the write
/// counts as after it.
fn truncate(t: Timestamp) -> Timestamp {
    Timestamp::from_second(t.as_second()).unwrap_or(t)
}

/// Whether an inline object is ready: it exists, and a `Ready` condition, when it has one, is
/// `True`.
#[must_use]
pub fn object_progress(object: Option<&DynamicObject>) -> Progress {
    let Some(object) = object else {
        return Progress::Pending;
    };
    let conditions: Vec<Condition> = object
        .data
        .get("status")
        .and_then(|s| s.get("conditions"))
        .cloned()
        .and_then(|c| serde_json::from_value(c).ok())
        .unwrap_or_default();
    match conditions.iter().find(|c| c.kind == "Ready") {
        Some(c) if c.status == "True" => Progress::Ready,
        Some(_) => Progress::Waiting,
        None => Progress::Ready,
    }
}

/// The API resource of a health check's kind, as `(group, version, plural)` [installers].
///
/// [installers]: https://github.com/zeabur-oss/catalog/blob/main/docs/reference/installers.mdx#install-and-check
#[must_use]
pub fn health_resource(
    kind: &str,
) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    Some(match kind {
        "APIService" => ("apiregistration.k8s.io", "v1", "apiservices", "APIService"),
        "DaemonSet" => ("apps", "v1", "daemonsets", "DaemonSet"),
        "Deployment" => ("apps", "v1", "deployments", "Deployment"),
        "GatewayClass" => (
            "gateway.networking.k8s.io",
            "v1",
            "gatewayclasses",
            "GatewayClass",
        ),
        "Node.longhorn.io" => ("longhorn.io", "v1beta2", "nodes", "Node"),
        "Pod" => ("", "v1", "pods", "Pod"),
        "StatefulSet" => ("apps", "v1", "statefulsets", "StatefulSet"),
        _ => return None,
    })
}

/// Whether a health check passes on `objects`, the objects its resource names: each is Current
/// by kstatus for the workload kinds, or has the check's condition `True` for the others. No
/// object yet is waiting.
#[must_use]
pub fn check_progress(check: &Health, objects: &[Value], now: Timestamp) -> Progress {
    if objects.is_empty() {
        return Progress::Waiting;
    }
    let mut waiting = false;
    for object in objects {
        let state = match check.resource.kind.as_str() {
            "Deployment" => typed(object).map(|d: Deployment| readiness::deployment(&d)),
            "StatefulSet" => typed(object).map(|s: StatefulSet| readiness::stateful_set(&s)),
            "DaemonSet" => typed(object).map(|d: DaemonSet| readiness::daemon_set(&d)),
            "Pod" => typed(object).map(|p: Pod| readiness::pod(&p, now)),
            _ => {
                if object_condition(object, &check.condition) {
                    continue;
                }
                waiting = true;
                continue;
            }
        };
        match state.map(|s| s.status) {
            Some(Status::Current) => {}
            Some(Status::Failed) => return Progress::Failed(check.advice.clone()),
            _ => waiting = true,
        }
    }
    if waiting {
        Progress::Waiting
    } else {
        Progress::Ready
    }
}

fn typed<K: serde::de::DeserializeOwned>(object: &Value) -> Option<K> {
    serde_json::from_value(object.clone()).ok()
}

fn object_condition(object: &Value, condition: &str) -> bool {
    let conditions: Vec<Condition> = object
        .pointer("/status/conditions")
        .cloned()
        .and_then(|c| serde_json::from_value(c).ok())
        .unwrap_or_default();
    conditions
        .iter()
        .any(|c| c.kind == condition && c.status == "True")
}

/// A check's `timeout`, such as `60s` or `5m`, in seconds; 300 when it does not read.
#[must_use]
pub fn timeout_seconds(text: &str) -> i64 {
    let (number, unit) = text.split_at(
        text.find(|c: char| !c.is_ascii_digit())
            .unwrap_or(text.len()),
    );
    let n: i64 = number.parse().unwrap_or(0);
    match unit {
        "s" | "" if n > 0 => n,
        "m" if n > 0 => n * 60,
        "h" if n > 0 => n * 3600,
        _ => 300,
    }
}

/// The selector of a check as `key=value,…`, for a label-selected list.
#[must_use]
pub fn selector(labels: &BTreeMap<String, String>) -> String {
    labels
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;
    use crate::api;

    const FILES: &[(&str, &str)] = &[
        (
            "cert-manager",
            include_str!("../tests/api/cert-manager.json"),
        ),
        (
            "cloudnative-pg",
            include_str!("../tests/api/cloudnative-pg.json"),
        ),
        ("crowdsec", include_str!("../tests/api/crowdsec.json")),
        ("keda", include_str!("../tests/api/keda.json")),
        (
            "keda-http-add-on",
            include_str!("../tests/api/keda-http-add-on.json"),
        ),
        ("traefik", include_str!("../tests/api/traefik.json")),
        (
            "victoria-logs",
            include_str!("../tests/api/victoria-logs.json"),
        ),
        (
            "victoria-metrics-operator",
            include_str!("../tests/api/victoria-metrics-operator.json"),
        ),
    ];

    /// Zeabur Catalog v0.5.3 with the components of `FILES` only.
    fn snapshot() -> Snapshot {
        let mut index = api::index(include_bytes!("../tests/api/catalog.json")).unwrap();
        index
            .components
            .retain(|c| FILES.iter().any(|(id, _)| *id == c.id));
        let fetched: BTreeMap<String, ComponentFile> = FILES
            .iter()
            .map(|(id, text)| {
                (
                    (*id).to_owned(),
                    api::component(&index, id, text.as_bytes()).unwrap(),
                )
            })
            .collect();
        Snapshot::assemble(index, fetched, None, Timestamp::UNIX_EPOCH).unwrap()
    }

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    fn kind(object: &DynamicObject) -> &str {
        object.types.as_ref().map_or("", |t| t.kind.as_str())
    }

    fn label<'a>(object: &'a DynamicObject, key: &str) -> Option<&'a str> {
        object
            .metadata
            .labels
            .as_ref()?
            .get(key)
            .map(String::as_str)
    }

    fn plan(id: &str, capabilities: &[&str], components: &[&str]) -> Result<Install, Refusal> {
        let capabilities = set(capabilities);
        let components = set(components);
        let target = Target {
            profile: "single-node",
            distribution: Some("k3s"),
            platform: "linux/amd64",
            capabilities: &capabilities,
            components: &components,
        };
        install(&snapshot(), id, &target)
    }

    #[test]
    fn cert_manager_installs_its_chart_then_its_issuers() {
        let install = plan("cert-manager", &["ingress"], &[]).unwrap();
        assert_eq!(install.components, ["cert-manager"]);
        let releases: Vec<&str> = install.steps.iter().map(|s| s.release.as_str()).collect();
        assert_eq!(releases, ["cert-manager", "issuers"]);

        let chart = &install.steps[0].objects;
        let namespace = &chart[0].object;
        assert_eq!(kind(namespace), "Namespace");
        assert_eq!(
            label(namespace, PodSecurityLevel::ENFORCE),
            Some("restricted")
        );
        let helm: HelmChart = chart[1].object.clone().try_parse().unwrap();
        assert_eq!(kind(&chart[1].object), HelmChart::KIND);
        assert_eq!(helm.metadata.namespace.as_deref(), Some(CHART_NAMESPACE));
        assert_eq!(
            helm.spec.chart,
            "oci://quay.io/jetstack/charts/cert-manager"
        );
        assert_eq!(helm.spec.version.as_deref(), Some("v1.21.3"));
        assert_eq!(helm.spec.target_namespace.as_deref(), Some("cert-manager"));
        assert_eq!(helm.spec.failure_policy, Some(FailurePolicy::Abort));
        assert_eq!(label(&chart[1].object, COMPONENT), Some("cert-manager"));
        assert_eq!(label(&chart[1].object, MANAGED_BY), Some(MANAGED_BY_ZEABUR));
        assert_eq!(
            helm.metadata.annotations.unwrap_or_default()[CATALOG_VERSION],
            "0.5.3"
        );

        // The single-node values merge over the release's: both are in the chart's values.
        let values: Value =
            serde_saphyr::from_str(helm.spec.values_content.as_deref().unwrap()).unwrap();
        assert_eq!(values["crds"]["keep"], true);
        assert_eq!(values["resources"]["limits"]["memory"], "128Mi");
        assert_eq!(values["webhook"]["resources"]["limits"]["memory"], "64Mi");
        assert!(
            values["webhook"]["image"]["tag"]
                .as_str()
                .unwrap()
                .contains("@sha256:")
        );

        // The issuers' namespace is already written by the step before.
        let issuers: Vec<&str> = install.steps[1]
            .objects
            .iter()
            .map(|d| d.object.metadata.name.as_deref().unwrap())
            .collect();
        assert_eq!(issuers, ["letsencrypt-staging", "letsencrypt"]);
        assert_eq!(
            label(&install.steps[1].objects[0].object, RELEASE),
            Some("issuers")
        );
    }

    #[test]
    fn a_needed_capability_the_cluster_lacks_comes_first() {
        // The ingress provider, traefik, installs manifests from URLs, which Zeabur cannot yet.
        let refusal = plan("cert-manager", &[], &[]).unwrap_err();
        assert!(
            matches!(refusal, Refusal::Unsupported { component, .. } if component == "traefik")
        );

        let install = plan("cloudnative-pg", &["ingress"], &[]).unwrap();
        assert_eq!(install.components, ["cert-manager", "cloudnative-pg"]);
    }

    #[test]
    fn a_needed_component_comes_first_unless_installed() {
        let install = plan("keda-http-add-on", &[], &[]).unwrap();
        assert_eq!(install.components, ["keda", "keda-http-add-on"]);
        let install = plan("keda-http-add-on", &[], &["keda"]).unwrap();
        assert_eq!(install.components, ["keda-http-add-on"]);
    }

    #[test]
    fn components_zeabur_cannot_install_yet_are_refused() {
        let refusal = plan("crowdsec", &["ingress", "certificates"], &["traefik"]).unwrap_err();
        assert!(
            matches!(refusal, Refusal::Unsupported { component, .. } if component == "crowdsec")
        );
        assert!(matches!(
            plan("nothing", &[], &[]),
            Err(Refusal::Unknown(_))
        ));
    }

    #[test]
    fn a_distribution_the_component_does_not_list_is_refused() {
        let capabilities = set(&["ingress"]);
        let components = set(&[]);
        let target = Target {
            profile: "single-node",
            distribution: Some("minikube"),
            platform: "linux/amd64",
            capabilities: &capabilities,
            components: &components,
        };
        let refusal = install(&snapshot(), "cert-manager", &target).unwrap_err();
        assert!(matches!(refusal, Refusal::NotHere { place, .. } if place == "minikube"));
    }

    #[test]
    fn an_inline_only_component_writes_its_objects() {
        let install = plan(
            "victoria-logs",
            &["certificates"],
            &["victoria-metrics-operator"],
        )
        .unwrap();
        assert_eq!(install.components, ["victoria-logs"]);
        let kinds: Vec<&str> = install.steps[0]
            .objects
            .iter()
            .map(|d| kind(&d.object))
            .collect();
        assert_eq!(kinds[0], "Namespace");
        assert!(kinds[1..].iter().all(|k| *k != HelmChart::KIND));
    }

    #[test]
    fn a_component_another_installed_one_needs_is_kept() {
        let snapshot = snapshot();
        let installed = set(&["keda", "keda-http-add-on"]);
        assert_eq!(
            needed_by(&snapshot, "keda", &installed),
            ["keda-http-add-on"]
        );
        assert_eq!(
            needed_by(&snapshot, "keda", &set(&["keda"])),
            Vec::<&str>::new()
        );
        // Needed through its capability.
        let installed = set(&["cert-manager", "cloudnative-pg"]);
        assert_eq!(
            needed_by(&snapshot, "cert-manager", &installed),
            ["cloudnative-pg"]
        );
    }

    fn chart(written: &str, conditions: &Value) -> HelmChart {
        serde_json::from_value(json!({
            "metadata": {
                "name": "cert-manager",
                "managedFields": [
                    {"manager": "zeabur", "operation": "Apply", "time": written},
                    {"manager": "helm-controller", "operation": "Update", "time": "2030-01-01T00:00:00Z"},
                ],
            },
            "status": {"jobName": "helm-install-cert-manager", "conditions": conditions},
        }))
        .unwrap()
    }

    fn job(created: &str, conditions: &Value) -> Job {
        serde_json::from_value(json!({
            "metadata": {"name": "helm-install-cert-manager", "creationTimestamp": created},
            "status": {"conditions": conditions},
        }))
        .unwrap()
    }

    fn complete() -> Value {
        json!([
            {"type": "SuccessCriteriaMet", "status": "True"},
            {"type": "Complete", "status": "True"},
        ])
    }

    #[test]
    fn a_chart_is_ready_when_the_job_after_the_last_write_completed() {
        let written = "2026-11-19T10:00:00.400Z";
        let c = chart(written, &json!([]));
        assert_eq!(chart_progress(&c, None), Progress::Waiting);
        // The Job of the write before.
        let old = job("2026-11-19T09:00:00Z", &complete());
        assert_eq!(chart_progress(&c, Some(&old)), Progress::Waiting);
        // Created in the same second as the write.
        let now = job("2026-11-19T10:00:00Z", &complete());
        assert_eq!(chart_progress(&c, Some(&now)), Progress::Ready);
        let running = job("2026-11-19T10:00:01Z", &json!([]));
        assert_eq!(chart_progress(&c, Some(&running)), Progress::Waiting);
    }

    #[test]
    fn a_failed_chart_or_job_is_a_failure() {
        let failed = chart(
            "2026-11-19T10:00:00Z",
            &json!([{"type": "Failed", "status": "True", "message": "chart not found"}]),
        );
        assert_eq!(
            chart_progress(&failed, None),
            Progress::Failed("chart not found".to_owned())
        );

        let c = chart("2026-11-19T10:00:00Z", &json!([]));
        let j = job(
            "2026-11-19T10:00:02Z",
            &json!([{"type": "Failed", "status": "True", "reason": "BackoffLimitExceeded"}]),
        );
        assert!(matches!(chart_progress(&c, Some(&j)), Progress::Failed(_)));
    }

    #[test]
    fn an_inline_object_is_ready_when_it_exists_and_says_ready() {
        assert_eq!(object_progress(None), Progress::Pending);
        let object = |v: Value| -> DynamicObject { serde_json::from_value(v).unwrap() };
        let plain = object(json!({"kind": "ConfigMap", "apiVersion": "v1", "metadata": {}}));
        assert_eq!(object_progress(Some(&plain)), Progress::Ready);
        let issuer = |status: &str| {
            object(json!({
                "metadata": {},
                "status": {"conditions": [{"type": "Ready", "status": status}]},
            }))
        };
        assert_eq!(object_progress(Some(&issuer("False"))), Progress::Waiting);
        assert_eq!(object_progress(Some(&issuer("True"))), Progress::Ready);
    }

    #[test]
    fn a_health_check_reads_its_workloads_by_kstatus() {
        let file = &snapshot().components["cert-manager"];
        let check = &file.declaration.health[0];
        let now = Timestamp::from_second(2_000_000_000).unwrap();
        assert_eq!(check_progress(check, &[], now), Progress::Waiting);

        let deployment = |available: i32| {
            json!({
                "apiVersion": "apps/v1",
                "kind": "Deployment",
                "metadata": {"name": "cert-manager", "generation": 1},
                "spec": {"replicas": 1, "selector": {}, "template": {}},
                "status": {
                    "observedGeneration": 1,
                    "replicas": 1,
                    "updatedReplicas": 1,
                    "readyReplicas": available,
                    "availableReplicas": available,
                    "conditions": [{"type": "Available", "status": if available > 0 { "True" } else { "False" }}],
                },
            })
        };
        assert_eq!(
            check_progress(check, &[deployment(0)], now),
            Progress::Waiting
        );
        assert_eq!(
            check_progress(check, &[deployment(1)], now),
            Progress::Ready
        );
    }

    #[test]
    fn a_check_timeout_reads_seconds_and_minutes() {
        assert_eq!(timeout_seconds("60s"), 60);
        assert_eq!(timeout_seconds("5m"), 300);
        assert_eq!(timeout_seconds("soon"), 300);
    }
}
