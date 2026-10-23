//! Which workloads each resource belongs to.

use std::collections::BTreeMap;

use k8s_openapi::api::apps::v1::StatefulSet;
use k8s_openapi::api::core::v1::{PodTemplateSpec, Secret, Service as KubeService, Volume};
use k8s_openapi::api::networking::v1::Ingress;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use zeabur_adapters::error_pages;
use zeabur_adapters::provenance::is_helm_release_secret;
use zeabur_model::{
    Attribution, NamespaceAttribution, ObjectRef, Origin, Ownership, Relation, RelationKind,
    WorkloadRef,
};

use crate::snapshot::{Snapshot, name_of};
use crate::workload::{ContainerView, Workload, containers, workloads};

/// The ConfigMap kube-apiserver publishes into every namespace; nobody's configuration.
const ROOT_CA_CONFIG_MAP: &str = "kube-root-ca.crt";

/// The `type` of a service account token Secret.
const SERVICE_ACCOUNT_TOKEN_TYPE: &str = "kubernetes.io/service-account-token";

/// The ownership of a resource referenced by `owners`: none is an orphan, one owns it, more
/// share it. The owners come back sorted by kind then name, without duplicates.
#[must_use]
pub fn attribute(mut owners: Vec<WorkloadRef>) -> Attribution {
    owners.sort();
    owners.dedup();
    let ownership = match owners.len() {
        0 => Ownership::Orphan,
        1 => Ownership::Owned,
        _ => Ownership::Shared,
    };
    Attribution { ownership, owners }
}

/// Whether a Secret holds cluster or tool state rather than a person's configuration: a
/// service account token, or one of Helm's release records. Only its `type` is read.
#[must_use]
pub fn is_machinery_secret(secret: &Secret) -> bool {
    secret
        .type_
        .as_deref()
        .is_some_and(|kind| kind == SERVICE_ACCOUNT_TOKEN_TYPE || is_helm_release_secret(kind))
}

/// Every reference from a workload to a resource of the namespace, sorted, without duplicates.
///
/// A reference counts whether or not the resource exists: a dangling reference is still what
/// the workload says. Covered: `env` `valueFrom` and `envFrom` of every container, volumes
/// (also as projected volume sources), `imagePullSecrets`, the PVCs a StatefulSet's claim
/// templates created, the Services whose selector matches a workload's pod labels, and the
/// Ingresses routing to those Services.
#[must_use]
pub fn relations(snapshot: &Snapshot) -> Vec<Relation> {
    relations_of(snapshot, &workloads(snapshot))
}

