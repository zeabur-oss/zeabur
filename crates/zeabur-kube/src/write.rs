//! Writes as the person: one request per object. Patches and deletions are built with kube's
//! `Request`; the query, the creation and the send are in [`crate::request`]. Each answer's
//! `Warning` headers come back in [`Written::warnings`].
//!
//! - **A change** is a `PATCH` with `application/json-patch+json` [rfc6902] (kube's `Patch::Json`);
//!   its `test` operations make it all-or-nothing against what the plan read.
//! - **A debug container** is the same `PATCH` of a Pod's `ephemeralcontainers` subresource
//!   [k8s-ephemeral], the only way to add one.
//! - **A new object** is a `POST` to its collection.
//! - **An applied object** is a server-side apply `PATCH` [k8s-ssa] (kube's `Patch::Apply`), never
//!   forced.
//! - **A deletion** is a `DELETE` with `DeleteOptions` in its body [k8s-delete-options]:
//!   `propagationPolicy: Background`, as `kubectl delete` does by default, so kube-apiserver
//!   removes the object at once and the garbage collector removes what it owns (a Deployment's
//!   ReplicaSets and their Pods) afterwards [k8s-gc], or `Foreground` when asked, so the object
//!   stays until what it owns is gone; and `preconditions` on its `uid` and `resourceVersion`, so
//!   an object changed or replaced since it was read is refused with `409 Conflict` instead of
//!   deleted.
//! - Creations and patches name `fieldManager=zeabur`, so `managedFields` records Zeabur as the one
//!   who changed those fields [k8s-field-manager], and ask for `fieldValidation=Strict`: a field
//!   the object's schema does not know fails the request instead of being dropped silently
//!   [k8s-validation]. A dry run adds `dryRun=All`, which runs validation and admission without
//!   persisting [k8s-dry-run].
//!
//! [rfc6902]: https://www.rfc-editor.org/rfc/rfc6902
//! [k8s-ephemeral]: https://kubernetes.io/docs/reference/kubernetes-api/workload-resources/pod-v1/#update-ephemeralcontainers-of-the-specified-pod
//! [k8s-field-manager]: https://kubernetes.io/docs/reference/using-api/server-side-apply/#field-management
//! [k8s-validation]: https://kubernetes.io/docs/reference/using-api/api-concepts/#field-validation
//! [k8s-dry-run]: https://kubernetes.io/docs/reference/using-api/api-concepts/#dry-run
//! [k8s-delete-options]: https://kubernetes.io/docs/reference/kubernetes-api/common-definitions/delete-options/
//! [k8s-ssa]: https://kubernetes.io/docs/reference/using-api/server-side-apply/
//! [k8s-gc]: https://kubernetes.io/docs/concepts/architecture/garbage-collection/#background-deletion

use kube::api::{DynamicObject, Patch};
use kube::core::Request;

use crate::port::{ClusterError, ObjectTarget, WriteBody, WriteRequest, Written};
use crate::{names, request};

/// The URL path of `target`'s collection: `/api/v1/…` for the core group,
/// `/apis/{group}/{version}/…` otherwise; without `namespaces/{namespace}/` for a cluster-scoped
/// object (an empty namespace).
pub(crate) fn collection_path(target: &ObjectTarget) -> String {
    let base = if target.api_version.contains('/') {
        format!("/apis/{}", target.api_version)
    } else {
        format!("/api/{}", target.api_version)
    };
    if target.namespace.is_empty() {
        return format!("{base}/{}", encode(&target.plural));
    }
    format!(
        "{base}/namespaces/{}/{}",
        encode(&target.namespace),
        encode(&target.plural)
    )
}

/// Refuses a target whose path would leave its segments, as reads do ([`crate::names`]):
/// percent-encoding keeps a name of `..` as it is. Checks the `apiVersion`'s group and version,
/// the plural, the namespace unless the object is cluster-scoped, and the name when the request
/// addresses the object itself (`with_name`: a patch or a deletion; a creation names it in its
/// body, which kube-apiserver validates).
pub(crate) fn checked(target: &ObjectTarget, with_name: bool) -> Result<(), ClusterError> {
    let mut parts = target.api_version.splitn(3, '/');
    let (first, second, rest) = (parts.next(), parts.next(), parts.next());
    if rest.is_some() {
        return Err(ClusterError::InvalidName(format!(
            "{:?} is not an apiVersion",
            target.api_version
        )));
    }
    for part in [first, second].into_iter().flatten() {
        names::name(part)?;
    }
    names::name(&target.plural)?;
    if !target.namespace.is_empty() {
        names::namespace(&target.namespace)?;
    }
    if with_name {
        names::name(&target.name)?;
    }
    Ok(())
}

/// Percent-encodes everything but the unreserved characters of RFC 3986 section 2.3.
fn encode(segment: &str) -> String {
    segment
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                char::from(byte).to_string()
            } else {
                format!(
                    "%{}",
                    zeabur_core::crypto::hex(&[byte]).to_ascii_uppercase()
                )
            }
        })
        .collect()
}

