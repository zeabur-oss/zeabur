//! What Zeabur's writes rely on kube-apiserver for, as one suite of behaviours run against both
//! the in-memory cluster (`tests/memory.rs`, always) and k3s (`tests/kubernetes.rs`, with a
//! cluster). The API tests trust the in-memory cluster to answer as kube-apiserver does; this
//! suite is what makes that trust checked rather than assumed [swe-doubles].
//!
//! Each behaviour takes a [`Cluster`] acting as a person who may get, create and patch
//! ConfigMaps and Deployments, and delete ConfigMaps, in `namespace`, which starts empty.
//!
//! [swe-doubles]: https://abseil.io/resources/swe-book/html/ch13.html

use serde_json::{Value, json};
use zeabur_core::json_patch::apply_patch;
use zeabur_core::problem::KubernetesStatus;
use zeabur_kube::{
    Cluster, ClusterError, ClusterExt as _, ObjectTarget, Preconditions, WriteBody, WriteRequest,
};

fn target(
    namespace: &str,
    api_version: &str,
    kind: &str,
    plural: &str,
    name: &str,
) -> ObjectTarget {
    ObjectTarget {
        api_version: api_version.to_owned(),
        kind: kind.to_owned(),
        plural: plural.to_owned(),
        namespace: namespace.to_owned(),
        name: name.to_owned(),
    }
}

fn config_map(namespace: &str, name: &str) -> ObjectTarget {
    target(namespace, "v1", "ConfigMap", "configmaps", name)
}

fn deployment(namespace: &str, name: &str) -> ObjectTarget {
    target(namespace, "apps/v1", "Deployment", "deployments", name)
}

fn write(target: &ObjectTarget, body: WriteBody, dry_run: bool) -> WriteRequest {
    WriteRequest {
        target: target.clone(),
        body,
        dry_run,
    }
}

fn patch(target: &ObjectTarget, operations: Value) -> WriteRequest {
    write(target, WriteBody::JsonPatch(operations), false)
}

/// Creates a ConfigMap holding `data`, and returns it as stored.
async fn new_config_map(cluster: &impl Cluster, target: &ObjectTarget, data: Value) -> Value {
    let body = json!({"apiVersion": "v1", "kind": "ConfigMap",
        "metadata": {"name": target.name}, "data": data});
    cluster
        .write(&write(target, WriteBody::Create(body), false))
        .await
        .expect("create the ConfigMap");
    stored(cluster, target).await
}

/// Creates a Deployment of no replicas with one container and one variable, and returns it as
/// stored.
async fn new_deployment(cluster: &impl Cluster, target: &ObjectTarget) -> Value {
    let body = json!({"apiVersion": "apps/v1", "kind": "Deployment",
        "metadata": {"name": target.name, "labels": {"app": target.name}},
        "spec": {"replicas": 0, "selector": {"matchLabels": {"app": target.name}},
            "template": {"metadata": {"labels": {"app": target.name}},
                "spec": {"containers": [{"name": "web", "image": "busybox:1.37",
                    "env": [{"name": "MODE", "value": "fast"}]}]}}}});
    cluster
        .write(&write(target, WriteBody::Create(body), false))
        .await
        .expect("create the Deployment");
    stored(cluster, target).await
}

/// The object `target` names as stored, or `None`.
async fn read(
    cluster: &impl Cluster,
    target: &ObjectTarget,
) -> Result<Option<Value>, ClusterError> {
    let found = cluster
        .find_object(&target.resource(), target.namespace(), &target.name)
        .await?;
    Ok(found.map(|object| serde_json::to_value(object).expect("JSON")))
}

async fn stored(cluster: &impl Cluster, target: &ObjectTarget) -> Value {
    read(cluster, target)
        .await
        .expect("read")
        .unwrap_or_else(|| panic!("{} {} is stored", target.kind, target.name))
}

