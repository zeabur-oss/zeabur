#![doc = include_str!("../README.md")]

mod diagnose;
mod durability;
mod event;
mod incident;
mod rules;
mod trace;

pub use diagnose::{Namespace, RECENT, diagnose, missing_claim, shortage};
pub use durability::EphemeralData;
pub use event::Event;
pub use incident::{
    Component, ComponentKind, NodeState, Reach, Wide, components, incidents, link, link_project,
    node_states,
};
pub use rules::{
    BUILTIN, ClusterTraits, DanglingRoute, HealthChecks, IngressClassFact, IngressToHttpRoute,
    Input, NamespaceFact, RETIRED_INGRESS_CONTROLLERS, ResourceLimits, Rule, cluster_gaps,
    evaluate, retired_classes,
};
pub use trace::{
    Certificates, Resolved, TraceInput, diagnoses as trace_diagnoses, ingress_addresses,
    names_to_resolve, trace,
};
pub use zeabur_kernel::names::GatewayRef;
