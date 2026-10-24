//! A project: one namespace, seen as services.

use serde::{Deserialize, Serialize};
pub use zeabur_kernel::data_store::DataStore;

use crate::attribution::{Attribution, WorkloadRef};
use crate::container::Container;
use crate::network::{Endpoint, Route};
use crate::provenance::Provenance;
use crate::source::{ObjectRef, Origin};
use crate::workload::WorkloadSummary;

/// One namespace projected into services, the resources around them and how they relate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// The namespace.
    pub namespace: String,
    /// One service per workload, sorted by kind then name.
    pub services: Vec<Service>,
    /// Every Kubernetes Service that selects a service's pods, once, sorted by name; a service
    /// names its own in `endpointNames`.
    pub endpoints: Vec<Endpoint>,
    /// Every Ingress that routes to one of those Services, once, sorted by name; a service names
    /// its own in `routeNames`.
    pub routes: Vec<Route>,
    /// The ConfigMaps, Secrets, PVCs, Services and Ingresses, sorted by kind then name.
    pub resources: Vec<Resource>,
    /// Which services appear to call which, sorted by source then target.
    pub connections: Vec<Connection>,
    /// The server of the error pages and the maintenance page. Absent when the project has none,
    /// or has more than one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_server: Option<PageServer>,
}

/// The page server Zeabur Catalog's `error-pages` template deploys: the Service and the ConfigMap
/// with the label `zeabur.com/page-server: "true"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PageServer {
    /// The Service's name.
    pub service: String,
    /// The ConfigMap of the pages, by file name such as `503.html`. Absent when no ConfigMap, or
    /// more than one, has the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<String>,
}

/// One service appearing to call another.
///
/// Kubernetes has no field for "this workload talks to that one", so every connection is read
/// from evidence and says which. It describes configuration, never observed traffic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    /// The caller.
    pub from: WorkloadRef,
    /// The service called.
    pub to: WorkloadRef,
    /// True when the only evidence is text inside a variable's value, false when a reference
    /// field of a Kubernetes object supports it.
    pub inferred: bool,
    /// Why the connection is drawn, sorted.
    pub evidence: Vec<ConnectionEvidence>,
}

/// Why a [`Connection`] is drawn.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "basis", rename_all = "camelCase")]
pub enum ConnectionEvidence {
    /// The caller and a data store read the same Secret or ConfigMap, which usually holds the
    /// store's address or credentials.
    #[serde(rename_all = "camelCase")]
    SharedObject {
        /// The Secret or ConfigMap.
        object: ObjectRef,
    },
    /// A literal variable's value names a Kubernetes Service that selects the target's pods,
    /// such as `redis://redis:6379` or `http://api.shop.svc.cluster.local`.
    #[serde(rename_all = "camelCase")]
    EnvValue {
        /// The caller's container.
        container: String,
        /// The variable.
        variable: String,
        /// The Kubernetes Service named.
        service: String,
    },
}

/// A service's endpoints and routes, looked up by name.
///
/// A project lists each Kubernetes Service and Ingress once, and a service names its own
/// ([`Service::endpoint_names`], [`Service::route_names`]). Copied under every workload they
/// reach, they would grow with the square of the workloads when every Service selects every
/// workload. Nothing is lost by sharing them: an [`Endpoint`] and a [`Route`] are read from their
/// object alone. A named `targetPort` stays a name, resolved against a workload's container
/// ports only by the reader that needs it, and a route lists every rule of its Ingress, also
/// those for other workloads' Services.
impl Project {
    /// The endpoint named `name`.
    #[must_use]
    pub fn endpoint(&self, name: &str) -> Option<&Endpoint> {
        by_name(&self.endpoints, name, |e| &e.name)
    }

    /// The route named `name`.
    #[must_use]
    pub fn route(&self, name: &str) -> Option<&Route> {
        by_name(&self.routes, name, |r| &r.name)
    }

    /// The endpoints `service` names, in its order.
    pub fn endpoints_of<'a>(&'a self, service: &'a Service) -> impl Iterator<Item = &'a Endpoint> {
        service
            .endpoint_names
            .iter()
            .filter_map(|name| self.endpoint(name))
    }

    /// The routes `service` names, in its order.
    pub fn routes_of<'a>(&'a self, service: &'a Service) -> impl Iterator<Item = &'a Route> {
        service
            .route_names
            .iter()
            .filter_map(|name| self.route(name))
    }
}

/// The entry of `list`, sorted by `key`, whose key is `name`.
fn by_name<'a, T>(list: &'a [T], name: &str, key: impl Fn(&T) -> &String) -> Option<&'a T> {
    list.binary_search_by(|entry| key(entry).as_str().cmp(name))
        .ok()
        .and_then(|index| list.get(index))
}

