//! Variables whose ConfigMap or Secret was written after a running pod started: see
//! [`zeabur_model::PendingRestart`] for what that does and does not tell.

use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use k8s_openapi::jiff::Timestamp;
use zeabur_model::{Container, PendingRestart, Source, SourceKind};

use crate::fields::{Step, reaches};
use crate::snapshot::{ByName, Snapshot, name_of};
use crate::workload::Workload;

/// The last write to `meta` by a manager owning `path`, and that manager.
fn last_write(meta: &ObjectMeta, path: &[Step]) -> Option<(Timestamp, String)> {
    meta.managed_fields
        .iter()
        .flatten()
        .filter(|entry| {
            entry
                .fields_v1
                .as_ref()
                .is_some_and(|f| reaches(&f.0, path))
        })
        .filter_map(|entry| Some((entry.time.as_ref()?.0, entry.manager.clone()?)))
        .max_by_key(|(time, _)| *time)
}

/// The running pod of `workload` that started first, with its start.
fn oldest(snapshot: &Snapshot, workload: Workload<'_>) -> Option<(Timestamp, String)> {
    snapshot
        .pods
        .iter()
        .filter(|pod| crate::pods::belongs(snapshot, workload, pod))
        .filter(|pod| pod.metadata.deletion_timestamp.is_none())
        .filter_map(|pod: &Pod| {
            let status = pod.status.as_ref()?;
            (status.phase.as_deref() == Some("Running"))
                .then_some(())
                .and(status.start_time.as_ref())
                .map(|t| (t.0, name_of(&pod.metadata).to_owned()))
        })
        .min_by_key(|(time, _)| *time)
}

fn written_after(
    objects: &ByName<'_>,
    source: &Source,
    started: &(Timestamp, String),
) -> Option<PendingRestart> {
    let name = source.name.as_deref()?;
    let meta = match source.kind {
        SourceKind::ConfigMap => &objects.config_map(name)?.metadata,
        SourceKind::Secret => &objects.secret(name)?.metadata,
        SourceKind::Literal
        | SourceKind::PodField
        | SourceKind::ResourceField
        | SourceKind::File => {
            return None;
        }
    };
    let mut path = vec![Step::field("data")];
    if let Some(key) = &source.key {
        path.push(Step::field(key));
    }
    let (written, manager) = last_write(meta, &path)
        .into_iter()
        .chain(
            source
                .key
                .as_ref()
                .filter(|_| source.kind == SourceKind::ConfigMap)
                .and_then(|key| last_write(meta, &[Step::field("binaryData"), Step::field(key)])),
        )
        .max_by_key(|(time, _)| *time)?;
    (written > started.0).then(|| PendingRestart {
        written_at: written.to_string(),
        manager,
        pod_started_at: started.0.to_string(),
        pod: started.1.clone(),
    })
}

/// Marks the variables and imports of `containers` whose object was written after the oldest
/// running pod of `workload` started.
pub(crate) fn mark(
    snapshot: &Snapshot,
    objects: &ByName<'_>,
    workload: Workload<'_>,
    containers: &mut [Container],
) {
    let Some(started) = oldest(snapshot, workload) else {
        return;
    };
    for container in containers {
        for variable in &mut container.env {
            variable.pending_restart = written_after(objects, &variable.source, &started);
        }
        for import in &mut container.imported_env {
            import.pending_restart = written_after(objects, &import.source, &started);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn snapshot(config_written: &str) -> Snapshot {
        let objects = vec![
            json!({"apiVersion": "apps/v1", "kind": "Deployment",
                "metadata": {"name": "web"},
                "spec": {"selector": {"matchLabels": {"app": "web"}},
                    "template": {"metadata": {"labels": {"app": "web"}},
                        "spec": {"containers": [{"name": "app", "image": "app:1",
                            "env": [{"name": "MODE", "valueFrom":
                                {"configMapKeyRef": {"name": "settings", "key": "mode"}}}],
                            "envFrom": [{"configMapRef": {"name": "settings"}}]}]}}}}),
            json!({"apiVersion": "v1", "kind": "Pod",
                "metadata": {"name": "web-1", "labels": {"app": "web"}},
                "spec": {"containers": [{"name": "app"}]},
                "status": {"phase": "Running", "startTime": "2026-10-01T10:00:00Z"}}),
            json!({"apiVersion": "v1", "kind": "ConfigMap",
                "metadata": {"name": "settings", "managedFields": [
                    {"manager": "kubectl-edit", "operation": "Update",
                        "time": config_written, "fieldsV1": {"f:data": {"f:mode": {}}}},
                    {"manager": "helm", "operation": "Update",
                        "time": "2026-09-01T00:00:00Z", "fieldsV1": {"f:data": {"f:other": {}}}}]},
                "data": {"mode": "on", "other": "x"}}),
        ];
        let mut snapshot = Snapshot {
            namespace: "shop".to_owned(),
            ..Snapshot::default()
        };
        for object in objects {
            snapshot.insert(object).unwrap();
        }
        snapshot
    }

    fn marked(snapshot: &Snapshot) -> Container {
        let project = crate::project(snapshot);
        project.services[0].containers[0].clone()
    }

    #[test]
    fn a_key_written_after_the_pod_started_waits_for_a_restart() {
        let container = marked(&snapshot("2026-10-02T08:00:00Z"));
        let pending = container.env[0].pending_restart.as_ref().unwrap();
        assert_eq!(pending.manager, "kubectl-edit");
        assert_eq!(pending.pod, "web-1");
        assert_eq!(pending.pod_started_at, "2026-10-01T10:00:00Z");
        assert!(container.imported_env[0].pending_restart.is_some());
    }

    #[test]
    fn a_key_written_before_the_pod_started_is_in_it() {
        let container = marked(&snapshot("2026-09-30T08:00:00Z"));
        assert_eq!(container.env[0].pending_restart, None);
        assert_eq!(container.imported_env[0].pending_restart, None);
    }
}
