//! What a plan reads before it changes a Secret: its values, read as the person.

use std::collections::BTreeMap;

use zeabur_kube::Cluster;
use zeabur_plan::{Change, DataKind, PlanRequest};

use crate::error::{ApiError, get_read, read_problem};

/// The values of the Secrets `request` changes.
///
/// # Errors
///
/// When the person may not read one: the plan compares an edit with the Secret as read.
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
    Ok(secret_values)
}
