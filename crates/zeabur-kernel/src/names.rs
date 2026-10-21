//! Names Zeabur and Kubernetes must agree on.

/// The domain every Zeabur label and annotation key lives under: Zeabur's own, `zeabur.com`.
pub const DOMAIN: &str = "zeabur.com";

/// The field manager (`fieldManager`) every write Zeabur makes names, so `managedFields`
/// records Zeabur as the one that changed those fields.
pub const FIELD_MANAGER: &str = "zeabur";

/// Label on a namespace that makes it an Zeabur project. Adopting a namespace adds only this
/// label; removing it (or Zeabur) leaves the namespace as it was.
pub const LABEL_PROJECT: &str = "zeabur.com/project";

/// Label on a workload that names its group within a project.
pub const LABEL_GROUP: &str = "zeabur.com/group";

/// Annotation on a project's namespace: the name to show instead of the namespace's.
///
/// OpenShift keeps the same detail as `openshift.io/display-name`. Annotations, not labels: they
/// hold any text, and nothing selects by them.
pub const ANNOTATION_DISPLAY_NAME: &str = "zeabur.com/display-name";

/// Annotation on a project's namespace: a sentence or two on what the project is (OpenShift:
/// `openshift.io/description`).
pub const ANNOTATION_DESCRIPTION: &str = "zeabur.com/description";

/// Annotation on a project's namespace: its icon, by name, one of Zeabur's built-in set.
pub const ANNOTATION_ICON: &str = "zeabur.com/icon";

/// Whether `namespace` is one Kubernetes keeps for itself, which is never adopted.
///
/// The `kube-` prefix "is reserved for Kubernetes system namespaces" (`kube-system`,
/// `kube-public`, `kube-node-lease`) [k8s-namespaces]: their objects are the cluster's own
/// components.
///
/// [k8s-namespaces]: https://kubernetes.io/docs/concepts/overview/working-with-objects/namespaces/
#[must_use]
pub fn is_system_namespace(namespace: &str) -> bool {
    namespace.starts_with("kube-")
}

/// A Gateway API `Gateway`, by namespace and name: one a new `HTTPRoute` can attach to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GatewayRef {
    /// Its namespace.
    pub namespace: String,
    /// Its name.
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::is_system_namespace;

    #[test]
    fn the_kube_prefix_is_kubernetes_own() {
        for name in ["kube-system", "kube-public", "kube-node-lease"] {
            assert!(is_system_namespace(name), "{name}");
        }
        for name in ["default", "shop", "kubeflow", "my-kube-app"] {
            assert!(!is_system_namespace(name), "{name}");
        }
    }
}
