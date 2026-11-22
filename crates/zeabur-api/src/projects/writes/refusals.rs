//! What a refused write tells the person: a plan that moved on, the request refused, or a
//! quota's or a policy's refusal (see [`super::admission`]).

use zeabur_core::problem::{Denied, Kind, Problem};
use zeabur_kube::{Cluster, ClusterError, WriteBody, WriteRequest};

/// The problem for a refused write of `target`, reading the object again when kube-apiserver
/// answers `422`.
///
/// A failed JSON Patch `test` is answered `422 Invalid` like any invalid object
/// [k8s-patch-handler], and its message is not an interface. So on a `422` to a patch, its `test`
/// operations are checked against the object read again: one that no longer holds is a `409`,
/// otherwise the `422` passes through. Only when the object cannot be read is the message consulted
/// ([`write_problem`]). A `403` or a webhook's `400` from admission becomes `quota-exceeded` or
/// `policy-denied` ([`crate::projects::writes::admission`]).
///
/// [k8s-patch-handler]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/endpoints/handlers/patch.go
pub(super) async fn refused(
    cluster: &dyn Cluster,
    project: &str,
    target: &zeabur_plan::Target,
    write: &WriteRequest,
    error: ClusterError,
) -> Problem {
    if matches!(write.body, WriteBody::EphemeralContainers(_)) && error.is_not_found() {
        return ephemeral_containers_off(cluster, project, target).await;
    }
    if let (
        ClusterError::Status(status),
        WriteBody::JsonPatch(operations) | WriteBody::EphemeralContainers(operations),
    ) = (&error, &write.body)
        && status.code == 422
    {
        let (group, version) = target
            .api_version
            .rsplit_once('/')
            .unwrap_or(("", target.api_version.as_str()));
        let resource = zeabur_kube::ResourceRef::new(group, version, &target.plural, &target.kind);
        let namespace = target.namespaced().then_some(project);
        if let Ok(Some(current)) = cluster.get_object(&resource, namespace, &target.name).await
            && let Ok(current) = serde_json::to_value(&current)
        {
            return if tests_hold(operations, &current) {
                error.into_problem(&format!("{} {}", target.kind, target.name))
            } else {
                moved_on(target)
            };
        }
    }
    let what = format!("{} {}", target.kind, target.name);
    crate::projects::writes::admission::refine(
        cluster,
        project,
        &what,
        write_problem(error, target, write),
    )
    .await
}

/// The problem for a `404` to a pod's `ephemeralcontainers`: the pod is gone, or kube-apiserver
/// does not serve the subresource (before Kubernetes 1.23 it is behind the `EphemeralContainers`
/// feature gate [k8s-ephemeral]).
///
/// [k8s-ephemeral]: https://kubernetes.io/docs/concepts/workloads/pods/ephemeral-containers/
async fn ephemeral_containers_off(
    cluster: &dyn Cluster,
    namespace: &str,
    target: &zeabur_plan::Target,
) -> Problem {
    let pods = zeabur_kube::ResourceRef::new("", "v1", "pods", "Pod");
    let found = zeabur_kube::ClusterExt::find_object(cluster, &pods, Some(namespace), &target.name);
    if found.await == Ok(None) {
        return moved_on(target);
    }
    let mut problem = Problem::new(
        Kind::Invalid,
        "This cluster does not accept debug containers: kube-apiserver does not serve the \
         ephemeral containers of a pod.",
    );
    problem.fix_for_occasion(
        "Ask a cluster administrator to turn on ephemeral containers (on by default since \
         Kubernetes 1.23), or use an image with a shell.",
        zeabur_core::message::Message::new("problem.invalid.fix.ephemeralContainers"),
    );
    problem
}

/// Whether every `test` operation of a JSON Patch holds on `object`: the value at its path equals
/// the one it names [rfc6902-test].
///
/// [rfc6902-test]: https://www.rfc-editor.org/rfc/rfc6902#section-4.6
pub(super) fn tests_hold(operations: &serde_json::Value, object: &serde_json::Value) -> bool {
    operations.as_array().is_none_or(|operations| {
        operations
            .iter()
            .filter(|op| op.get("op").and_then(serde_json::Value::as_str) == Some("test"))
            .all(|op| {
                let path = op
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                object.pointer(path) == op.get("value")
            })
    })
}

