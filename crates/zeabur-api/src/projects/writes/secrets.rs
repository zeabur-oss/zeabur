//! What a plan reads before it changes a Secret: its values, where the person may read them.
//!
//! A `port` change sets `PORT` in the Secret the container imports it from, so the Secrets its
//! workload imports whole are read too.

use std::collections::BTreeMap;

use zeabur_graph::{Snapshot, workloads};
use zeabur_kube::Cluster;
use zeabur_model::WorkloadRef;
use zeabur_plan::{Change, DataKind, PlanRequest};

use crate::error::ApiError;

/// The values of the Secrets `request` changes, where the person may read them.
pub(super) async fn secret_values(
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
    snapshot: &Snapshot,
) -> Result<BTreeMap<String, BTreeMap<String, Vec<u8>>>, ApiError> {
    let mut names = Vec::new();
    for change in &request.changes {
        if let Change::Data {
            kind: DataKind::Secret,
            name,
            ..
        } = change
        {
            names.push(name.clone());
        } else if let Change::Edit { resource, name, .. } = change
            && resource.group.is_empty()
            && resource.resource == "secrets"
        {
            names.push(name.clone());
        } else if let Change::Port { workload, .. } = change {
            names.extend(imported_secrets(snapshot, workload));
        }
    }
    let mut secret_values = BTreeMap::new();
    for name in &names {
        if !secret_values.contains_key(name) {
            match cluster.reveal_secret(project, name).await {
                Ok(values) => {
                    secret_values.insert(name.clone(), values);
                }
                Err(error) if error.is_refused() || error.is_not_found() => {}
                Err(other) => {
                    return Err(ApiError::from(
                        other.into_problem(&format!("Secret {name}")),
                    ));
                }
            }
        }
    }
    Ok(secret_values)
}

/// The Secrets the containers of `reference` import whole.
fn imported_secrets(snapshot: &Snapshot, reference: &WorkloadRef) -> Vec<String> {
    let mut names = Vec::new();
    for workload in workloads(snapshot) {
        if workload.reference() != *reference {
            continue;
        }
        let Some(spec) = workload.pod_spec() else {
            continue;
        };
        for container in &spec.containers {
            for from in container.env_from.iter().flatten() {
                if let Some(secret) = &from.secret_ref {
                    names.push(secret.name.clone());
                }
            }
        }
    }
    names
}
