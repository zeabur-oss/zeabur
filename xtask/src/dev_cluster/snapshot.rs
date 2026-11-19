//! Every cluster-scoped object by kind and name, and every namespace's labels and annotations:
//! what e2e compares before Zeabur's installation and after its removal.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::Namespace;
use serde::Deserialize;

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
    let namespaces: Namespaces =
        serde_json::from_str(&namespaces).map_err(|e| format!("reading namespaces: {e}"))?;
    Ok(format(objects, &namespaces))
}

/// `kubectl get namespaces -o json`.
#[derive(Deserialize)]
struct Namespaces {
    items: Vec<Namespace>,
}

/// `<resource> <kind>/<name>` lines, sorted within each resource (not every list comes in a
/// fixed order), then `<namespace>\t<labels>\t<annotations>` with both as JSON.
fn format(objects: BTreeMap<String, Vec<String>>, namespaces: &Namespaces) -> String {
    let mut lines = Vec::new();
    for (resource, mut names) in objects {
        names.sort();
        lines.extend(names.iter().map(|name| format!("{resource} {name}\n")));
    }
    let json = |map: Option<&BTreeMap<String, String>>| {
        serde_json::to_string(&map.cloned().unwrap_or_default()).unwrap_or_default()
    };
    for namespace in &namespaces.items {
        let metadata = &namespace.metadata;
        lines.push(format!(
            "{}\t{}\t{}\n",
            metadata.name.as_deref().unwrap_or_default(),
            json(metadata.labels.as_ref()),
            json(metadata.annotations.as_ref()),
        ));
    }
    lines.concat()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{Namespaces, format};

    fn namespaces(list: &serde_json::Value) -> Namespaces {
        serde_json::from_value(list.clone()).unwrap()
    }

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
            format(objects, &namespaces(&json!({"items": []}))),
            "clusterroles.rbac.authorization.k8s.io clusterrole.rbac.authorization.k8s.io/admin\n\
             clusterroles.rbac.authorization.k8s.io clusterrole.rbac.authorization.k8s.io/view\n\
             storageclasses.storage.k8s.io storageclass.storage.k8s.io/local-path\n"
        );
    }

    #[test]
    fn a_namespace_line_has_its_labels_and_annotations() {
        let list = json!({"items": [
            {"metadata": {"name": "zeabur-e2e", "labels": {"zeabur.com/platform": "true", "a": "b"}}},
            {"metadata": {"name": "default", "annotations": {"note": "x"}}},
        ]});
        assert_eq!(
            format(BTreeMap::new(), &namespaces(&list)),
            "zeabur-e2e\t{\"a\":\"b\",\"zeabur.com/platform\":\"true\"}\t{}\n\
             default\t{}\t{\"note\":\"x\"}\n"
        );
    }

    #[test]
    fn a_change_of_label_changes_the_snapshot() {
        let with = |value: &str| {
            format(
                BTreeMap::new(),
                &namespaces(
                    &json!({"items": [{"metadata": {"name": "n", "labels": {"k": value}}}]}),
                ),
            )
        };
        assert_ne!(with("1"), with("2"));
    }
}
