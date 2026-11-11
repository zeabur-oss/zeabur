//! What a plan reads before it edits a Secret as YAML: its values, read as the person.

use std::collections::BTreeMap;

use zeabur_kube::Cluster;
use zeabur_plan::{Change, PlanRequest};

use crate::error::{ApiError, get_read, read_problem};

/// The values of the Secrets `request` edits as YAML. A key edit reads none: it is sent as a
/// merge patch.
///
/// # Errors
///
/// When the person may not read one: the plan compares a YAML edit with the Secret as read.
pub(super) async fn secret_values(
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
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
    Ok(secret_values)
}
