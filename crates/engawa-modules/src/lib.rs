#![doc = include_str!("../README.md")]

use engawa_kube::{DynamicObject, ResourceRef};
use serde::Serialize;

pub mod backups;
pub mod cloudnative_pg;
pub mod k8up;
pub mod metrics_server;
pub mod network_policy;
pub mod prometheus_api;
pub mod system_upgrade_controller;
pub mod velero;

/// One gap a module finds, with the object that closes it when engawa cannot write the fix
/// itself.
///
/// Modules cannot implement `engawa-gaps`' rule trait (the core never depends on a module, and a
/// module never on a crate above it), so a module's rule is a pure function returning these, and
/// the composer wraps it as a rule next to the built-in ones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// The gap.
    pub gap: engawa_model::Gap,
    /// The object to write by hand, shown as YAML to copy; absent when the fix is a plan or an
    /// install.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<serde_json::Value>,
}

/// One integration: a provider of a capability, and what it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Module {
    /// Its id (`metrics-server`), the provider's own name.
    pub id: &'static str,
    /// The capability it provides, as the recommendations catalogue names it (`metrics`).
    pub capability: &'static str,
    /// The Kubernetes resources it reads.
    pub reads: &'static [Read],
    /// The HTTP APIs it reads from a Service, through kube-apiserver's service proxy. Which
    /// Service is a setting, not the module's.
    pub http: &'static [HttpApi],
}

impl Module {
    /// The group versions whose presence in discovery means the component is installed: those
    /// of the resources it reads.
    #[must_use]
    pub fn group_versions(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .reads
            .iter()
            .map(|r| r.resource().api_version())
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

/// A resource a module reads, and where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Read {
    /// The API group.
    pub group: &'static str,
    /// The version.
    pub version: &'static str,
    /// The plural resource name.
    pub resource: &'static str,
    /// The kind.
    pub kind: &'static str,
}

impl Read {
    /// The resource to read.
    #[must_use]
    pub fn resource(&self) -> ResourceRef {
        ResourceRef::new(self.group, self.version, self.resource, self.kind)
    }
}

/// An HTTP API a module reads from a Service (see [`Module::http`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpApi {
    /// The API and its version (`prometheus-http/v1`).
    pub name: &'static str,
    /// A path that answers `200` when the API is there, and asks nothing of it.
    pub probe: &'static str,
}

/// What reading a module's resources gave: the objects, nothing because the component is not
/// installed (discovery does not serve its API), or a refusal (the person may not read them).
#[derive(Debug, Clone, PartialEq)]
pub enum Reading<R> {
    /// The objects read.
    Objects(Vec<DynamicObject>),
    /// The component's API is not served.
    NotInstalled,
    /// The read was refused; `R` is the caller's record of the refusal.
    Refused(R),
}

/// Every module, in a fixed order; nothing registers itself (see the crate documentation).
#[must_use]
pub fn modules() -> Vec<&'static Module> {
    vec![
        &metrics_server::MODULE,
        &prometheus_api::MODULE,
        &system_upgrade_controller::MODULE,
        &network_policy::MODULE,
        &k8up::MODULE,
        &velero::MODULE,
        &cloudnative_pg::MODULE,
    ]
}

/// A string field of an object, by JSON pointer; `None` when missing or empty.
pub(crate) fn text(object: &DynamicObject, pointer: &str) -> Option<String> {
    object
        .data
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// A map of strings at `pointer`; empty when missing.
pub(crate) fn strings(object: &DynamicObject, pointer: &str) -> Vec<String> {
    object
        .data
        .pointer(pointer)
        .and_then(serde_json::Value::as_array)
        .map_or_else(Vec::new, |items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modules_are_listed_once_each_and_read_something() {
        let list = modules();
        let mut ids: Vec<&str> = list.iter().map(|m| m.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), list.len());
        for module in list {
            assert!(
                !module.reads.is_empty() || !module.http.is_empty(),
                "{} reads nothing",
                module.id
            );
        }
    }
}
