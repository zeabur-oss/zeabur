//! The API against an in-memory cluster, in one test binary
//! (<https://doc.rust-lang.org/cargo/reference/cargo-targets.html#integration-tests>); the
//! cluster tests on k3s are their own binary, `kubernetes`.

mod account;
mod api;
mod cluster_name;
#[cfg(test)]
#[path = "../common/mod.rs"]
mod common;
mod durability;
mod evidence;
mod exec;
mod incidents;
mod members;
mod namespace;
mod preview;
mod releases;
mod reloader;
mod rollout;
mod terminal;
mod timeline;
mod trace;
mod usage;
mod writes;
