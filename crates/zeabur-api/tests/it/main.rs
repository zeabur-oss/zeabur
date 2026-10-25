//! The API against an in-memory cluster, in one test binary
//! (<https://doc.rust-lang.org/cargo/reference/cargo-targets.html#integration-tests>); the
//! cluster tests on k3s are their own binary, `kubernetes`.

mod account;
mod api;
mod backups;
mod certificates;
mod cluster_name;
mod cluster_tools;
#[cfg(test)]
#[path = "../common/mod.rs"]
mod common;
mod creations;
mod databases;
mod deploy_from_git;
mod domains;
mod durability;
mod evidence;
mod exec;
mod files;
mod github_app;
mod images;
mod incidents;
mod issuers;
mod log_history;
mod maintenance;
mod members;
mod namespace;
mod node_shell;
mod notifications;
mod pod_security;
mod preview;
mod releases;
mod reloader;
mod rollout;
mod s3_destinations;
mod storage;
mod templates;
mod terminal;
mod timeline;
mod trace;
mod usage;
mod volume_restores;
mod writes;
