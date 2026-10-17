#![doc = include_str!("../README.md")]

mod attribution;
mod container;
mod details;
mod diagnosis;
mod gap;
mod health;
mod http_route;
pub mod id;
mod network;
mod project;
mod provenance;
mod source;
mod workload;

pub use attribution::{Attribution, NamespaceAttribution, Ownership, WorkloadRef};
pub use container::{
    Container, ContainerResources, EnvVar, ImportedEnv, Mount, MountCategory, ProbeHandler,
    ProbeSummary, Probes, Quantities, ReadOnlyReason,
};
pub use details::{DESCRIPTION_MAX, DISPLAY_NAME_MAX, ProjectDetails, ProjectIcon};
pub use diagnosis::{Diagnosis, DiagnosisKind, Message};
pub use gap::{Citation, Fix, FixKind, Gap, Severity};
pub use health::{Health, ProjectHealth, ServiceStatus, StatusWord};
pub use http_route::{BackendRef, HttpRoute, HttpRouteMetadata, HttpRouteRule, HttpRouteSpec};
pub use network::{
    Endpoint, InferredFrom, Port, PortProtocol, ProtocolInference, Route, RouteRule,
};
pub use project::{
    Connection, ConnectionEvidence, DataStore, Project, Relation, RelationKind, Resource,
    ResourceKind, Service,
};
pub use provenance::{Provenance, Tool};
pub use source::{ObjectRef, Origin, Source, SourceKind};
pub use workload::{
    Classification, ContainerStatusSummary, ContainerTermination, ContainerWaiting, ManagedBy,
    Phase, PodSummary, Replicas, WorkloadKind, WorkloadSummary,
};

/// `skip_serializing_if` helper: a `false` flag is left out, so the wire form only names flags
/// that are set.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
const fn is_false(value: &bool) -> bool {
    !*value
}