/// [`relations`] of the workloads `all`.
///
/// Each join goes through an index rather than a scan, so the time follows the number of
/// relations found: a Service's selector is tried on the workloads carrying its rarest label
/// only, a claim template looks up the PVCs named after it, and an Ingress backend finds the
/// workloads its Service selects by the Service's name.
pub(crate) fn relations_of(snapshot: &Snapshot, all: &[Workload<'_>]) -> Vec<Relation> {
    let mut out = Vec::new();
    let claims = sorted_names(&snapshot.persistent_volume_claims, |claim| &claim.metadata);
    for &workload in all {
        if let Some(template) = workload.pod_template() {
            pod_template_relations(workload, template, &mut out);
        }
        if let Workload::StatefulSet(set) = workload {
            claim_template_relations(&claims, workload, set, &mut out);
        }
    }
    let selected_from = out.len();
    let labels = LabelIndex::new(all);
    for service in &snapshot.services {
        selector_relations(all, &labels, service, &mut out);
    }
    let mut selected: BTreeMap<&str, Vec<&WorkloadRef>> = BTreeMap::new();
    for relation in out.get(selected_from..).unwrap_or_default() {
        selected
            .entry(relation.object.name.as_str())
            .or_default()
            .push(&relation.workload);
    }
    let mut routes = Vec::new();
    for ingress in &snapshot.ingresses {
        route_relations(&selected, ingress, &mut routes);
    }
    out.append(&mut routes);
    // Relations that compare equal are identical, so an unstable sort gives the same list.
    out.sort_unstable();
    out.dedup();
    out
}

/// The names of `objects`, sorted, so that the names starting with a prefix are one range.
fn sorted_names<T>(objects: &[T], meta: impl Fn(&T) -> &ObjectMeta) -> Vec<&str> {
    let mut names: Vec<&str> = objects.iter().map(|object| name_of(meta(object))).collect();
    names.sort_unstable();
    names
}

/// Pod template label `(key, value)` → the positions of the workloads carrying it, ascending.
struct LabelIndex<'a>(BTreeMap<(&'a str, &'a str), Vec<usize>>);

impl<'a> LabelIndex<'a> {
    fn new(all: &[Workload<'a>]) -> Self {
        let mut index: BTreeMap<(&str, &str), Vec<usize>> = BTreeMap::new();
        for (position, workload) in all.iter().enumerate() {
            for (key, value) in pod_labels(*workload).into_iter().flatten() {
                index
                    .entry((key.as_str(), value.as_str()))
                    .or_default()
                    .push(position);
            }
        }
        Self(index)
    }

    /// The positions of the workloads that may match `selector`, ascending: those carrying its
    /// rarest pair, none when one of its pairs is carried by none.
    fn candidates<'s>(&'s self, selector: &'s BTreeMap<String, String>) -> &'s [usize] {
        selector
            .iter()
            .map(|(key, value)| {
                self.0
                    .get(&(key.as_str(), value.as_str()))
                    .map_or(&[][..], Vec::as_slice)
            })
            .min_by_key(|positions| positions.len())
            .unwrap_or_default()
    }
}

/// The labels of a workload's pod template.
fn pod_labels(workload: Workload<'_>) -> Option<&BTreeMap<String, String>> {
    workload
        .pod_template()
        .and_then(|template| template.metadata.as_ref())
        .and_then(|meta| meta.labels.as_ref())
}

/// One relation from `workload` to `kind`/`name`.
fn relation(
    workload: &WorkloadRef,
    kind: &str,
    name: &str,
    how: RelationKind,
    origin: Origin,
) -> Relation {
    Relation {
        workload: workload.clone(),
        object: ObjectRef::new(kind, name),
        kind: how,
        origin,
    }
}

fn pod_template_relations(
    workload: Workload<'_>,
    template: &PodTemplateSpec,
    out: &mut Vec<Relation>,
) {
    let Some(spec) = template.spec.as_ref() else {
        return;
    };
    let owner = workload.reference();
    let base = workload.pod_spec_path();
    for container in containers(spec) {
        container_relations(workload, &owner, container, out);
    }
    for volume in spec.volumes.iter().flatten() {
        volume_relations(workload, &owner, volume, out);
    }
    for secret in spec.image_pull_secrets.iter().flatten() {
        out.push(relation(
            &owner,
            "Secret",
            &secret.name,
            RelationKind::ImagePullSecret,
            workload.origin(format!("{base}.imagePullSecrets[name={}]", secret.name)),
        ));
    }
}

/// The `env` and `envFrom` references of one container.
fn container_relations(
    workload: Workload<'_>,
    owner: &WorkloadRef,
    container: ContainerView<'_>,
    out: &mut Vec<Relation>,
) {
    let path = container.path(workload);
    for variable in container.env {
        let Some(from) = variable.value_from.as_ref() else {
            continue;
        };
        let at = |selector: &str| {
            workload.origin(format!(
                "{path}.env[name={}].valueFrom.{selector}",
                variable.name
            ))
        };
        if let Some(reference) = &from.secret_key_ref {
            out.push(relation(
                owner,
                "Secret",
                &reference.name,
                RelationKind::EnvReference,
                at("secretKeyRef"),
            ));
        }
        if let Some(reference) = &from.config_map_key_ref {
            out.push(relation(
                owner,
                "ConfigMap",
                &reference.name,
                RelationKind::EnvReference,
                at("configMapKeyRef"),
            ));
        }
    }
    for (index, import) in container.env_from.iter().enumerate() {
        let at = |selector: &str| workload.origin(format!("{path}.envFrom[{index}].{selector}"));
        if let Some(reference) = &import.secret_ref {
            out.push(relation(
                owner,
                "Secret",
                &reference.name,
                RelationKind::EnvImport,
                at("secretRef"),
            ));
        }
        if let Some(reference) = &import.config_map_ref {
            out.push(relation(
                owner,
                "ConfigMap",
                &reference.name,
                RelationKind::EnvImport,
                at("configMapRef"),
            ));
        }
    }
}

/// The references of one pod volume, also as projected volume sources.
fn volume_relations(
    workload: Workload<'_>,
    owner: &WorkloadRef,
    volume: &Volume,
    out: &mut Vec<Relation>,
) {
    let base = workload.pod_spec_path();
    let path = format!("{base}.volumes[name={}]", volume.name);
    let at = |selector: &str| workload.origin(format!("{path}.{selector}"));
    if let Some(source) = &volume.secret
        && let Some(name) = source.secret_name.as_deref()
    {
        out.push(relation(
            owner,
            "Secret",
            name,
            RelationKind::Volume,
            at("secret"),
        ));
    }
    if let Some(source) = &volume.config_map {
        out.push(relation(
            owner,
            "ConfigMap",
            &source.name,
            RelationKind::Volume,
            at("configMap"),
        ));
    }
    if let Some(source) = &volume.persistent_volume_claim {
        out.push(relation(
            owner,
            "PersistentVolumeClaim",
            &source.claim_name,
            RelationKind::Volume,
            at("persistentVolumeClaim"),
        ));
    }
    let projected = volume
        .projected
        .iter()
        .flat_map(|projected| projected.sources.iter().flatten());
    for (index, source) in projected.enumerate() {
        if let Some(secret) = &source.secret {
            out.push(relation(
                owner,
                "Secret",
                &secret.name,
                RelationKind::Volume,
                at(&format!("projected.sources[{index}].secret")),
            ));
        }
        if let Some(config_map) = &source.config_map {
            out.push(relation(
                owner,
                "ConfigMap",
                &config_map.name,
                RelationKind::Volume,
                at(&format!("projected.sources[{index}].configMap")),
            ));
        }
    }
}

/// The PVCs a StatefulSet's claim templates created, matched by their name
/// `<template>-<set>-<ordinal>`.
fn claim_template_relations(
    claims: &[&str],
    workload: Workload<'_>,
    set: &StatefulSet,
    out: &mut Vec<Relation>,
) {
    let owner = workload.reference();
    let set_name = workload.name();
    let templates = set
        .spec
        .iter()
        .flat_map(|spec| spec.volume_claim_templates.iter().flatten());
    for template in templates {
        let template_name = name_of(&template.metadata);
        let prefix = format!("{template_name}-{set_name}-");
        let first = claims.partition_point(|claim| *claim < prefix.as_str());
        let named = claims
            .get(first..)
            .unwrap_or_default()
            .iter()
            .take_while(|claim| claim.starts_with(&prefix));
        for &claim_name in named {
            if created_by_claim_template(claim_name, template_name, set_name) {
                out.push(relation(
                    &owner,
                    "PersistentVolumeClaim",
                    claim_name,
                    RelationKind::ClaimTemplate,
                    workload.origin(format!("spec.volumeClaimTemplates[name={template_name}]")),
                ));
            }
        }
    }
}

/// Whether `claim` is named `<template>-<set>-<ordinal>`.
///
/// The rest after the prefix must be the ordinal alone, or a set named `db` would claim the PVCs
/// of one named `db-replica`.
pub(crate) fn created_by_claim_template(claim: &str, template: &str, set: &str) -> bool {
    claim
        .strip_prefix(template)
        .and_then(|rest| rest.strip_prefix('-'))
        .and_then(|rest| rest.strip_prefix(set))
        .and_then(|rest| rest.strip_prefix('-'))
        .is_some_and(|ordinal| {
            !ordinal.is_empty() && ordinal.bytes().all(|byte| byte.is_ascii_digit())
        })
}

/// The workloads whose pod labels a Service's selector matches. A Service without a selector
/// selects nothing (its endpoints are managed by hand).
fn selector_relations(
    all: &[Workload<'_>],
    labels: &LabelIndex<'_>,
    service: &KubeService,
    out: &mut Vec<Relation>,
) {
    let name = name_of(&service.metadata);
    let Some(selector) = service
        .spec
        .as_ref()
        .and_then(|spec| spec.selector.as_ref())
        .filter(|selector| !selector.is_empty())
    else {
        return;
    };
    for workload in labels
        .candidates(selector)
        .iter()
        .filter_map(|&position| all.get(position))
    {
        let labels = pod_labels(*workload);
        let matches = selector
            .iter()
            .all(|(key, value)| labels.and_then(|labels| labels.get(key)) == Some(value));
        if matches {
            out.push(relation(
                &workload.reference(),
                "Service",
                name,
                RelationKind::Selector,
                Origin::field("Service", name, "spec.selector".to_owned()),
            ));
        }
    }
}

/// An Ingress belongs to the workloads its backend Services select (`selected`, by Service
/// name).
fn route_relations(
    selected: &BTreeMap<&str, Vec<&WorkloadRef>>,
    ingress: &Ingress,
    out: &mut Vec<Relation>,
) {
    let name = name_of(&ingress.metadata);
    let kept: Vec<String> = error_pages::maintenance(&ingress.metadata)
        .unwrap_or_default()
        .values()
        .flat_map(error_pages::ingress_services)
        .collect();
    let kept_at = format!(
        "metadata.annotations[{}]",
        error_pages::MAINTENANCE_ANNOTATION
    );
    let backends = ingress_backends(ingress)
        .into_iter()
        .chain(kept.iter().map(|s| (s.as_str(), kept_at.clone())));
    for (service, path) in backends {
        for &workload in selected.get(service).into_iter().flatten() {
            out.push(relation(
                workload,
                "Ingress",
                name,
                RelationKind::Route,
                Origin::field("Ingress", name, path.clone()),
            ));
        }
    }
}

/// The Services an Ingress routes to, with the field path naming each.
#[must_use]
pub fn ingress_backends(ingress: &Ingress) -> Vec<(&str, String)> {
    let Some(spec) = ingress.spec.as_ref() else {
        return Vec::new();
    };
    let default = spec
        .default_backend
        .as_ref()
        .and_then(|backend| backend.service.as_ref())
        .map(|service| {
            (
                service.name.as_str(),
                "spec.defaultBackend.service".to_owned(),
            )
        });
    let rules = spec
        .rules
        .iter()
        .flatten()
        .enumerate()
        .flat_map(|(rule, value)| {
            value
                .http
                .iter()
                .flat_map(|http| http.paths.iter().enumerate())
                .filter_map(move |(path, entry)| {
                    entry.backend.service.as_ref().map(|service| {
                        (
                            service.name.as_str(),
                            format!("spec.rules[{rule}].http.paths[{path}].backend.service"),
                        )
                    })
                })
        });
    default.into_iter().chain(rules).collect()
}

/// The attribution of every ConfigMap, Secret, PVC, Service and Ingress of the namespace.
///
/// It is decided by references alone ([`relations`]): ownerReferences say which controller
/// garbage-collects an object, not which workload uses it, and a ConfigMap two Deployments
/// import has no ownerReference to either.
#[must_use]
pub fn attribution(snapshot: &Snapshot) -> NamespaceAttribution {
    let all = workloads(snapshot);
    attribution_of(snapshot, &all, &relations_of(snapshot, &all))
}

/// [`attribution`] from the workloads `all` and their [`relations`].
pub(crate) fn attribution_of(
    snapshot: &Snapshot,
    all: &[Workload<'_>],
    relations: &[Relation],
) -> NamespaceAttribution {
    let mut owners: BTreeMap<(&str, &str), Vec<&WorkloadRef>> = BTreeMap::new();
    for relation in relations {
        owners
            .entry((relation.object.kind.as_str(), relation.object.name.as_str()))
            .or_default()
            .push(&relation.workload);
    }
    let attribute_named = |kind: &str, name: &str| {
        let mut found = owners.get(&(kind, name)).cloned().unwrap_or_default();
        // A workload often reaches a resource more than one way; clone each owner once.
        found.sort_unstable();
        found.dedup();
        (
            name.to_owned(),
            attribute(found.into_iter().cloned().collect()),
        )
    };
    NamespaceAttribution {
        config_maps: snapshot
            .config_maps
            .iter()
            .map(|object| name_of(&object.metadata))
            .filter(|name| *name != ROOT_CA_CONFIG_MAP)
            .map(|name| attribute_named("ConfigMap", name))
            .collect(),
        secrets: snapshot
            .secrets
            .iter()
            .filter(|secret| !is_machinery_secret(secret))
            .map(|object| attribute_named("Secret", name_of(&object.metadata)))
            .collect(),
        persistent_volume_claims: snapshot
            .persistent_volume_claims
            .iter()
            .map(|object| attribute_named("PersistentVolumeClaim", name_of(&object.metadata)))
            .collect(),
        services: snapshot
            .services
            .iter()
            .map(|object| attribute_named("Service", name_of(&object.metadata)))
            .collect(),
        ingresses: snapshot
            .ingresses
            .iter()
            .map(|object| attribute_named("Ingress", name_of(&object.metadata)))
            .collect(),
        workloads: all.iter().map(|workload| workload.reference()).collect(),
    }
}
