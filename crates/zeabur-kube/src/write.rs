//! Writes as the person: one request per object. Patches go through kube's `Api::patch` with
//! `PatchParams`; a creation is built by hand, because kube's `PostParams` cannot ask for strict
//! field validation.
//!
//! - **A change** is a `PATCH` with `application/json-patch+json` [rfc6902] (kube's `Patch::Json`);
//!   its `test` operations make it all-or-nothing against what the plan read.
//! - **A debug container** is the same `PATCH` of a Pod's `ephemeralcontainers` subresource
//!   [k8s-ephemeral], the only way to add one.
//! - **A new object** is a `POST` to its collection.
//! - **An applied object** is a server-side apply `PATCH` [k8s-ssa] (kube's `Patch::Apply`), never
//!   forced.
//! - **A deletion** is kube's `Api::delete` with `DeleteOptions` in its body [k8s-delete-options]:
//!   `propagationPolicy: Background`, as `kubectl delete` does by default, so kube-apiserver
//!   removes the object at once and the garbage collector removes what it owns (a Deployment's
//!   ReplicaSets and their Pods) afterwards [k8s-gc]; and `preconditions` on its `uid` and
//!   `resourceVersion`, so an object changed or replaced since it was read is refused with `409
//!   Conflict` instead of deleted.
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

use kube::api::{Api, DeleteParams, DynamicObject, Patch, PatchParams};
use kube::core::{ApiResource, GroupVersionKind};

use crate::names;
use crate::port::{
    ClusterError, FIELD_MANAGER, ObjectTarget, Preconditions, WriteBody, WriteRequest, Written,
};

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

/// The query string of a creation. A deletion writes no fields, so it names no field manager and
/// no field validation, and its dry run goes in its body ([`delete_params`]).
fn query(request: &WriteRequest) -> String {
    let mut parts = vec![
        format!("fieldManager={FIELD_MANAGER}"),
        "fieldValidation=Strict".to_owned(),
    ];
    if request.dry_run {
        parts.push("dryRun=All".to_owned());
    }
    parts.join("&")
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
    let api = api(client, &request.target);
    let name = request.target.name.as_str();
    let patch_params = patch_params(request);
    let answer = match &request.body {
        WriteBody::Create(object) => {
            let http_request = http::Request::post(format!(
                "{}?{}",
                collection_path(&request.target),
                query(request)
            ))
            .header(http::header::CONTENT_TYPE, "application/json")
            .header(http::header::ACCEPT, "application/json")
            .body(serde_json::to_vec(object).map_err(|e| ClusterError::Decode(e.to_string()))?)
            .map_err(|e| ClusterError::Transport(e.to_string()))?;
            client.request::<DynamicObject>(http_request).await?
        }
        WriteBody::JsonPatch(operations) => {
            api.patch(name, &patch_params, &json_patch(operations)?)
                .await?
        }
        WriteBody::MergePatch(body) => api.patch(name, &patch_params, &Patch::Merge(body)).await?,
        WriteBody::Apply(object) => {
            api.patch(name, &patch_params, &Patch::Apply(object))
                .await?
        }
        WriteBody::EphemeralContainers(operations) => {
            api.patch_subresource(
                "ephemeralcontainers",
                name,
                &patch_params,
                &json_patch(operations)?,
            )
            .await?
        }
        WriteBody::Delete(preconditions) => {
            let answer = api
                .delete(name, &delete_params(preconditions, request.dry_run))
                .await?;
            return Ok(Written {
                resource_version: answer.left().and_then(|o| o.metadata.resource_version),
                generation: None,
                uid: None,
            });
        }
    };
    Ok(Written {
        resource_version: answer.metadata.resource_version,
        generation: answer.metadata.generation,
        uid: answer.metadata.uid,
    })
}

/// The client of `target`'s kind, namespaced unless the object is cluster-scoped.
fn api(client: &kube::Client, target: &ObjectTarget) -> Api<DynamicObject> {
    let (group, version) = target
        .api_version
        .split_once('/')
        .unwrap_or(("", target.api_version.as_str()));
    let resource = ApiResource::from_gvk_with_plural(
        &GroupVersionKind::gvk(group, version, &target.kind),
        &target.plural,
    );
    if target.namespace.is_empty() {
        Api::all_with(client.clone(), &resource)
    } else {
        Api::namespaced_with(client.clone(), &target.namespace, &resource)
    }
}

/// Names the field manager and asks for strict field validation, so a field the object's schema
/// does not know fails the request instead of being dropped silently; a dry run adds `dryRun=All`.
fn patch_params(request: &WriteRequest) -> PatchParams {
    let params = PatchParams {
        dry_run: request.dry_run,
        field_manager: Some(FIELD_MANAGER.to_owned()),
        ..PatchParams::default()
    };
    params.validation_strict()
}

/// The operations of a plan as kube's JSON Patch.
fn json_patch(operations: &serde_json::Value) -> Result<Patch<()>, ClusterError> {
    serde_json::from_value(operations.clone())
        .map(Patch::Json)
        .map_err(|e| ClusterError::Decode(e.to_string()))
}

/// The options of a deletion [k8s-delete-options], which kube sends as its body, the dry run
/// included: kube-apiserver reads a deletion's options from its body when it has one and only
/// otherwise from the query string [delete-handler], so a `dryRun=All` query beside a body would
/// be ignored and the object deleted for real.
///
/// [k8s-delete-options]: https://kubernetes.io/docs/reference/kubernetes-api/common-definitions/delete-options/
/// [delete-handler]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/endpoints/handlers/delete.go
pub(crate) fn delete_params(preconditions: &Preconditions, dry_run: bool) -> DeleteParams {
    DeleteParams {
        dry_run,
        preconditions: Some(kube::api::Preconditions {
            uid: preconditions.uid.clone(),
            resource_version: preconditions.resource_version.clone(),
        }),
        grace_period_seconds: preconditions.grace_period_seconds,
        ..DeleteParams::background()
    }
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

    #[test]
    fn every_write_names_the_manager_and_asks_for_strict_validation() {
        let mut request = WriteRequest {
            target: target("apps/v1", "deployments"),
            body: WriteBody::Create(serde_json::json!({})),
            dry_run: true,
        };
        assert_eq!(
            query(&request),
            "fieldManager=zeabur&fieldValidation=Strict&dryRun=All"
        );
        request.body = WriteBody::JsonPatch(serde_json::json!([]));
        let params = patch_params(&request);
        assert_eq!(params.field_manager.as_deref(), Some("zeabur"));
        assert_eq!(
            params
                .field_validation
                .as_ref()
                .map(kube::api::ValidationDirective::as_str),
            Some("Strict")
        );
        assert!(params.dry_run);
        request.dry_run = false;
        assert!(!patch_params(&request).dry_run);
        assert_eq!(
            query(&request),
            "fieldManager=zeabur&fieldValidation=Strict"
        );
    }

    #[test]
    fn a_deletion_propagates_in_the_background_and_names_its_preconditions() {
        let preconditions = Preconditions {
            uid: Some("u-1".to_owned()),
            resource_version: Some("42".to_owned()),
            grace_period_seconds: None,
        };
        let body = |dry_run| serde_json::to_value(delete_params(&preconditions, dry_run)).unwrap();
        let options = body(false);
        assert_eq!(options["propagationPolicy"], "Background");
        assert_eq!(
            options["preconditions"],
            serde_json::json!({"uid": "u-1", "resourceVersion": "42"})
        );
        assert_eq!(options.get("dryRun"), None);
        assert_eq!(body(true)["dryRun"], serde_json::json!(["All"]));
    }
}
