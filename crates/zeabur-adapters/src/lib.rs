#![doc = include_str!("../README.md")]

use std::collections::BTreeMap;

pub mod cluster_name;
pub mod group;
pub mod images;
pub mod operators;
pub mod provenance;
pub mod selector;

/// The label and annotation keys and the field manager, Zeabur's by default.
pub use zeabur_kernel::names::Conventions;
/// The field manager (`fieldManager`) Zeabur's writes are recorded under in `managedFields`.
pub use zeabur_kernel::names::FIELD_MANAGER;
/// The label that names the service a scheduled task was made from.
pub use zeabur_kernel::names::LABEL_TASK_OF;

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
