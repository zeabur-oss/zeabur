use std::collections::BTreeMap;

pub mod error_pages;
pub mod gateway;
pub mod group;
pub mod images;
pub mod operators;
pub mod provenance;
pub mod selector;
pub mod traefik;

/// The label and annotation keys and the field manager, Zeabur's by default.
pub use crate::kernel::names::Conventions;
/// The field manager (`fieldManager`) Zeabur's writes are recorded under in `managedFields`.
pub use crate::kernel::names::FIELD_MANAGER;
/// The label that names the service a scheduled task was made from.
pub use crate::kernel::names::{LABEL_SHARED_VARIABLES, LABEL_TASK_OF};

/// The object's controller: the owner reference with `controller: true`, of which there is at
/// most one [k8s-owners]. Other owners only keep the object from garbage collection.
///
/// [k8s-owners]: https://kubernetes.io/docs/concepts/overview/working-with-objects/owners-dependents/
#[must_use]
pub fn controller_of(
    meta: &k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta,
) -> Option<&k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference> {
    meta.owner_references
        .iter()
        .flatten()
        .find(|owner| owner.controller == Some(true))
}

/// Whether `claim` is named `<template>-<set>-<ordinal>`, the name a StatefulSet gives the claims
/// of its `volumeClaimTemplates` [k8s-sts-storage].
///
/// The rest after the prefix must be the ordinal alone, or a set named `db` would claim the PVCs
/// of one named `db-replica`.
///
/// [k8s-sts-storage]: https://kubernetes.io/docs/concepts/workloads/controllers/statefulset/#stable-storage
#[must_use]
pub fn created_by_claim_template(claim: &str, template: &str, set: &str) -> bool {
    claim
        .strip_prefix(template)
        .and_then(|rest| rest.strip_prefix('-'))
        .and_then(|rest| rest.strip_prefix(set))
        .and_then(|rest| rest.strip_prefix('-'))
        .is_some_and(|ordinal| {
            !ordinal.is_empty() && ordinal.bytes().all(|byte| byte.is_ascii_digit())
        })
}

/// The value of label `key`, when it is present and not empty.
///
/// An empty label value says nothing, so every convention here treats it as absent.
#[must_use]
pub fn label<'a>(labels: Option<&'a BTreeMap<String, String>>, key: &str) -> Option<&'a str> {
    labels
        .and_then(|labels| labels.get(key))
        .map(String::as_str)
        .filter(|value| !value.is_empty())
}