/// Sends `request` as the person `client` is bound to.
pub(crate) async fn write(
    client: &kube::Client,
    request: &WriteRequest,
) -> Result<Written, ClusterError> {
    checked(
        &request.target,
        !matches!(request.body, WriteBody::Create(_)),
    )?;
    let path = Request::new(collection_path(&request.target));
    let name = request.target.name.as_str();
    let patch_params = request::patch_params(request.dry_run);
    let built = match &request.body {
        WriteBody::Create(object) => {
            request::create(&collection_path(&request.target), object, request.dry_run)
        }
        WriteBody::JsonPatch(operations) => path
            .patch(name, &patch_params, &json_patch(operations)?)
            .map_err(|e| built_wrong(&e)),
        WriteBody::MergePatch(body) => path
            .patch(name, &patch_params, &Patch::Merge(body))
            .map_err(|e| built_wrong(&e)),
        WriteBody::Apply(object) => path
            .patch(name, &patch_params, &Patch::Apply(object))
            .map_err(|e| built_wrong(&e)),
        WriteBody::EphemeralContainers(operations) => path
            .patch_subresource(
                "ephemeralcontainers",
                name,
                &patch_params,
                &json_patch(operations)?,
            )
            .map_err(|e| built_wrong(&e)),
        WriteBody::Delete(preconditions) => path
            .delete(
                name,
                &request::delete_params(preconditions, request.dry_run),
            )
            .map_err(|e| built_wrong(&e)),
    }?;
    let (body, warnings) = request::send(client, built).await?;
    let answer: DynamicObject = match serde_json::from_slice(&body) {
        Ok(answer) => answer,
        // A deletion that kube-apiserver finished at once answers with a `Status`.
        Err(_status) if matches!(request.body, WriteBody::Delete(_)) => {
            return Ok(Written {
                warnings,
                ..Written::default()
            });
        }
        Err(error) => return Err(ClusterError::Decode(error.to_string())),
    };
    if matches!(request.body, WriteBody::Delete(_)) {
        return Ok(Written {
            resource_version: answer.metadata.resource_version,
            warnings,
            ..Written::default()
        });
    }
    let object = serde_json::to_value(&answer).ok();
    Ok(Written {
        resource_version: answer.metadata.resource_version,
        generation: answer.metadata.generation,
        uid: answer.metadata.uid,
        object,
        warnings,
    })
}

fn built_wrong(error: &kube::core::request::Error) -> ClusterError {
    ClusterError::Transport(error.to_string())
}

/// The operations of a plan as kube's JSON Patch.
fn json_patch(operations: &serde_json::Value) -> Result<Patch<()>, ClusterError> {
    serde_json::from_value(operations.clone())
        .map(Patch::Json)
        .map_err(|e| ClusterError::Decode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(api_version: &str, plural: &str) -> ObjectTarget {
        ObjectTarget {
            api_version: api_version.to_owned(),
            kind: String::new(),
            plural: plural.to_owned(),
            namespace: "shop".to_owned(),
            name: "web".to_owned(),
        }
    }

    #[test]
    fn a_write_cannot_leave_its_path_segments() {
        assert_eq!(checked(&target("apps/v1", "deployments"), true), Ok(()));
        let namespace = ObjectTarget {
            namespace: String::new(),
            ..target("v1", "namespaces")
        };
        assert_eq!(checked(&namespace, true), Ok(()));
        let named = |name: &str| ObjectTarget {
            name: name.to_owned(),
            ..target("v1", "configmaps")
        };
        for name in [
            "..",
            ".",
            "",
            "../secrets/db",
            "web/status",
            "web?dryRun=All",
        ] {
            let refused = checked(&named(name), true).unwrap_err();
            assert_eq!(refused.code(), Some(404), "{name:?}");
            // A creation is posted to the collection; its name is in the body.
            assert_eq!(checked(&named(name), false), Ok(()), "{name:?}");
        }
        let in_namespace = |namespace: &str| ObjectTarget {
            namespace: namespace.to_owned(),
            ..target("v1", "configmaps")
        };
        for namespace in ["..", "shop/../kube-system", "Shop"] {
            assert!(
                checked(&in_namespace(namespace), false).is_err(),
                "{namespace}"
            );
        }
        for (api_version, plural) in [
            ("../v1", "configmaps"),
            ("apps/v1/x", "deployments"),
            ("v1", ".."),
        ] {
            assert!(
                checked(&target(api_version, plural), true).is_err(),
                "{api_version} {plural}"
            );
        }
    }

    #[test]
    fn a_creation_is_posted_to_its_collection() {
        assert_eq!(
            collection_path(&target("v1", "configmaps")),
            "/api/v1/namespaces/shop/configmaps"
        );
        assert_eq!(
            collection_path(&target("networking.k8s.io/v1", "ingresses")),
            "/apis/networking.k8s.io/v1/namespaces/shop/ingresses"
        );
        assert_eq!(encode("a b/c"), "a%20b%2Fc");
        let namespace = ObjectTarget {
            namespace: String::new(),
            ..target("v1", "namespaces")
        };
        assert_eq!(collection_path(&namespace), "/api/v1/namespaces");
    }
}
