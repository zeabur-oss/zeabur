//! Which workloads a resource belongs to.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::workload::WorkloadKind;

/// How many workloads reference a resource, which decides where a client shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Ownership {
    /// Exactly one workload: the resource is shown inside it.
    Owned,
    /// Several: editing it from one workload affects the others, and a client must say so.
    Shared,
    /// None: the resource is shown among the project's other resources (an unmounted volume
    /// still costs money, so it must stay reachable).
    Orphan,
}

/// A workload, by kind and name.
///
/// The derived order is kind, then name, as strings: [`WorkloadKind`]'s variants are declared
/// in alphabetical order.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadRef {
    /// The workload's kind.
    pub kind: WorkloadKind,
    /// The workload's name.
    pub name: String,
}

/// The resolved ownership of one resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Attribution {
    /// Owned, shared or orphan.
    pub ownership: Ownership,
    /// The referencing workloads, sorted by kind then name, without duplicates; always present,
    /// empty for an orphan.
    pub owners: Vec<WorkloadRef>,
}

/// The attribution of every resource in one namespace, and the workloads it was computed from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceAttribution {
    /// ConfigMaps by name, except the `kube-root-ca.crt` kube-apiserver adds to every namespace.
    pub config_maps: BTreeMap<String, Attribution>,
    /// Secrets by name, except machinery Secrets (service account tokens, Helm release records).
    pub secrets: BTreeMap<String, Attribution>,
    /// Persistent volume claims by name.
    pub persistent_volume_claims: BTreeMap<String, Attribution>,
    /// Services by name.
    pub services: BTreeMap<String, Attribution>,
    /// Ingresses by name; they inherit the owners of the Services they route to.
    pub ingresses: BTreeMap<String, Attribution>,
    /// The workloads, sorted by kind then name. A Job a CronJob controls is one of its runs and
    /// is not listed.
    pub workloads: Vec<WorkloadRef>,
}
