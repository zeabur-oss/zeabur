//! A workload's releases: the pod templates it has run, newest first, and which one runs now.
//!
//! - A **Deployment** keeps one ReplicaSet per template; the ReplicaSet's
//!   `deployment.kubernetes.io/revision` annotation numbers it, and the Deployment's own annotation
//!   names the current one [k8s-deployment-rollback].
//! - A **StatefulSet** or **DaemonSet** keeps one ControllerRevision per template, numbered by its
//!   `revision` field; its `data` is the patch that restores the template. A StatefulSet names the
//!   current one in `status.updateRevision`; for a DaemonSet it is the highest number
//!   [k8s-controllerrevision]. Their pods carry the revision in the `controller-revision-hash`
//!   label.
//!
//! How many old releases remain is the workload's `revisionHistoryLimit` (10 by default).
//!
//! [k8s-deployment-rollback]: https://kubernetes.io/docs/concepts/workloads/controllers/deployment/#rolling-back-a-deployment
//! [k8s-controllerrevision]: https://kubernetes.io/docs/reference/kubernetes-api/workload-resources/controller-revision-v1/

use k8s_openapi::api::apps::v1::{ControllerRevision, ReplicaSet};
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeabur_model::{WorkloadKind, WorkloadRef};

/// The annotation numbering a Deployment's ReplicaSets.
pub const REVISION: &str = "deployment.kubernetes.io/revision";
/// The annotation `kubectl` and people use to say why a release was made.
pub const CHANGE_CAUSE: &str = "kubernetes.io/change-cause";
/// The label a Deployment adds to each ReplicaSet's template.
const POD_TEMPLATE_HASH: &str = "pod-template-hash";
/// The label a StatefulSet or DaemonSet puts on each pod.
const REVISION_HASH: &str = "controller-revision-hash";

/// How many of a release's pods there are and how many are ready.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = WorkloadReleasePods))]
pub struct ReleasePods {
    /// Ready.
    pub ready: i32,
    /// All.
    pub total: i32,
}

/// One pod template a workload has run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", schema(as = WorkloadRelease))]
pub struct Release {
    /// Its number; higher is newer.
    pub revision: i64,
    /// The ReplicaSet or ControllerRevision that holds it.
    pub name: String,
    /// The images of its containers, in order.
    pub images: Vec<String>,
    /// When it was made, RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// Why it was made, from `kubernetes.io/change-cause`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_cause: Option<String>,
    /// It is the template the workload runs now.
    pub current: bool,
    /// Its pods.
    pub pods: ReleasePods,
    /// The pod template, as the workload would hold it again after a rollback.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub template: Value,
    /// `template` as YAML, written as the object's YAML view writes it.
    #[serde(default)]
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub template_yaml: String,
}

fn is_controlled_by(meta: &ObjectMeta, kind: &str, name: &str) -> bool {
    zeabur_adapters::controller_of(meta).is_some_and(|o| o.kind == kind && o.name == name)
}

