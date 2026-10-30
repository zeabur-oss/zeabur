//! Every cluster-scoped object by kind and name, and every namespace's labels and annotations:
//! what e2e-l2 compares before Zeabur's installation and after its removal.

use std::collections::BTreeMap;

use serde_json::Value;

use super::DevCluster;

/// Lists the cluster. A kind whose list fails (an aggregated API that is not ready) is left out,
/// as `kubectl api-resources` leaves it out.
pub(crate) fn take(dev: &DevCluster) -> Result<String, String> {
    let resources = dev
        .kubectl()
        .args([
            "api-resources",
            "--namespaced=false",
            "--verbs=list",
            "-o",
            "name",
        ])
        .output()?;
    let mut objects = BTreeMap::new();
    for resource in String::from_utf8_lossy(&resources.stdout).split_whitespace() {
        let listed = dev
            .kubectl()
            .args(["get", resource, "-o", "name"])
            .output()?;
        let names = String::from_utf8_lossy(&listed.stdout)
            .lines()
            .map(str::to_owned)
            .collect();
        objects.insert(resource.to_owned(), names);
    }
    let namespaces = dev
        .kubectl()
        .args(["get", "namespaces", "-o", "json"])
        .read()?;
    let namespaces =
        serde_json::from_str(&namespaces).map_err(|e| format!("reading namespaces: {e}"))?;
    Ok(format(objects, &namespaces))
}

/// `<resource> <kind>/<name>` lines, sorted within each resource (not every list comes in a
/// fixed order), then `<namespace>\t<labels>\t<annotations>` with both as JSON.
fn format(objects: BTreeMap<String, Vec<String>>, namespaces: &Value) -> String {
    let mut lines = Vec::new();
    for (resource, mut names) in objects {
        names.sort();
        lines.extend(names.iter().map(|name| format!("{resource} {name}\n")));
    }
    let empty = Value::Object(serde_json::Map::new());
    for namespace in namespaces["items"].as_array().into_iter().flatten() {
        let metadata = &namespace["metadata"];
        let or_empty = |value: &Value| {
            if value.is_null() {
                empty.to_string()
            } else {
                value.to_string()
            }
        };
        lines.push(format!(
            "{}\t{}\t{}\n",
            metadata["name"].as_str().unwrap_or_default(),
            or_empty(&metadata["labels"]),
            or_empty(&metadata["annotations"]),
        ));
    }
    lines.concat()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::format;

    #[test]
    fn objects_come_sorted_by_resource_then_name() {
        let objects = BTreeMap::from([
            (
                "storageclasses.storage.k8s.io".to_owned(),
                vec!["storageclass.storage.k8s.io/local-path".to_owned()],
            ),
            (
                "clusterroles.rbac.authorization.k8s.io".to_owned(),
                vec![
                    "clusterrole.rbac.authorization.k8s.io/view".to_owned(),
                    "clusterrole.rbac.authorization.k8s.io/admin".to_owned(),
                ],
            ),
        ]);
        assert_eq!(
            format(objects, &json!({"items": []})),
            "clusterroles.rbac.authorization.k8s.io clusterrole.rbac.authorization.k8s.io/admin\n\
             clusterroles.rbac.authorization.k8s.io clusterrole.rbac.authorization.k8s.io/view\n\
             storageclasses.storage.k8s.io storageclass.storage.k8s.io/local-path\n"
        );
    }

    #[test]
    fn a_namespace_line_has_its_labels_and_annotations() {
        let namespaces = json!({"items": [
            {"metadata": {"name": "zeabur-e2e", "labels": {"zeabur.com/platform": "true", "a": "b"}}},
            {"metadata": {"name": "default", "annotations": {"note": "x"}}},
        ]});
        assert_eq!(
            format(BTreeMap::new(), &namespaces),
            "zeabur-e2e\t{\"a\":\"b\",\"zeabur.com/platform\":\"true\"}\t{}\n\
             default\t{}\t{\"note\":\"x\"}\n"
        );
    }

    #[test]
    fn a_change_of_label_changes_the_snapshot() {
        let with = |value: &str| {
            format(
                BTreeMap::new(),
                &json!({"items": [{"metadata": {"name": "n", "labels": {"k": value}}}]}),
            )
        };
        assert_ne!(with("1"), with("2"));
    }
}