fn text(object: &Value, pointer: &str) -> String {
    object
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn generation(object: &Value) -> i64 {
    object
        .pointer("/metadata/generation")
        .and_then(Value::as_i64)
        .unwrap_or_default()
}

/// `object` without what only the server and controllers write: `status`, the metadata the
/// server sets, and the revision the Deployment controller notes in an annotation.
fn content(object: &Value) -> Value {
    let mut out = object.clone();
    if let Some(map) = out.as_object_mut() {
        map.remove("status");
    }
    if let Some(metadata) = out.get_mut("metadata").and_then(Value::as_object_mut) {
        for field in [
            "resourceVersion",
            "generation",
            "managedFields",
            "uid",
            "creationTimestamp",
        ] {
            metadata.remove(field);
        }
        if let Some(annotations) = metadata
            .get_mut("annotations")
            .and_then(Value::as_object_mut)
        {
            annotations.remove("deployment.kubernetes.io/revision");
            if annotations.is_empty() {
                metadata.remove("annotations");
            }
        }
    }
    out
}

/// kube-apiserver's answer, or a panic naming what came instead.
fn answer(result: Result<zeabur_kube::Written, ClusterError>) -> KubernetesStatus {
    match result {
        Err(ClusterError::Status(status)) => status,
        other => panic!("a status answer: {other:?}"),
    }
}

/// kube-apiserver's code and reason, or a panic naming what came instead.
fn status(result: Result<zeabur_kube::Written, ClusterError>) -> (u16, String) {
    let status = answer(result);
    (status.code, status.reason)
}

/// Given a ConfigMap and a Deployment, when a JSON Patch whose `test` operations hold is
/// written, then it is applied, and the object stored is the object the patch makes of the one
/// read (RFC 6902, as the plan computes it), with a new `resourceVersion`. A Deployment's
/// generation moves when its spec changes, and only then.
pub(crate) async fn a_patch_whose_tests_hold_is_stored_as_computed(
    cluster: &impl Cluster,
    namespace: &str,
) {
    let settings = config_map(namespace, "settings");
    let before = new_config_map(cluster, &settings, json!({"a": "1", "b": "2"})).await;
    let operations = json!([
        {"op": "test", "path": "/metadata/resourceVersion", "value": before["metadata"]["resourceVersion"]},
        {"op": "test", "path": "/data/a", "value": "1"},
        {"op": "replace", "path": "/data/a", "value": "10"},
        {"op": "remove", "path": "/data/b"},
        {"op": "add", "path": "/data/c", "value": "3"}
    ]);
    let written = cluster
        .write(&patch(&settings, operations.clone()))
        .await
        .expect("patch");
    let after = stored(cluster, &settings).await;
    let computed = apply_patch(&before, operations.as_array().unwrap()).unwrap();
    assert_eq!(content(&after), content(&computed));
    let version = text(&after, "/metadata/resourceVersion");
    assert_eq!(written.resource_version.as_deref(), Some(version.as_str()));
    assert_ne!(version, text(&before, "/metadata/resourceVersion"));

    // A keyed list gains an entry in front of the one it had, and the image changes: what a
    // plan's env and YAML edits send.
    let web = deployment(namespace, "web");
    let before = new_deployment(cluster, &web).await;
    let container = "/spec/template/spec/containers/0";
    let operations = json!([
        {"op": "test", "path": "/metadata/generation", "value": generation(&before)},
        {"op": "test", "path": format!("{container}/env/0/name"), "value": "MODE"},
        {"op": "add", "path": format!("{container}/env/0"), "value": {"name": "FIRST", "value": "1"}},
        {"op": "test", "path": format!("{container}/image"), "value": "busybox:1.37"},
        {"op": "replace", "path": format!("{container}/image"), "value": "busybox:1.36"}
    ]);
    cluster
        .write(&patch(&web, operations.clone()))
        .await
        .expect("patch");
    let after = stored(cluster, &web).await;
    let computed = apply_patch(&before, operations.as_array().unwrap()).unwrap();
    assert_eq!(content(&after), content(&computed));
    assert_eq!(generation(&after), generation(&before) + 1);

    // A label is metadata, not spec: the generation stays.
    let label = json!([{"op": "add", "path": "/metadata/labels/tier", "value": "front"}]);
    cluster.write(&patch(&web, label)).await.expect("label");
    let labelled = stored(cluster, &web).await;
    assert_eq!(text(&labelled, "/metadata/labels/tier"), "front");
    assert_eq!(generation(&labelled), generation(&after));
}

/// Given an object read for a plan, when the patch's `test` of its `resourceVersion`, its
/// `generation` or a value it replaces no longer holds, then the write is refused with `422
/// Invalid` naming no invalid field (how the API tells it from an invalid result and answers
/// `409`), and nothing is stored.
pub(crate) async fn a_test_that_fails_refuses_the_write_and_stores_nothing(
    cluster: &impl Cluster,
    namespace: &str,
) {
    let settings = config_map(namespace, "settings");
    let before = new_config_map(cluster, &settings, json!({"a": "1"})).await;
    let version = text(&before, "/metadata/resourceVersion");
    let stale = [
        json!([
            {"op": "test", "path": "/metadata/resourceVersion", "value": format!("{version}0")},
            {"op": "replace", "path": "/data/a", "value": "2"}
        ]),
        json!([
            {"op": "test", "path": "/metadata/resourceVersion", "value": version},
            {"op": "test", "path": "/data/a", "value": "changed meanwhile"},
            {"op": "replace", "path": "/data/a", "value": "2"}
        ]),
    ];
    for operations in stale {
        let answer = answer(cluster.write(&patch(&settings, operations)).await);
        assert_eq!((answer.code, answer.reason.as_str()), (422, "Invalid"));
        assert!(!answer.names_invalid_field, "{answer:?}");
    }
    assert_eq!(stored(cluster, &settings).await, before);

    let web = deployment(namespace, "web");
    let before = new_deployment(cluster, &web).await;
    let operations = json!([
        {"op": "test", "path": "/metadata/generation", "value": generation(&before) + 1},
        {"op": "replace", "path": "/spec/replicas", "value": 1}
    ]);
    let answer = answer(cluster.write(&patch(&web, operations)).await);
    assert_eq!((answer.code, answer.reason.as_str()), (422, "Invalid"));
    assert!(!answer.names_invalid_field, "{answer:?}");
    let after = stored(cluster, &web).await;
    assert_eq!(after.pointer("/spec/replicas"), Some(&json!(0)));
    assert_eq!(generation(&after), generation(&before));
}

/// Given a name that is taken, when an object of that name is created, then it is refused with
/// `409 AlreadyExists`; a patch of an object that does not exist is refused with `404`.
pub(crate) async fn a_taken_name_is_refused_and_a_missing_object_is_not_found(
    cluster: &impl Cluster,
    namespace: &str,
) {
    let settings = config_map(namespace, "settings");
    let before = new_config_map(cluster, &settings, json!({"a": "1"})).await;
    let again = json!({"apiVersion": "v1", "kind": "ConfigMap",
        "metadata": {"name": "settings"}, "data": {"a": "other"}});
    let answer = status(
        cluster
            .write(&write(&settings, WriteBody::Create(again), false))
            .await,
    );
    assert_eq!(answer, (409, "AlreadyExists".to_owned()));
    assert_eq!(stored(cluster, &settings).await, before);

    let missing = config_map(namespace, "missing");
    let add = json!([{"op": "add", "path": "/data/a", "value": "1"}]);
    let answer = status(cluster.write(&patch(&missing, add)).await);
    assert_eq!(answer.0, 404, "{answer:?}");
    assert_eq!(read(cluster, &missing).await, Ok(None));
}

/// Given `fieldValidation=Strict`, which every Zeabur write asks for, when a write names a
/// field the kind's schema does not know, then it is refused, dry run or not: `400 BadRequest`
/// for a creation, `422 Invalid` naming the field for a patch. A dry run that passes stores
/// nothing.
pub(crate) async fn a_dry_run_validates_strictly_and_stores_nothing(
    cluster: &impl Cluster,
    namespace: &str,
) {
    let draft = config_map(namespace, "draft");
    let body = json!({"apiVersion": "v1", "kind": "ConfigMap",
        "metadata": {"name": "draft"}, "data": {"k": "v"}});
    cluster
        .write(&write(&draft, WriteBody::Create(body.clone()), true))
        .await
        .expect("dry-run create");
    assert_eq!(read(cluster, &draft).await, Ok(None));
    let mut unknown = body;
    unknown["bogus"] = json!(1);
    for dry_run in [true, false] {
        let created = cluster
            .write(&write(&draft, WriteBody::Create(unknown.clone()), dry_run))
            .await;
        assert_eq!(status(created), (400, "BadRequest".to_owned()));
    }
    assert_eq!(read(cluster, &draft).await, Ok(None));

    let settings = config_map(namespace, "settings");
    let before = new_config_map(cluster, &settings, json!({"a": "1"})).await;
    let change = json!([{"op": "replace", "path": "/data/a", "value": "2"}]);
    cluster
        .write(&write(&settings, WriteBody::JsonPatch(change), true))
        .await
        .expect("dry-run patch");
    assert_eq!(stored(cluster, &settings).await, before);
    let bogus = json!([{"op": "add", "path": "/bogus", "value": 1}]);
    for dry_run in [true, false] {
        let patched = cluster
            .write(&write(
                &settings,
                WriteBody::JsonPatch(bogus.clone()),
                dry_run,
            ))
            .await;
        let answer = answer(patched);
        assert_eq!((answer.code, answer.reason.as_str()), (422, "Invalid"));
        assert!(answer.names_invalid_field, "{answer:?}");
    }

    assert_eq!(stored(cluster, &settings).await, before);

    let web = deployment(namespace, "web");
    new_deployment(cluster, &web).await;
    let nested =
        json!([{"op": "add", "path": "/spec/template/spec/containers/0/bogus", "value": 1}]);
    let patched = cluster
        .write(&write(&web, WriteBody::JsonPatch(nested), false))
        .await;
    assert_eq!(status(patched), (422, "Invalid".to_owned()));
    let after = stored(cluster, &web).await;
    assert_eq!(
        after.pointer("/spec/template/spec/containers/0/bogus"),
        None
    );
}

/// Given N plans of one ConfigMap made against the same `resourceVersion`, when all are written
/// at once, then exactly one is stored; every other is refused (`422`, or `409` when
/// kube-apiserver's own retries on the conflict run out, or `429` when API Priority and
/// Fairness turns it away).
pub(crate) async fn of_concurrent_patches_of_one_version_exactly_one_is_stored(
    cluster: &impl Cluster,
    namespace: &str,
) {
    const N: usize = 16;
    let settings = config_map(namespace, "settings");
    let before = new_config_map(cluster, &settings, json!({"a": "start"})).await;
    let version = &before["metadata"]["resourceVersion"];
    let writes: Vec<WriteRequest> = (0..N)
        .map(|i| {
            patch(
                &settings,
                json!([
                    {"op": "test", "path": "/metadata/resourceVersion", "value": version},
                    {"op": "replace", "path": "/data/a", "value": format!("plan {i}")}
                ]),
            )
        })
        .collect();
    let results = futures_util::future::join_all(writes.iter().map(|w| cluster.write(w))).await;
    let winners: Vec<usize> = (0..N).filter(|i| results[*i].is_ok()).collect();
    assert_eq!(winners.len(), 1, "{results:?}");
    assert!(
        results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .all(|e| matches!(e.code(), Some(409 | 422 | 429))),
        "{results:?}"
    );
    let after = stored(cluster, &settings).await;
    assert_eq!(text(&after, "/data/a"), format!("plan {}", winners[0]));
}

/// Given a ConfigMap, when it is deleted with a `uid` or `resourceVersion` precondition it no
/// longer meets, then the deletion is refused with `409 Conflict` and it stays; a dry run that
/// meets them keeps it too; a deletion that meets them takes it away, and deleting it again is
/// `404`.
pub(crate) async fn a_deletion_keeps_to_its_preconditions(cluster: &impl Cluster, namespace: &str) {
    let settings = config_map(namespace, "settings");
    let before = new_config_map(cluster, &settings, json!({"a": "1"})).await;
    let uid = text(&before, "/metadata/uid");
    let version = text(&before, "/metadata/resourceVersion");
    assert!(!uid.is_empty() && !version.is_empty(), "{before}");
    let delete = |uid: &str, version: &str, dry_run: bool| {
        write(
            &settings,
            WriteBody::Delete(Preconditions {
                uid: Some(uid.to_owned()),
                resource_version: Some(version.to_owned()),
            }),
            dry_run,
        )
    };
    let older = format!("{version}0");
    for stale in [
        delete("another-uid", &version, false),
        delete(&uid, &older, false),
    ] {
        let answer = status(cluster.write(&stale).await);
        assert_eq!(answer, (409, "Conflict".to_owned()));
    }
    cluster
        .write(&delete(&uid, &version, true))
        .await
        .expect("dry-run delete");
    assert_eq!(stored(cluster, &settings).await, before);
    cluster
        .write(&delete(&uid, &version, false))
        .await
        .expect("delete");
    assert_eq!(read(cluster, &settings).await, Ok(None));
    let again = status(cluster.write(&delete(&uid, &version, false)).await);
    assert_eq!(again.0, 404, "{again:?}");
}