/// The `409` for an object that changed after its plan was made.
pub(super) fn moved_on(target: &zeabur_plan::Target) -> Problem {
    let mut problem = Problem::new(
        Kind::Conflict,
        format!(
            "{} {} changed after the plan was made.",
            target.kind, target.name
        ),
    );
    problem.fix_for_occasion(
        "Make the plan again: it will start from the object as it is now.",
        zeabur_core::message::Message::new("problem.conflict.fix.review"),
    );
    problem
}

/// The problem for a refused write of `target`.
///
/// A create whose name is taken is answered `409 AlreadyExists`: the object is named as existing,
/// whoever made it, since a plan made again would create it again. A plan's other preconditions
/// failing means the object moved on since the plan: `409 Conflict`, or a `422` that names no
/// invalid field, which is how a JSON Patch that does not apply is answered [k8s-patch]. Both are
/// `409` here, with the fix to plan again. A `422` that
/// names a field stays as kube-apiserver gave it, as do `429` (with its `Retry-After`) and other
/// refusals. A `403` also names the request that was refused ([`denied`]).
///
/// [k8s-patch]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/endpoints/handlers/patch.go
pub(super) fn write_problem(
    error: ClusterError,
    target: &zeabur_plan::Target,
    write: &WriteRequest,
) -> Problem {
    if let ClusterError::Status(status) = &error
        && status.code == 409
        && status.reason == "AlreadyExists"
    {
        return super::creations::already_exist(
            &format!("{} {}", target.kind, target.name),
            &write.target.namespace,
        );
    }
    let changed = match &error {
        ClusterError::Status(status) => {
            status.code == 409 || (status.code == 422 && !status.names_invalid_field)
        }
        ClusterError::Transport(_) | ClusterError::Decode(_) | ClusterError::InvalidName(_) => {
            false
        }
    };
    if changed {
        return moved_on(target);
    }
    let mut problem = error.into_problem(&format!("{} {}", target.kind, target.name));
    if problem.kind == Kind::Forbidden {
        problem.deny(denied(write));
    }
    problem
}

