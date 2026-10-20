//! What a plan reads before it rolls a service back: its releases.

use std::collections::BTreeMap;

use zeabur_kube::k8s_openapi::api::apps::v1::{ControllerRevision, ReplicaSet};
use zeabur_kube::{Cluster, ClusterExt as _};
use zeabur_model::WorkloadRef;
use zeabur_plan::{Change, PlanRequest, Release};

use crate::error::{ApiError, list_read, read_problem};

/// The releases of the workloads `request` rolls back.
pub(super) async fn releases_to_roll_back(
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
    snapshot: &zeabur_graph::Snapshot,
) -> Result<BTreeMap<WorkloadRef, Vec<Release>>, ApiError> {
    let wanted: Vec<&WorkloadRef> = request
        .changes
        .iter()
        .filter_map(|c| {
            if let Change::Rollback { workload, .. } = c {
                Some(workload)
            } else {
                None
            }
        })
        .collect();
    let mut out = BTreeMap::new();
    if wanted.is_empty() {
        return Ok(out);
    }
    let history = History::read(cluster, project).await?;
    for workload in wanted {
        if let Some(found) = zeabur_graph::workloads(snapshot)
            .into_iter()
            .find(|w| w.kind() == workload.kind && w.name() == workload.name)
        {
            out.insert(workload.clone(), history.of(found, &snapshot.pods));
        }
    }
    Ok(out)
}

/// A namespace's `ReplicaSets` and `ControllerRevisions`, read once.
pub(crate) struct History {
    replica_sets: Vec<ReplicaSet>,
    revisions: Vec<ControllerRevision>,
}

impl History {
    pub(crate) async fn read(cluster: &dyn Cluster, project: &str) -> Result<Self, ApiError> {
        let (replica_sets, revisions) = futures_util::join!(
            cluster.list::<ReplicaSet>(Some(project)),
            cluster.list::<ControllerRevision>(Some(project))
        );
        Ok(Self {
            replica_sets: replica_sets.map_err(|e| {
                read_problem(
                    e,
                    "the ReplicaSets",
                    list_read("apps", "replicasets", Some(project)),
                )
            })?,
            revisions: revisions.map_err(|e| {
                read_problem(
                    e,
                    "the ControllerRevisions",
                    list_read("apps", "controllerrevisions", Some(project)),
                )
            })?,
        })
    }

    /// The releases of `workload`, newest first.
    pub(crate) fn of(
        &self,
        workload: zeabur_graph::Workload<'_>,
        pods: &[zeabur_kube::k8s_openapi::api::core::v1::Pod],
    ) -> Vec<Release> {
        let object = match workload {
            zeabur_graph::Workload::Deployment(o) => serde_json::to_value(o),
            zeabur_graph::Workload::StatefulSet(o) => serde_json::to_value(o),
            zeabur_graph::Workload::DaemonSet(o) => serde_json::to_value(o),
            zeabur_graph::Workload::CronJob(o) => serde_json::to_value(o),
            zeabur_graph::Workload::Job(o) => serde_json::to_value(o),
        }
        .unwrap_or_default();
        zeabur_plan::releases::releases(
            &workload.reference(),
            &object,
            &self.replica_sets,
            &self.revisions,
            pods,
        )
    }
}
