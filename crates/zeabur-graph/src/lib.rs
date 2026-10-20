#![doc = include_str!("../README.md")]

mod attribution;
mod connections;
mod env;
pub mod fields;
mod health;
pub mod manifest;
mod mounts;
mod network;
mod pods;
mod project;
mod snapshot;
mod workload;

pub use attribution::{attribute, attribution, ingress_backends, is_machinery_secret, relations};
pub use connections::{connections, data_store, named_hosts, names_host};
pub use env::{env, env_from};
pub use health::{failing, pod_health, status};
pub use mounts::mounts;
pub use network::{port_protocol, ports};
pub use pods::pods_of;
pub use project::{endpoint, project, provenance};
pub use snapshot::Snapshot;
pub use workload::{Workload, classify, deployment_phase, phase, replicas, summarise, workloads};
