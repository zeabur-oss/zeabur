//! A project being deleted: its Namespace is `Terminating`, and the namespace controller says in
//! its conditions what is left [k8s-ns-conditions]. When objects with finalizers hold it, they are
//! listed as the person, so the interface can name them and show how to look at them. Zeabur
//! never removes a finalizer.
//!
//! The controller's messages are `Some resources are remaining: <group-resource> has <n>
//! resource instances, …` (`NamespaceContentRemaining`) and `Some content in the namespace has
//! finalizers remaining: <finalizer> in <n> resource instances, …`
//! (`NamespaceFinalizersRemaining`); a core resource is written with a trailing dot
//! (`pods.`) [k8s-ns-messages].
//!
//! [k8s-ns-conditions]: https://kubernetes.io/docs/reference/kubernetes-api/cluster-resources/namespace-v1/#NamespaceStatus
//! [k8s-ns-messages]: https://github.com/kubernetes/kubernetes/blob/v1.34.1/pkg/controller/namespace/deletion/status_condition_utils.go

use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_kube::{Cluster, DynamicObject, ResourceRef};

/// Objects held back from more than this many kinds are not listed one by one.
const KINDS_LISTED: usize = 10;

/// A project whose Namespace is being deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDeleting {
    /// When the deletion started, RFC 3339.
    pub since: String,
    /// What is left, by resource, as the namespace controller counts it.
    pub remaining: Vec<RemainingContent>,
    /// The finalizers that hold objects back, as the namespace controller counts them.
    pub finalizers: Vec<RemainingFinalizer>,
    /// The objects with finalizers the person could read.
    pub held: Vec<HeldObject>,
}

/// Objects of one resource that are left.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemainingContent {
    /// The API group; empty for the core group.
    pub group: String,
    /// The resource (plural).
    pub resource: String,
    /// How many.
    pub count: u64,
}

/// One finalizer that holds objects back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemainingFinalizer {
    /// The finalizer (`cnpg.io/cleanup`).
    pub finalizer: String,
    /// On how many objects.
    pub count: u64,
}

/// An object whose finalizers keep the Namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HeldObject {
    /// The API group; empty for the core group.
    pub group: String,
    /// The resource (plural).
    pub resource: String,
    /// The kind.
    pub kind: String,
    /// The name.
    pub name: String,
    /// Its finalizers.
    pub finalizers: Vec<String>,
}

/// What the namespace controller says is left of `namespace`; `None` when it is not being
/// deleted.
pub(super) async fn read(
    cluster: &dyn Cluster,
    project: &str,
    namespace: &DynamicObject,
) -> Option<ProjectDeleting> {
    let since = namespace
        .metadata
        .deletion_timestamp
        .as_ref()?
        .0
        .to_string();
    let conditions = namespace
        .data
        .pointer("/status/conditions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let message = |kind: &str| {
        conditions
            .iter()
            .find(|c| {
                c.get("type").and_then(Value::as_str) == Some(kind)
                    && c.get("status").and_then(Value::as_str) == Some("True")
            })
            .and_then(|c| c.get("message").and_then(Value::as_str))
            .unwrap_or_default()
            .to_owned()
    };
    let remaining = remaining(&message("NamespaceContentRemaining"));
    let finalizers = finalizers(&message("NamespaceFinalizersRemaining"));
    let held = if finalizers.is_empty() {
        Vec::new()
    } else {
        held(cluster, project, &remaining).await
    };
    Some(ProjectDeleting {
        since,
        remaining,
        finalizers,
        held,
    })
}

/// The items of a condition message: what follows the colon, split on `", "`.
fn items(message: &str) -> impl Iterator<Item = &str> {
    message
        .split_once(": ")
        .map_or("", |(_, rest)| rest)
        .split(", ")
        .filter(|item| !item.is_empty())
}

/// `<resource>.<group> has <n> resource instances`, of `NamespaceContentRemaining`.
fn remaining(message: &str) -> Vec<RemainingContent> {
    items(message)
        .filter_map(|item| {
            let (what, rest) = item.split_once(" has ")?;
            let count = rest.split_whitespace().next()?.parse().ok()?;
            let (resource, group) = what.split_once('.').unwrap_or((what, ""));
            Some(RemainingContent {
                group: group.to_owned(),
                resource: resource.to_owned(),
                count,
            })
        })
        .collect()
}

/// `<finalizer> in <n> resource instances`, of `NamespaceFinalizersRemaining`.
fn finalizers(message: &str) -> Vec<RemainingFinalizer> {
    items(message)
        .filter_map(|item| {
            let (finalizer, rest) = item.rsplit_once(" in ")?;
            let count = rest.split_whitespace().next()?.parse().ok()?;
            Some(RemainingFinalizer {
                finalizer: finalizer.to_owned(),
                count,
            })
        })
        .collect()
}

/// The objects with finalizers among `remaining`, read as the person; a resource that is not
/// served or not readable is left out.
async fn held(
    cluster: &dyn Cluster,
    project: &str,
    remaining: &[RemainingContent],
) -> Vec<HeldObject> {
    let Ok(served) = cluster.served_resources().await else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for left in remaining.iter().take(KINDS_LISTED) {
        let Some(found) = served
            .iter()
            .find(|s| s.group == left.group && s.resource == left.resource && s.namespaced)
        else {
            continue;
        };
        let resource = ResourceRef::new(&found.group, &found.version, &found.resource, &found.kind);
        let Ok(Some(objects)) = cluster.list_objects(&resource, Some(project)).await else {
            continue;
        };
        out.extend(objects.into_iter().filter_map(|object| {
            let finalizers = object.metadata.finalizers.clone().unwrap_or_default();
            (!finalizers.is_empty()).then(|| HeldObject {
                group: found.group.clone(),
                resource: found.resource.clone(),
                kind: found.kind.clone(),
                name: object.metadata.name.clone().unwrap_or_default(),
                finalizers,
            })
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_controllers_messages_are_read_as_it_writes_them() {
        let content = remaining(
            "Some resources are remaining: clusters.postgresql.cnpg.io has 1 resource instances, \
             persistentvolumeclaims. has 2 resource instances",
        );
        assert_eq!(
            content,
            [
                RemainingContent {
                    group: "postgresql.cnpg.io".to_owned(),
                    resource: "clusters".to_owned(),
                    count: 1
                },
                RemainingContent {
                    group: String::new(),
                    resource: "persistentvolumeclaims".to_owned(),
                    count: 2
                },
            ]
        );
        assert_eq!(
            finalizers(
                "Some content in the namespace has finalizers remaining: cnpg.io/cleanup in 1 \
                 resource instances, kubernetes.io/pvc-protection in 2 resource instances"
            ),
            [
                RemainingFinalizer {
                    finalizer: "cnpg.io/cleanup".to_owned(),
                    count: 1
                },
                RemainingFinalizer {
                    finalizer: "kubernetes.io/pvc-protection".to_owned(),
                    count: 2
                },
            ]
        );
        assert_eq!(remaining(""), []);
    }
}
