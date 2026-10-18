#![doc = include_str!("../README.md")]

mod diagnose;
mod event;
mod rules;
mod snippet;

pub use diagnose::{Namespace, RECENT, diagnose};
pub use engawa_core::names::GatewayRef;
pub use event::Event;
pub use rules::{
    BUILTIN, ClusterTraits, HealthChecks, IngressClassFact, IngressToHttpRoute, Input,
    RETIRED_INGRESS_CONTROLLERS, ResourceLimits, Rule, cluster_gaps, evaluate, retired_classes,
};
pub use snippet::snippet;
