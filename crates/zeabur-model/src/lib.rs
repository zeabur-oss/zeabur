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
mod pod_security;
mod project;
mod provenance;
mod reload;
mod source;
mod trace;
mod workload;

pub use attribution::{Attribution, NamespaceAttribution, Ownership, WorkloadRef};
pub use container::{
    Container, ContainerResources, EnvVar, ImportedEnv, Mount, MountCategory, PendingRestart,
    ProbeHandler, ProbeSummary, Probes, Quantities, ReadOnlyReason,
};
pub use details::{DESCRIPTION_MAX, DISPLAY_NAME_MAX, ProjectDetails, ProjectIcon};
pub use diagnosis::{
    AffectedService, Diagnosis, DiagnosisKind, Fault, FaultOrigin, Fixer, Incident, IncidentKind,
    Message,
};
pub use gap::{Citation, Fix, FixKind, Gap, Severity};
pub use health::{Health, ProjectHealth, ServiceStatus, StatusWord};
pub use http_route::{
    BackendRef, GATEWAY_GROUP, HttpRoute, HttpRouteMetadata, HttpRouteRule, HttpRouteSpec,
    ParentReference, RouteCondition, RouteParentStatus, RouteStatus,
};
pub use network::{
    Endpoint, InferredFrom, Port, PortProtocol, ProtocolInference, Route, RouteMiddleware,
    RouteRule,
};
pub use pod_security::PodSecurityLevel;
pub use project::{
    Connection, ConnectionEvidence, DataStore, Project, Relation, RelationKind, Resource,
    ResourceKind, Service,
};
pub use provenance::{Provenance, Tool};
pub use reload::ConfigReload;
pub use source::{ObjectRef, Origin, Source, SourceKind};
pub use trace::{
    AddressKind, CertificateCheck, CertificateState, DnsState, DomainCheck, Hop, HopStatus,
    HopStep, RouteTrace,
};
pub use workload::{
    Classification, ContainerStatusSummary, ContainerTermination, ContainerWaiting, ManagedBy,
    Phase, PodSummary, Replicas, TaskSchedule, WorkloadKind, WorkloadSummary,
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
