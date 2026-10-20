#![doc = include_str!("../README.md")]

mod diagnose;
mod event;
mod incident;
mod rules;
mod snippet;

pub use diagnose::{Namespace, RECENT, diagnose};
pub use event::Event;
pub use incident::{
    Component, ComponentKind, NodeState, Reach, Wide, components, incidents, link, link_project,
    node_states,
};
pub use rules::{
    BUILTIN, ClusterTraits, DanglingRoute, HealthChecks, IngressClassFact, IngressToHttpRoute,
    Input, RETIRED_INGRESS_CONTROLLERS, ResourceLimits, Rule, cluster_gaps, evaluate,
    retired_classes,
};
pub use snippet::snippet;
pub use zeabur_core::names::GatewayRef;
