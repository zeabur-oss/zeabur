//! What a plan reads before it changes a Secret: its values, where the person may read them.

use std::collections::BTreeMap;

use engawa_kube::Cluster;
use engawa_plan::{Change, DataKind, PlanRequest};

use crate::error::ApiError;

/// The values of the Secrets `request` changes, where the person may read them.
pub(super) async fn secret_values(
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
) -> Result<BTreeMap<String, BTreeMap<String, Vec<u8>>>, ApiError> {
    let mut secret_values = BTreeMap::new();
    for change in &request.changes {
        let secret = if let Change::Data {
            kind: DataKind::Secret,
            name,
            ..
        } = change
        {
            Some(name)
        } else if let Change::Edit { resource, name, .. } = change
            && resource.group.is_empty()
            && resource.resource == "secrets"
        {
            Some(name)
        } else {
            None
        };
        if let Some(name) = secret
            && !secret_values.contains_key(name)
        {
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
