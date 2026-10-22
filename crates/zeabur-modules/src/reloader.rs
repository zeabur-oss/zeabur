//! Stakater Reloader: whether it runs, so a plan that changes a ConfigMap or Secret can say who
//! restarts the workloads that read it [reloader].
//!
//! Reloader has no API of its own: it is found by its Deployment, by the label Zeabur Catalog
//! detects it by, `app.kubernetes.io/name: reloader`. It runs when one of those Deployments has
//! an available replica. Which workloads it restarts is theirs to say, by annotation
//! (`zeabur_model::ConfigReload`); the catalog's values leave `autoReloadAll` off, so a workload
//! without one is left alone.
//!
//! Its Deployment usually lives in a namespace of its own, so a person who may only read their
//! projects cannot tell whether it runs; the plan then says Reloader restarts the readers
//! without confirming it.
//!
//! [reloader]: https://github.com/stakater/Reloader

use zeabur_kube::DynamicObject;
use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

use crate::{Module, Read};

/// The module.
pub static MODULE: Module = Module {
    id: "reloader",
    capability: "config-reload",
    reads: &[DEPLOYMENTS],
    http: &[],
};

/// `apps/v1` `Deployment`, cluster-wide, metadata first.
pub const DEPLOYMENTS: Read = Read {
    group: "apps",
    version: "v1",
    resource: "deployments",
    kind: "Deployment",
};

/// The label its chart puts on its Deployment.
pub const LABEL: (&str, &str) = ("app.kubernetes.io/name", "reloader");

/// Whether a Deployment's metadata is Reloader's.
#[must_use]
pub fn is_reloader(meta: &ObjectMeta) -> bool {
    meta.labels
        .as_ref()
        .and_then(|labels| labels.get(LABEL.0))
        .is_some_and(|value| value == LABEL.1)
}

/// Whether Reloader runs, from its Deployments: one with an available replica.
#[must_use]
pub fn running(deployments: &[DynamicObject]) -> bool {
    deployments.iter().any(|deployment| {
        deployment
            .data
            .pointer("/status/availableReplicas")
            .and_then(serde_json::Value::as_i64)
            .is_some_and(|n| n > 0)
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn deployment(status: &serde_json::Value) -> DynamicObject {
        serde_json::from_value(json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": "reloader-reloader", "namespace": "reloader",
                         "labels": {"app.kubernetes.io/name": "reloader"}},
            "status": status,
        }))
        .unwrap()
    }

    #[test]
    fn it_runs_with_an_available_replica() {
        assert!(running(&[deployment(&json!({"availableReplicas": 1}))]));
        assert!(!running(&[deployment(&json!({"replicas": 0}))]));
        assert!(!running(&[]));
    }

    #[test]
    fn it_is_found_by_its_chart_label() {
        assert!(is_reloader(&deployment(&json!({})).metadata));
        let other = ObjectMeta {
            labels: Some([(LABEL.0.to_owned(), "web".to_owned())].into()),
            ..ObjectMeta::default()
        };
        assert!(!is_reloader(&other));
    }
}