fn images(template: &Value) -> Vec<String> {
    template
        .pointer("/spec/containers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("image").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

fn annotation(meta: &ObjectMeta, name: &str) -> Option<String> {
    meta.annotations.as_ref()?.get(name).cloned()
}

fn created(meta: &ObjectMeta) -> Option<String> {
    meta.creation_timestamp.as_ref().map(|t| t.0.to_string())
}

/// The releases of `workload`, newest first.
///
/// `object` is the workload as JSON; `replica_sets`, `revisions` and `pods` are those of its
/// namespace (only the ones it controls are used). Jobs and CronJobs have no releases.
#[must_use]
pub fn releases(
    workload: &WorkloadRef,
    object: &Value,
    replica_sets: &[ReplicaSet],
    revisions: &[ControllerRevision],
    pods: &[Pod],
) -> Vec<Release> {
    let mut out = match workload.kind {
        WorkloadKind::Deployment => deployment_releases(&workload.name, object, replica_sets),
        WorkloadKind::StatefulSet | WorkloadKind::DaemonSet => {
            revision_releases(workload, object, revisions, pods)
        }
        WorkloadKind::Job | WorkloadKind::CronJob => Vec::new(),
    };
    out.sort_by_key(|r| std::cmp::Reverse(r.revision));
    out
}

fn deployment_releases(name: &str, object: &Value, replica_sets: &[ReplicaSet]) -> Vec<Release> {
    let current = object
        .pointer("/metadata/annotations")
        .and_then(|a| a.get(REVISION))
        .and_then(Value::as_str)
        .and_then(|r| r.parse::<i64>().ok());
    replica_sets
        .iter()
        .filter(|rs| is_controlled_by(&rs.metadata, "Deployment", name))
        .filter_map(|rs| {
            let revision = annotation(&rs.metadata, REVISION)?.parse::<i64>().ok()?;
            let mut template = serde_json::to_value(rs.spec.as_ref()?.template.as_ref()?).ok()?;
            if let Some(labels) = template
                .pointer_mut("/metadata/labels")
                .and_then(Value::as_object_mut)
            {
                labels.remove(POD_TEMPLATE_HASH);
            }
            let status = rs.status.as_ref();
            Some(Release {
                revision,
                name: rs.metadata.name.clone().unwrap_or_default(),
                images: images(&template),
                created: created(&rs.metadata),
                change_cause: annotation(&rs.metadata, CHANGE_CAUSE),
                current: current == Some(revision),
                pods: ReleasePods {
                    ready: status.and_then(|s| s.ready_replicas).unwrap_or(0),
                    total: status.map_or(0, |s| s.replicas),
                },
                template_yaml: zeabur_graph::manifest::to_yaml(&template).unwrap_or_default(),
                template,
            })
        })
        .collect()
}

fn revision_releases(
    workload: &WorkloadRef,
    object: &Value,
    revisions: &[ControllerRevision],
    pods: &[Pod],
) -> Vec<Release> {
    let kind = workload.kind.as_str();
    let owned: Vec<&ControllerRevision> = revisions
        .iter()
        .filter(|r| is_controlled_by(&r.metadata, kind, &workload.name))
        .collect();
    let newest = owned.iter().map(|r| r.revision).max();
    let update = object
        .pointer("/status/updateRevision")
        .and_then(Value::as_str);
    owned
        .into_iter()
        .filter_map(|r| {
            let name = r.metadata.name.clone().unwrap_or_default();
            let mut template = r.data.as_ref()?.0.pointer("/spec/template")?.clone();
            if let Some(map) = template.as_object_mut() {
                map.remove("$patch");
            }
            let mine: Vec<&Pod> = pods
                .iter()
                .filter(|p| is_controlled_by(&p.metadata, kind, &workload.name))
                .filter(|p| {
                    p.metadata
                        .labels
                        .as_ref()
                        .and_then(|l| l.get(REVISION_HASH))
                        .is_some_and(|hash| {
                            *hash == name
                                || name
                                    .strip_suffix(hash.as_str())
                                    .is_some_and(|rest| rest.ends_with('-'))
                        })
                })
                .collect();
            let ready = mine
                .iter()
                .filter(|p| {
                    p.status
                        .as_ref()
                        .and_then(|s| s.conditions.as_ref())
                        .is_some_and(|c| c.iter().any(|c| c.type_ == "Ready" && c.status == "True"))
                })
                .count();
            let current = update.map_or_else(|| newest == Some(r.revision), |u| u == name);
            Some(Release {
                revision: r.revision,
                images: images(&template),
                created: created(&r.metadata),
                change_cause: annotation(&r.metadata, CHANGE_CAUSE),
                current,
                pods: ReleasePods {
                    ready: i32::try_from(ready).unwrap_or(i32::MAX),
                    total: i32::try_from(mine.len()).unwrap_or(i32::MAX),
                },
                name,
                template_yaml: zeabur_graph::manifest::to_yaml(&template).unwrap_or_default(),
                template,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::{from_value, json};

    use super::*;

    fn owner(kind: &str, name: &str) -> Value {
        json!([{"apiVersion": "apps/v1", "kind": kind, "name": name, "uid": "u", "controller": true}])
    }

    #[test]
    fn a_deployment_numbers_its_replica_sets_and_names_the_current_one() {
        let deployment = json!({"metadata": {"name": "web", "annotations": {REVISION: "3"}}});
        let rs = |n: i64, image: &str, ready: i32| -> ReplicaSet {
            from_value(json!({
                "metadata": {"name": format!("web-{n}"), "ownerReferences": owner("Deployment", "web"),
                             "annotations": {REVISION: n.to_string(), CHANGE_CAUSE: format!("image {image}")}},
                "spec": {"selector": {}, "template": {"metadata": {"labels": {"app": "web", POD_TEMPLATE_HASH: "abc"}},
                         "spec": {"containers": [{"name": "app", "image": image}]}}},
                "status": {"replicas": ready, "readyReplicas": ready}
            }))
            .unwrap()
        };
        let other: ReplicaSet = from_value(json!({
            "metadata": {"name": "api-1", "ownerReferences": owner("Deployment", "api"), "annotations": {REVISION: "9"}},
            "spec": {"selector": {}, "template": {}}
        }))
        .unwrap();
        let list = releases(
            &WorkloadRef {
                kind: WorkloadKind::Deployment,
                name: "web".to_owned(),
            },
            &deployment,
            &[rs(2, "web:2", 0), rs(3, "web:3", 2), other],
            &[],
            &[],
        );
        assert_eq!(list.iter().map(|r| r.revision).collect::<Vec<_>>(), [3, 2]);
        assert!(list[0].current && !list[1].current);
        assert_eq!(list[0].images, ["web:3"]);
        assert_eq!(list[0].pods, ReleasePods { ready: 2, total: 2 });
        assert_eq!(list[1].change_cause.as_deref(), Some("image web:2"));
        assert_eq!(
            list[1].template["metadata"]["labels"],
            json!({"app": "web"}),
            "no pod-template-hash"
        );
    }

    #[test]
    fn a_stateful_set_reads_controller_revisions_and_counts_their_pods() {
        let set = json!({"metadata": {"name": "db"}, "status": {"updateRevision": "db-bbb"}});
        let revision = |name: &str, n: i64, image: &str| -> ControllerRevision {
            from_value(json!({
                "metadata": {"name": name, "ownerReferences": owner("StatefulSet", "db")},
                "revision": n,
                "data": {"spec": {"template": {"$patch": "replace", "metadata": {"labels": {"app": "db"}},
                         "spec": {"containers": [{"name": "db", "image": image}]}}}}
            }))
            .unwrap()
        };
        let pod = |name: &str, hash: &str, ready: &str| -> Pod {
            from_value(json!({
                "metadata": {"name": name, "ownerReferences": owner("StatefulSet", "db"),
                             "labels": {REVISION_HASH: hash}},
                "status": {"conditions": [{"type": "Ready", "status": ready}]}
            }))
            .unwrap()
        };
        let list = releases(
            &WorkloadRef {
                kind: WorkloadKind::StatefulSet,
                name: "db".to_owned(),
            },
            &set,
            &[],
            &[
                revision("db-aaa", 1, "pg:15"),
                revision("db-bbb", 2, "pg:16"),
            ],
            &[
                pod("db-0", "db-bbb", "True"),
                pod("db-1", "db-aaa", "False"),
            ],
        );
        assert_eq!(list[0].name, "db-bbb");
        assert!(list[0].current);
        assert_eq!(list[0].pods, ReleasePods { ready: 1, total: 1 });
        assert_eq!(list[1].pods, ReleasePods { ready: 0, total: 1 });
        assert!(list[1].template.get("$patch").is_none());
    }
}
