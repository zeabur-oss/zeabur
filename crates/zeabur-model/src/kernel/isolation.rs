//! The `NetworkPolicy` every new project namespace gets: CIS Kubernetes Benchmark 5.3.2
//! [cis-rke2] asks for a policy in every namespace.
//!
//! The policy isolates every pod of the namespace for ingress. Pods of the same namespace, of
//! `kube-system` and of the namespaces labelled [`PLATFORM_LABEL`] may connect. Egress stays open.
//! A namespace that another project must reach gets one more policy per allowed namespace,
//! selected by `kubernetes.io/metadata.name` [k8s-ns-label]: rules of all policies add up
//! [k8s-netpol].
//!
//! [cis-rke2]: https://docs.rke2.io/security/cis_self_assessment112
//! [k8s-netpol]: https://kubernetes.io/docs/concepts/services-networking/network-policies/
//! [k8s-ns-label]: https://kubernetes.io/docs/reference/labels-annotations-taints/#kubernetes-io-metadata-name

use serde_json::{Value, json};

/// The name of the project's own policy.
pub const PROJECT_POLICY: &str = "zeabur-project";

/// The label on the namespaces of the platform: the ingress controller, metrics collectors and
/// database operators. zeabur-installer sets it on each namespace it makes (its
/// `BASELINE_LABEL`).
pub const PLATFORM_LABEL: &str = "zeabur.com/platform";

/// The namespace label the API server sets to the namespace's name.
pub const NAMESPACE_NAME_LABEL: &str = "kubernetes.io/metadata.name";

/// The project's own policy in `namespace`.
#[must_use]
pub fn project_policy(namespace: &str) -> Value {
    json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "NetworkPolicy",
        "metadata": {
            "name": PROJECT_POLICY,
            "namespace": namespace,
            "labels": {"app.kubernetes.io/managed-by": "zeabur"},
        },
        "spec": {
            "podSelector": {},
            "policyTypes": ["Ingress"],
            "ingress": [{"from": [
                {"podSelector": {}},
                {"namespaceSelector": {"matchLabels": {PLATFORM_LABEL: "true"}}},
                {"namespaceSelector": {"matchLabels": {NAMESPACE_NAME_LABEL: "kube-system"}}},
            ]}],
        },
    })
}
