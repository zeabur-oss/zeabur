//! What a plan reads before it changes a Secret: its values, read as the person.
//!
//! A YAML edit compares the Secret as read, so a Secret it edits must be readable. A `port` change
//! sets `PORT` in the Secret the container imports it from, so the Secrets its workload imports
//! whole are read too, where the person may read them; one they may not read is taken not to hold
//! `PORT`.

use std::collections::BTreeMap;

use zeabur_graph::{Snapshot, workloads};
use zeabur_kube::Cluster;
use zeabur_model::WorkloadRef;
use zeabur_plan::{Change, PlanRequest};

use crate::error::{ApiError, get_read, read_problem};

/// The values of the Secrets `request` edits as YAML, and of those a `port` change's workload
/// imports whole. A key edit reads none: it is sent as a merge patch.
///
/// # Errors
///
/// When the person may not read a Secret edited as YAML: the plan compares the edit with the
/// Secret as read.
pub(super) async fn secret_values(
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
    snapshot: &Snapshot,
) -> Result<BTreeMap<String, BTreeMap<String, Vec<u8>>>, ApiError> {
    let mut secret_values = BTreeMap::new();
    for change in &request.changes {
        if let Change::Edit { resource, name, .. } = change
            && resource.group.is_empty()
            && resource.resource == "secrets"
            && !secret_values.contains_key(name)
        {
            match cluster.reveal_secret(project, name).await {
                Ok(values) => {
                    secret_values.insert(name.clone(), values);
                }
                Err(error) if error.is_not_found() => {}
                Err(other) => {
                    return Err(read_problem(
                        other,
                        &format!("Secret {name}"),
                        get_read("", "secrets", Some(project), name),
                    ));
                }
            }
        }
    }
    for change in &request.changes {
        let Change::Port { workload, .. } = change else {
            continue;
        };
        for name in imported_secrets(snapshot, workload) {
            if secret_values.contains_key(&name) {
                continue;
            }
            match cluster.reveal_secret(project, &name).await {
                Ok(values) => {
                    secret_values.insert(name, values);
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
