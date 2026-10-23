//! Names Zeabur and Kubernetes must agree on.

use std::borrow::Cow;

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

/// Label on a scheduled task's CronJob that names the service it was made from.
pub const LABEL_TASK_OF: &str = "zeabur.com/task-of";

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

/// The RBAC verb on a namespace that lets a patch change more than the project's details.
///
/// The chart's admission policy asks for it before a patch changes the labels, which adopting
/// writes, or annotations other than the details. No built-in role names it; `*`
/// (`cluster-admin`) covers it.
pub const MANAGE_VERB: &str = "zeabur-manage";

/// The namespace of node shells: privileged pods in the host's namespaces, which only cluster
/// administrators may run or enter. It is never a project, so no member role is bound in it.
pub const NODE_SHELL_NAMESPACE: &str = "zeabur-node-shell";

/// The name of every debug container Zeabur adds starts with this.
pub const DEBUG_PREFIX: &str = "zeabur-debug-";

/// The name of every tools pod starts with this.
pub const TOOLS_PREFIX: &str = "zeabur-tools-";

/// The label every tools pod carries: `"true"` on a project's, `"cluster"` on one for the whole
/// cluster.
pub const TOOLS_LABEL: &str = "zeabur.com/tools";

/// The label naming whose tools pod it is: a key derived from their user name.
pub const TOOLS_PERSON_LABEL: &str = "zeabur.com/tools-person";

/// The annotation on a tools pod with the person's user name, as the cluster gave it.
pub const TOOLS_PERSON_ANNOTATION: &str = "zeabur.com/tools-person";

/// The annotation on a tools pod with the project it was opened for.
pub const TOOLS_PROJECT_ANNOTATION: &str = "zeabur.com/tools-project";

/// The label and annotation keys, name prefixes and the field manager the projection and plans
/// read and write.
///
/// [`Conventions::ZEABUR`], the default, holds Zeabur's own names above. Another tool that uses
/// these crates passes its own, so its projects, groups and writes keep its names.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Conventions {
    /// The label on a namespace that makes it a project ([`LABEL_PROJECT`]).
    pub project_label: Cow<'static, str>,
    /// The label on a workload that names its group ([`LABEL_GROUP`]).
    pub group_label: Cow<'static, str>,
    /// The annotation holding a project's display name ([`ANNOTATION_DISPLAY_NAME`]).
    pub display_name_annotation: Cow<'static, str>,
    /// The annotation holding a project's description ([`ANNOTATION_DESCRIPTION`]).
    pub description_annotation: Cow<'static, str>,
    /// The annotation holding a project's icon ([`ANNOTATION_ICON`]).
    pub icon_annotation: Cow<'static, str>,
    /// The field manager writes are made as ([`FIELD_MANAGER`]), also the
    /// `app.kubernetes.io/managed-by` of the pods a terminal creates.
    pub field_manager: Cow<'static, str>,
    /// The prefix of a debug container's name ([`DEBUG_PREFIX`]).
    pub debug_prefix: Cow<'static, str>,
    /// The prefix of a tools pod's name ([`TOOLS_PREFIX`]); without its last `-`, also the
    /// pod's `app.kubernetes.io/name`.
    pub tools_prefix: Cow<'static, str>,
    /// The label every tools pod carries ([`TOOLS_LABEL`]).
    pub tools_label: Cow<'static, str>,
    /// The label naming whose tools pod it is ([`TOOLS_PERSON_LABEL`]).
    pub tools_person_label: Cow<'static, str>,
    /// The annotation with a tools pod's person ([`TOOLS_PERSON_ANNOTATION`]).
    pub tools_person_annotation: Cow<'static, str>,
    /// The annotation with a tools pod's project ([`TOOLS_PROJECT_ANNOTATION`]).
    pub tools_project_annotation: Cow<'static, str>,
}

impl Conventions {
    /// Zeabur's names, as zeabur.com uses them.
    pub const ZEABUR: Self = Self {
        project_label: Cow::Borrowed(LABEL_PROJECT),
        group_label: Cow::Borrowed(LABEL_GROUP),
        display_name_annotation: Cow::Borrowed(ANNOTATION_DISPLAY_NAME),
        description_annotation: Cow::Borrowed(ANNOTATION_DESCRIPTION),
        icon_annotation: Cow::Borrowed(ANNOTATION_ICON),
        field_manager: Cow::Borrowed(FIELD_MANAGER),
        debug_prefix: Cow::Borrowed(DEBUG_PREFIX),
        tools_prefix: Cow::Borrowed(TOOLS_PREFIX),
        tools_label: Cow::Borrowed(TOOLS_LABEL),
        tools_person_label: Cow::Borrowed(TOOLS_PERSON_LABEL),
        tools_person_annotation: Cow::Borrowed(TOOLS_PERSON_ANNOTATION),
        tools_project_annotation: Cow::Borrowed(TOOLS_PROJECT_ANNOTATION),
    };
}

impl Default for Conventions {
    fn default() -> Self {
        Self::ZEABUR
    }
}

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
