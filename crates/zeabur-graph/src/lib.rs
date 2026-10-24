#![doc = include_str!("../README.md")]

mod attribution;
mod connections;
mod env;
pub mod fields;
mod health;
pub mod manifest;
mod mounts;
mod network;
pub mod pod_security;
mod pods;
mod project;
mod restart;
mod snapshot;
mod workload;

pub use attribution::{attribute, attribution, ingress_backends, is_machinery_secret, relations};
pub use connections::{connections, data_store, named_hosts, names_host};
pub use env::{env, env_from};
pub use health::{failing, pod_health, status};
pub use mounts::mounts;
pub use network::{port_protocol, ports};
pub use pods::{pods_of, summary as pod_summary};
pub use project::{endpoint, project, project_with, provenance, with_http_routes};
pub use snapshot::Snapshot;
pub use workload::{
    Workload, classify, classify_with, deployment_phase, phase, replicas, summarise,
    summarise_with, workloads,
};