/// `write` as kube-apiserver's authorizers see it [k8s-authz]: a creation is a `create` of the
/// collection, which names no object; a JSON Patch or a merge patch is a `patch` of the
/// object, a deletion a
/// `delete` of it, and a debug container a `patch` of the pod's `ephemeralcontainers`
/// subresource. A cluster-scoped object (the project's Namespace) names no namespace.
///
/// [k8s-authz]: https://kubernetes.io/docs/reference/access-authn-authz/authorization/#determine-the-request-verb
pub(super) fn denied(write: &WriteRequest) -> Denied {
    let target = &write.target;
    let (verb, name, subresource) = match write.body {
        WriteBody::Create(_) => ("create", None, ""),
        WriteBody::JsonPatch(_) | WriteBody::MergePatch(_) | WriteBody::Apply(_) => {
            ("patch", Some(target.name.clone()), "")
        }
        WriteBody::EphemeralContainers(_) => {
            ("patch", Some(target.name.clone()), "/ephemeralcontainers")
        }
        WriteBody::Delete(_) => ("delete", Some(target.name.clone()), ""),
    };
    Denied {
        verb: verb.to_owned(),
        group: target
            .api_version
            .rsplit_once('/')
            .map_or("", |(group, _)| group)
            .to_owned(),
        resource: format!("{}{subresource}", target.plural),
        name,
        namespace: (!target.namespace.is_empty()).then(|| target.namespace.clone()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zeabur_core::problem::{Denied, Kind, KubernetesStatus};
    use zeabur_kube::{ClusterError, WriteBody, WriteRequest};

    use crate::projects::writes::write_request;

    fn target() -> zeabur_plan::Target {
        zeabur_plan::Target {
            api_version: "apps/v1".to_owned(),
            kind: "Deployment".to_owned(),
            plural: "deployments".to_owned(),
            name: "web".to_owned(),
        }
    }

    fn patch() -> WriteRequest {
        write_request(
            "shop",
            &target(),
            &zeabur_plan::Write::Patch {
                operations: vec![json!({"op": "test", "path": "/metadata/generation", "value": 1})],
            },
        )
    }

    fn write_problem(
        error: ClusterError,
        target: &zeabur_plan::Target,
    ) -> zeabur_core::problem::Problem {
        super::write_problem(error, target, &patch())
    }

    fn answer(code: u16, reason: &str, message: &str) -> ClusterError {
        ClusterError::Status(KubernetesStatus::new(code, reason, message))
    }

    fn invalid_field(message: &str) -> ClusterError {
        ClusterError::Status(KubernetesStatus {
            names_invalid_field: true,
            ..KubernetesStatus::new(422, "Invalid", message)
        })
    }

    #[test]
    fn a_failed_precondition_is_a_conflict_however_kube_apiserver_says_it() {
        for error in [
            answer(
                422,
                "Invalid",
                "the server rejected our request due to an error in our request",
            ),
            answer(409, "Conflict", "the object has been modified"),
        ] {
            let problem = write_problem(error, &target());
            assert_eq!(problem.kind, Kind::Conflict, "{problem:?}");
            assert!(problem.fix.contains("plan again"), "{problem:?}");
        }
    }

    /// A name taken by an object nobody planned around, such as one left by kubectl, is named as
    /// existing: planning again would not help.
    #[test]
    fn a_taken_name_says_the_object_exists() {
        let create = WriteRequest {
            body: WriteBody::Create(json!({})),
            ..patch()
        };
        let problem = super::write_problem(
            answer(409, "AlreadyExists", "deployments \"web\" already exists"),
            &target(),
            &create,
        );
        assert_eq!(problem.kind, Kind::Conflict);
        assert_eq!(problem.summary.key, "problem.summary.already-exists");
        assert_eq!(problem.summary.params["objects"], "Deployment web");
        assert_eq!(problem.summary.params["namespace"], "shop");
        assert!(!problem.detail.contains("changed"), "{problem:?}");
    }

    #[test]
    fn other_refusals_keep_kube_apiservers_answer() {
        let invalid = write_problem(invalid_field("spec.replicas: Invalid value"), &target());
        assert_ne!(invalid.kind, Kind::Conflict);
        let busy = write_problem(
            answer(429, "TooManyRequests", "too many requests"),
            &target(),
        );
        assert_eq!(busy.kind, Kind::TooManyRequests);
        assert_eq!(busy.denied, None);
    }

    /// The refused request comes from what Zeabur sent, whatever kube-apiserver's message says.
    #[test]
    fn a_403_names_the_request_that_was_refused() {
        let forbidden = || answer(403, "Forbidden", "no");
        let problem = write_problem(forbidden(), &target());
        assert_eq!(problem.kind, Kind::Forbidden);
        assert_eq!(
            problem.denied,
            Some(Denied {
                verb: "patch".to_owned(),
                group: "apps".to_owned(),
                resource: "deployments".to_owned(),
                name: Some("web".to_owned()),
                namespace: Some("shop".to_owned()),
            })
        );
        let create = WriteRequest {
            body: WriteBody::Create(json!({})),
            ..patch()
        };
        let denied = super::write_problem(forbidden(), &target(), &create)
            .denied
            .unwrap();
        assert_eq!((denied.verb.as_str(), denied.name), ("create", None));
        let core = WriteRequest {
            target: zeabur_kube::ObjectTarget {
                api_version: "v1".to_owned(),
                ..patch().target
            },
            ..patch()
        };
        let denied = super::write_problem(forbidden(), &target(), &core)
            .denied
            .unwrap();
        assert_eq!(denied.group, "");
        // The project's Namespace is cluster-scoped: its patch names no namespace.
        let namespace = write_request(
            "shop",
            &zeabur_plan::Target::namespace("shop"),
            &zeabur_plan::Write::Patch {
                operations: Vec::new(),
            },
        );
        assert_eq!(namespace.target.namespace, "");
        let denied = super::write_problem(forbidden(), &target(), &namespace)
            .denied
            .unwrap();
        assert_eq!(
            denied,
            Denied {
                verb: "patch".to_owned(),
                group: String::new(),
                resource: "namespaces".to_owned(),
                name: Some("shop".to_owned()),
                namespace: None,
            }
        );
    }
}
