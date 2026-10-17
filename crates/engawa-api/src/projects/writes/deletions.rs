//! What a plan reads before it deletes an object: what goes with it, the volume a claim
//! leaves behind, and what deleting a NetworkPolicy opens.

use std::collections::BTreeMap;

use engawa_kube::k8s_openapi::api::apps::v1::ReplicaSet;
use engawa_kube::{Cluster, ClusterExt as _};
use engawa_model::WorkloadRef;
use engawa_plan::{Change, PlanRequest};

use crate::error::ApiError;

/// The objects `request` deletes, as `(resource, name)`.
pub(super) fn deleted(
    request: &PlanRequest,
) -> impl Iterator<Item = (&engawa_plan::ResourceRef, &str)> {
    request.changes.iter().filter_map(|c| {
        if let Change::Delete { resource, name, .. } = c {
            Some((resource, name.as_str()))
        } else {
            None
        }
    })
}

/// The project's ReplicaSets when `request` deletes a Deployment, so the plan can list them
/// with its pods; empty when the person may not list them (the pods are still found by the
/// Deployment's selector).
pub(super) async fn replica_sets_of_deleted(
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
) -> Result<Vec<ReplicaSet>, ApiError> {
    if !deleted(request).any(|(r, _)| r.group == "apps" && r.resource == "deployments") {
        return Ok(Vec::new());
    }
    match cluster.list::<ReplicaSet>(Some(project)).await {
        Ok(list) => Ok(list),
        Err(error) if error.is_refused() => Ok(Vec::new()),
        Err(other) => Err(ApiError::from(other.into_problem("the ReplicaSets"))),
    }
}

/// The reclaim policy of the PersistentVolume of each claim `request` deletes, as the person
/// may read it [k8s-reclaim]; a volume they may not read is left out, and the plan says the
/// policy is unknown.
///
/// [k8s-reclaim]: https://kubernetes.io/docs/concepts/storage/persistent-volumes/#reclaiming
pub(super) async fn reclaim_policies(
    cluster: &dyn Cluster,
    request: &PlanRequest,
    snapshot: &engawa_graph::Snapshot,
) -> Result<BTreeMap<String, String>, ApiError> {
    let volumes = engawa_kube::ResourceRef::new("", "v1", "persistentvolumes", "PersistentVolume");
    let mut out = BTreeMap::new();
    for (resource, name) in deleted(request) {
        if !(resource.group.is_empty() && resource.resource == "persistentvolumeclaims") {
            continue;
        }
        let Some(volume) = snapshot
            .persistent_volume_claim(name)
            .and_then(|c| c.spec.as_ref())
            .and_then(|s| s.volume_name.clone())
        else {
            continue;
        };
        match cluster.get_object(&volumes, None, &volume).await {
            Ok(Some(found)) => {
                if let Some(policy) = found
                    .data
                    .pointer("/spec/persistentVolumeReclaimPolicy")
                    .and_then(serde_json::Value::as_str)
                {
                    out.insert(volume, policy.to_owned());
                }
            }
            Ok(None) => {}
            Err(error) if error.is_refused() || error.is_not_found() => {}
            Err(other) => {
                return Err(ApiError::from(
                    other.into_problem(&format!("PersistentVolume {volume}")),
                ));
            }
        }
    }
    Ok(out)
}

/// What deleting each NetworkPolicy of `names` opens, from the project's policies as the person
/// reads them (`engawa_modules::network_policy::opened_without`), in order; none for a policy
/// whose removal opens nothing, and none at all when the policies could not be read. The
/// project is projected and its policies read once, whatever the number of names.
pub(super) async fn network_policies_opened(
    cluster: &dyn Cluster,
    snapshot: &engawa_graph::Snapshot,
    names: &[&str],
) -> Result<Vec<engawa_plan::Warning>, ApiError> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let project = engawa_graph::project(snapshot);
    let mut unreadable = Vec::new();
    let Some(facts) =
        crate::projects::network_policy::read(cluster, snapshot, &project, &mut unreadable).await?
    else {
        return Ok(Vec::new());
    };
    let by_id: BTreeMap<String, WorkloadRef> = engawa_graph::workloads(snapshot)
        .into_iter()
        .map(|w| {
            (
                engawa_model::id::service(&snapshot.namespace, w.kind(), w.name()),
                w.reference(),
            )
        })
        .collect();
    let refs = |ids: &[String]| -> Vec<WorkloadRef> {
        ids.iter().filter_map(|id| by_id.get(id).cloned()).collect()
    };
    let mut out = Vec::new();
    for &name in names {
        let opened = engawa_modules::network_policy::opened_without(facts.input(), name);
        let connections: Vec<engawa_plan::ConnectionRef> = opened
            .connections
            .iter()
            .filter_map(|(from, to)| {
                Some(engawa_plan::ConnectionRef {
                    from: by_id.get(from)?.clone(),
                    to: by_id.get(to)?.clone(),
                })
            })
            .collect();
        let (ingress, egress) = (refs(&opened.ingress), refs(&opened.egress));
        if ingress.is_empty() && egress.is_empty() && connections.is_empty() {
            continue;
        }
        out.push(engawa_plan::Warning::NetworkPolicyRemoved {
            policy: name.to_owned(),
            ingress_opened: ingress,
            egress_opened: egress,
            connections_opened: connections,
        });
    }
    Ok(out)
}