/// A workload and what it uses: what people think of as "a service".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Service {
    /// Its resource name, `projects/<project>/services/<kind>/<name>`; see [`crate::id`].
    pub id: String,
    /// The workload.
    #[serde(flatten)]
    pub workload: WorkloadSummary,
    /// Its health and the word for it, from the workload and its pods.
    pub status: crate::ServiceStatus,
    /// Its pods, sorted by name.
    pub pods: Vec<crate::PodSummary>,
    /// Its init and regular containers, in that order.
    pub containers: Vec<Container>,
    /// The names of the Kubernetes Services selecting its pods, sorted by name; each is in
    /// the document's `endpoints`.
    pub endpoint_names: Vec<String>,
    /// The names of the Ingresses and `HTTPRoute`s routing to those Services, sorted by name;
    /// each is in the document's `routes`.
    pub route_names: Vec<String>,
    /// What kind of data store it is, when it is one: named like a well-known data store
    /// image, in the `databases` group, or a StatefulSet with persistent storage. Image names
    /// are a convention, so this is a hint for how to show the service, not a fact about it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_store: Option<DataStore>,
    /// Which of its ConfigMaps and Secrets Stakater Reloader restarts it for, from its
    /// annotations; absent when they ask for none. Whether Reloader runs is the cluster's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_reload: Option<crate::ConfigReload>,
    /// The strictest Pod Security Standards level its pod template meets; absent when it has
    /// none. Whether the namespace enforces a level is the namespace's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pod_security: Option<crate::PodSecurityLevel>,
    /// The user and groups its pods run as; absent when its template sets none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_as: Option<crate::RunAs>,
    /// What is going wrong with it, in plain words, sorted by kind then container. The
    /// projection leaves this empty: diagnoses need the namespace's events and the time of the
    /// read, so the API fills it in when it serves the project document.
    #[serde(default)]
    pub diagnoses: Vec<crate::Diagnosis>,
}

/// The kinds of resource attributed to workloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ResourceKind {
    /// A ConfigMap.
    ConfigMap,
    /// An Ingress.
    Ingress,
    /// A persistent volume claim.
    PersistentVolumeClaim,
    /// A Secret: metadata and type only, never its values.
    Secret,
    /// A Kubernetes Service.
    Service,
}

/// A resource of the namespace and the workloads it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    /// Its resource name, `projects/<project>/resources/<kind>/<name>`; see [`crate::id`].
    pub id: String,
    /// The resource's kind.
    pub kind: ResourceKind,
    /// The resource's name.
    pub name: String,
    /// Owned, shared or orphan, and by which workloads.
    pub attribution: Attribution,
    /// The tool that manages the resource, if any.
    pub provenance: Provenance,
    /// For a ConfigMap or Secret, how many keys it holds, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_count: Option<usize>,
    /// A ConfigMap or Secret of shared variables, labelled `zeabur.com/shared-variables: "true"`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shared_variables: bool,
    /// The resource object.
    pub origin: Origin,
}

/// How a workload uses a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum RelationKind {
    /// An `env` entry reads one key (`valueFrom`).
    EnvReference,
    /// An `envFrom` entry imports every key.
    EnvImport,
    /// A pod volume (directly, or as a projected volume source).
    Volume,
    /// A StatefulSet's claim template created the PVC (`<template>-<set>-<ordinal>`).
    ClaimTemplate,
    /// `imagePullSecrets`.
    ImagePullSecret,
    /// A Service's selector matches the workload's pod labels.
    Selector,
    /// An Ingress routes to a Service that selects the workload.
    Route,
}

/// One reference between a workload and a resource.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Relation {
    /// The workload.
    pub workload: WorkloadRef,
    /// The resource.
    pub object: ObjectRef,
    /// How the workload uses it.
    pub kind: RelationKind,
    /// Where the reference is written: in the workload, or for `Selector` and `Route` in the
    /// Service or Ingress.
    pub origin: Origin,
}

/// The resource kinds' Kubernetes names.
impl ResourceKind {
    /// Every kind, in [`Ord`] order.
    pub const ALL: [Self; 5] = [
        Self::ConfigMap,
        Self::Ingress,
        Self::PersistentVolumeClaim,
        Self::Secret,
        Self::Service,
    ];

    /// The Kubernetes kind name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConfigMap => "ConfigMap",
            Self::Ingress => "Ingress",
            Self::PersistentVolumeClaim => "PersistentVolumeClaim",
            Self::Secret => "Secret",
            Self::Service => "Service",
        }
    }

    /// The kind as a path segment: lower case (`configmap`); see [`crate::id`].
    #[must_use]
    pub const fn segment(self) -> &'static str {
        match self {
            Self::ConfigMap => "configmap",
            Self::Ingress => "ingress",
            Self::PersistentVolumeClaim => "persistentvolumeclaim",
            Self::Secret => "secret",
            Self::Service => "service",
        }
    }

    /// The kind named `text` in any case (`configmap`, `ConfigMap`).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str().eq_ignore_ascii_case(text))
    }
}
