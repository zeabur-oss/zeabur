#![doc = include_str!("../README.md")]

/// Conventions of third-party tools: Helm, Argo CD, Flux, operators, Gateway API.
pub mod adapters;
mod attribution;
mod build_limits;
mod container;
mod database_updates;
mod details;
mod diagnosis;
mod gap;
mod health;
mod http_route;
pub mod id;
mod image_updates;
/// Names, quantities, JSON Patch and the values every layer shares.
pub mod kernel;
mod network;
mod pod_security;
mod project;
mod provenance;
/// Whether a workload or a pod has reached its spec, as kstatus computes it.
pub mod readiness;
mod reload;
mod source;
mod trace;
mod verdict;
mod workload;

pub use attribution::{Attribution, NamespaceAttribution, Ownership, WorkloadRef};
pub use build_limits::{ProjectBuildLimits, is_build_quantity};
pub use container::{
    Container, ContainerPort, ContainerResources, EnvVar, ImportedEnv, Mount, MountCategory,
    PendingRestart, ProbeHandler, ProbeSummary, Probes, Quantities, ReadOnlyReason,
};
pub use database_updates::{database_image_updates, database_poll_schedule};
pub use details::{DESCRIPTION_MAX, DISPLAY_NAME_MAX, ProjectDetails, ProjectIcon};
pub use diagnosis::{
    AffectedService, Diagnosis, DiagnosisGroup, DiagnosisKind, Fault, FaultOrigin, Fixer, Incident,
    IncidentKind, Message,
};
pub use gap::{Citation, Fix, FixKind, FixValues, Gap, Severity};
pub use health::{Health, ProjectHealth, ServiceStatus, StatusCause, StatusWord};
pub use http_route::{
    BackendRef, GATEWAY_GROUP, HttpRoute, HttpRouteMetadata, HttpRouteRule, HttpRouteSpec,
    ParentReference, RouteCondition, RouteParentStatus, RouteStatus,
};
pub use image_updates::{
    ImagePolicy, ImageUpdates, KEEL_ANNOTATIONS, KEEL_FORCE, KEEL_HOURLY, KEEL_MATCH_TAG,
    KEEL_POLICY, KEEL_POLL, KEEL_POLL_SCHEDULE, KEEL_TRIGGER, UpdateSetting, UpdateWindow, Weekday,
};
pub use network::{
    Endpoint, InferredFrom, OpenedPort, Port, PortAddress, PortAddressKind, PortProtocol,
    ProtocolInference, Route, RouteMiddleware, RouteRule, RouteTls,
};
pub use pod_security::{PodSecurityLevel, RunAs};
pub use project::{
    Connection, ConnectionEvidence, DataStore, PageServer, Project, Relation, RelationKind,
    Resource, ResourceKind, Service, ServiceRole, WorkloadSelector,
};
pub use provenance::{Provenance, Tool};
pub use reload::ConfigReload;
pub use source::{ObjectRef, Origin, Source, SourceKind};
pub use trace::{
    AddressKind, CertificateCheck, CertificateState, DnsState, DomainCheck, DomainSetup, Hop,
    HopStatus, HopStep, RouteTrace,
};
pub use verdict::{Headline, ServiceVerdict, Trouble};
pub use workload::{
    Classification, ContainerStatusSummary, ContainerTermination, ContainerWaiting, ManagedBy,
    Phase, PodSummary, Replicas, TaskConcurrency, TaskSchedule, WorkloadKind, WorkloadSummary,
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
