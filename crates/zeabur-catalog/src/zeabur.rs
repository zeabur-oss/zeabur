//! The objects Zeabur adds when it installs a component.
//!
//! These are the alert rules of its notifications with VictoriaMetrics, and Zeabur Proxy's
//! access to the log store with VictoriaLogs. They ship with the Zeabur version that reads them,
//! not with the catalog. [`engine::steps_of`] adds them as a last step of the component, and
//! `zeaburctl components export` prints them.
//!
//! [`engine::steps_of`]: crate::engine::steps_of

use serde_json::Value;
use sha2::{Digest as _, Sha256};
use zeabur_kube::DynamicObject;

use crate::engine::{SOURCE_DIGEST, label_release};

/// The release name of the step with Zeabur's objects.
pub const RELEASE: &str = "zeabur";

/// The namespace the files name as Zeabur's; [`objects`] puts the real one in its place.
const FILE_NAMESPACE: &str = "zeabur";

/// The label a namespace selector matches a namespace's name with.
const NAMESPACE_NAME: &str = "kubernetes.io/metadata.name";

/// Each component with Zeabur's objects, and the file that holds them.
const FILES: [(&str, &str); 2] = [
    (
        "victoria-metrics",
        include_str!("../zeabur/notification-rules.yaml"),
    ),
    ("victoria-logs", include_str!("../zeabur/proxy-logs.yaml")),
];

/// The file of `component`'s objects.
fn file(component: &str) -> Option<&'static str> {
    FILES
        .iter()
        .find(|(id, _)| *id == component)
        .map(|(_, text)| *text)
}

/// The SHA-256 of the file of `component`'s objects, hex; `None` when Zeabur adds none. A
/// written object carries it in [`SOURCE_DIGEST`], so a different value means another Zeabur
/// version wrote it.
#[must_use]
pub fn digest(component: &str) -> Option<String> {
    file(component).map(|text| hex::encode(Sha256::digest(text)))
}

/// The objects Zeabur adds to `component`, labelled as the component's [`RELEASE`] step, for a
/// Zeabur that runs in `zeabur_namespace`; empty when it adds none.
///
/// # Errors
///
/// A file does not read, which a test rules out.
pub fn objects(component: &str, zeabur_namespace: &str) -> Result<Vec<DynamicObject>, String> {
    let Some(text) = file(component) else {
        return Ok(Vec::new());
    };
    let digest = digest(component).unwrap_or_default();
    let documents: Vec<Value> = serde_saphyr::from_multiple(text).map_err(|e| e.to_string())?;
    documents
        .into_iter()
        .filter(|d| !d.is_null())
        .map(|mut document| {
            set_namespace(&mut document, zeabur_namespace);
            let mut object: DynamicObject =
                serde_json::from_value(document).map_err(|e| e.to_string())?;
            label_release(&mut object.metadata, component, RELEASE);
            object
                .metadata
                .annotations
                .get_or_insert_default()
                .insert(SOURCE_DIGEST.to_owned(), digest.clone());
            Ok(object)
        })
        .collect()
}

/// [`objects`] as YAML documents; empty when Zeabur adds none.
///
/// # Errors
///
/// A file does not read, which a test rules out.
pub fn yaml(component: &str, zeabur_namespace: &str) -> Result<String, String> {
    let mut out = String::new();
    for object in objects(component, zeabur_namespace)? {
        if !out.is_empty() {
            out.push_str("---\n");
        }
        out.push_str(&serde_saphyr::to_string(&object).map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// Puts `namespace` in each namespace selector that names [`FILE_NAMESPACE`].
fn set_namespace(value: &mut Value, namespace: &str) {
    match value {
        Value::Object(fields) => {
            for (key, field) in fields.iter_mut() {
                if key == NAMESPACE_NAME && field.as_str() == Some(FILE_NAMESPACE) {
                    *field = namespace.into();
                } else {
                    set_namespace(field, namespace);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|i| set_namespace(i, namespace)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_reads() {
        for (component, _) in FILES {
            let objects = objects(component, "zeabur").unwrap();
            assert!(!objects.is_empty(), "{component}");
        }
    }

    #[test]
    fn the_proxy_policy_admits_zeaburs_namespace() {
        let objects = objects("victoria-logs", "platform").unwrap();
        let policy = serde_json::to_value(&objects[0]).unwrap();
        let from = &policy["spec"]["ingress"][0]["from"][0];
        assert_eq!(
            from["namespaceSelector"]["matchLabels"][NAMESPACE_NAME],
            "platform"
        );
        assert_eq!(
            from["podSelector"]["matchLabels"]["app.kubernetes.io/name"],
            "zeabur-proxy"
        );
    }

    #[test]
    fn objects_are_labelled_as_the_components_step() {
        let objects = objects("victoria-metrics", "zeabur").unwrap();
        let metadata = &objects[0].metadata;
        let labels = metadata.labels.as_ref().unwrap();
        assert_eq!(labels[crate::engine::COMPONENT], "victoria-metrics");
        assert_eq!(labels[crate::engine::RELEASE], RELEASE);
        assert_eq!(
            metadata.annotations.as_ref().unwrap()[SOURCE_DIGEST],
            digest("victoria-metrics").unwrap()
        );
    }

    /// vmalert-tool, from the dev shell, evaluates the rules as vmalert does.
    #[test]
    fn the_notification_rules_fire_on_the_events() {
        let rules = objects("victoria-metrics", "zeabur").unwrap();
        let spec = serde_json::to_value(&rules[0]).unwrap()["spec"].clone();
        let dir = std::env::temp_dir().join(format!("zeabur-rules-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("rules.yaml"),
            serde_saphyr::to_string(&spec).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("tests.yaml"),
            include_str!("../zeabur/notification-rules.test.yaml"),
        )
        .unwrap();
        let out = std::process::Command::new("vmalert-tool")
            .args([
                "unittest",
                "--disableAlertgroupLabel",
                "--files",
                "tests.yaml",
            ])
            .current_dir(&dir)
            .output()
            .expect("vmalert-tool from the dev shell");
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(
            out.status.success(),
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn other_components_get_nothing() {
        assert_eq!(objects("cert-manager", "zeabur").unwrap(), Vec::new());
        assert!(digest("cert-manager").is_none());
    }
}
