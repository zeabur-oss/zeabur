//! The API against an in-memory cluster, in one test binary
//! (<https://doc.rust-lang.org/cargo/reference/cargo-targets.html#integration-tests>); the
//! cluster tests on k3s are their own binary, `kubernetes`.

mod api;
#[cfg(test)]
#[path = "../common/mod.rs"]
mod common;
mod evidence;
mod exec;
mod incidents;
mod namespace;
mod preview;
mod releases;
mod rollout;
mod timeline;
mod trace;
mod usage;
mod writes;
