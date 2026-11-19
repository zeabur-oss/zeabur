//! What Zeabur relies on from Zeabur Identity and Zeabur Builder (`fixtures/contracts`).
//!
//! An object of `identity.zeabur.com` or `builder.zeabur.com` is checked against its CRD's
//! `openAPIV3Schema` [k8s-structural]: a field the schema does not have, a value of another type
//! or outside an enum, or, for an object Zeabur writes, a missing required field, is refused.
//! Formats, patterns, bounds and CEL rules are not checked.
//!
//! [k8s-structural]: https://kubernetes.io/docs/tasks/extend-kubernetes/custom-resources/custom-resource-definitions/#specifying-a-structural-schema

use std::sync::LazyLock;

use serde_json::{Map, Value};

const IDENTITY_CRDS: &str = include_str!("../../../fixtures/contracts/identity/crds.json");
const BUILDER_CRDS: &str = include_str!("../../../fixtures/contracts/builder/crds.yaml");

/// The projects, by API group.
const PROJECTS: [(&str, &str); 2] = [
    ("identity.zeabur.com", "Zeabur Identity"),
    ("builder.zeabur.com", "Zeabur Builder"),
];

static CRDS: LazyLock<Result<Vec<Value>, String>> = LazyLock::new(|| {
    let identity: Value = serde_json::from_str(IDENTITY_CRDS)
        .map_err(|e| format!("fixtures/contracts/identity/crds.json: {e}"))?;
    let mut crds = identity
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .ok_or("fixtures/contracts/identity/crds.json is not a List")?;
    let builder = serde_saphyr::from_multiple::<Value>(BUILDER_CRDS)
        .map_err(|e| format!("fixtures/contracts/builder/crds.yaml: {e}"))?;
    crds.extend(builder.into_iter().filter(|crd| !crd.is_null()));
    Ok(crds)
});

/// Every CRD of Zeabur Identity and Zeabur Builder.
///
/// # Errors
///
/// When a fixture does not parse.
pub fn crds() -> Result<&'static [Value], String> {
    CRDS.as_deref().map_err(Clone::clone)
}

/// A served kind of a contract CRD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Served {
    /// The API group.
    pub group: String,
    /// The version.
    pub version: String,
    /// The plural resource name.
    pub resource: String,
    /// The kind.
    pub kind: String,
    /// Whether its objects live in a namespace.
    pub namespaced: bool,
}

/// The served kinds of `group`'s CRDs.
///
/// # Errors
///
/// When a fixture does not parse.
pub fn served(group: &str) -> Result<Vec<Served>, String> {
    let mut served = Vec::new();
    for crd in crds()? {
        let spec = crd.get("spec").unwrap_or(&Value::Null);
        if text(spec, "group") != group {
            continue;
        }
        let names = spec.get("names").unwrap_or(&Value::Null);
        for version in versions(spec) {
            if version.get("served").and_then(Value::as_bool) == Some(true) {
                served.push(Served {
                    group: group.to_owned(),
                    version: text(version, "name").to_owned(),
                    resource: text(names, "plural").to_owned(),
                    kind: text(names, "kind").to_owned(),
                    namespaced: text(spec, "scope") == "Namespaced",
                });
            }
        }
    }
    Ok(served)
}

/// Checks an object Zeabur reads; a contract object may leave out required fields, as a fixture
/// fills in only what a case depends on. Any other object passes.
///
/// # Errors
///
/// What does not fit the CRD.
pub fn check_read(object: &Value) -> Result<(), String> {
    check(object, false)
}

/// Checks an object Zeabur writes, required fields included. Any other object passes.
///
/// # Errors
///
/// What does not fit the CRD, as kube-apiserver would refuse it.
pub fn check_write(object: &Value) -> Result<(), String> {
    check(object, true)
}

fn check(object: &Value, written: bool) -> Result<(), String> {
    let api_version = text(object, "apiVersion");
    let (group, version) = api_version.split_once('/').unwrap_or(("", api_version));
    let Some((_, project)) = PROJECTS.iter().find(|(g, _)| *g == group) else {
        return Ok(());
    };
    let kind = text(object, "kind");
    let schema = crds()?
        .iter()
        .filter_map(|crd| crd.get("spec"))
        .filter(|spec| text(spec, "group") == group)
        .filter(|spec| spec.pointer("/names/kind").and_then(Value::as_str) == Some(kind))
        .flat_map(versions)
        .find(|v| {
            text(v, "name") == version && v.get("served").and_then(Value::as_bool) == Some(true)
        })
        .and_then(|v| v.pointer("/schema/openAPIV3Schema"))
        .ok_or_else(|| format!("{project} serves no {kind} {api_version}"))?;
    let mut errors = Vec::new();
    if let (Some(fields), Some(properties)) = (object.as_object(), schema.get("properties")) {
        for (name, value) in fields {
            if matches!(name.as_str(), "apiVersion" | "kind" | "metadata") {
                continue;
            }
            match properties.get(name) {
                Some(field) => walk(field, value, &format!(".{name}"), written, &mut errors),
                None => errors.push(format!(".{name}: no such field")),
            }
        }
        if written {
            missing(schema, fields, "", &mut errors);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{kind} {api_version} does not fit {project}'s CRD: {}",
            errors.join("; ")
        ))
    }
}

fn walk(schema: &Value, value: &Value, path: &str, written: bool, errors: &mut Vec<String>) {
    // kube-apiserver drops a null field of a non-nullable schema as if it were absent.
    if value.is_null()
        || flag(schema, "x-kubernetes-int-or-string")
            && (value.is_string() || value.is_i64() || value.is_u64())
    {
        return;
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.contains(value)
    {
        errors.push(format!(
            "{path}: {value} is not one of {}",
            Value::Array(allowed.clone())
        ));
    }
    let fits = match schema.get("type").and_then(Value::as_str) {
        None => true,
        Some("object") => value.is_object(),
        Some("array") => value.is_array(),
        Some("string") => value.is_string(),
        Some("integer") => value.is_i64() || value.is_u64(),
        Some("number") => value.is_number(),
        Some("boolean") => value.is_boolean(),
        Some(other) => {
            errors.push(format!("{path}: the schema has type {other}"));
            return;
        }
    };
    if !fits {
        errors.push(format!(
            "{path}: {value} is not of type {}",
            text(schema, "type")
        ));
        return;
    }
    if let Some(items) = value.as_array() {
        if let Some(item) = schema.get("items") {
            for (i, value) in items.iter().enumerate() {
                walk(item, value, &format!("{path}[{i}]"), written, errors);
            }
        }
        return;
    }
    let Some(fields) = value.as_object() else {
        return;
    };
    let properties = schema.get("properties");
    let additional = schema.get("additionalProperties");
    for (name, value) in fields {
        let path = format!("{path}.{name}");
        if let Some(field) = properties.and_then(|p| p.get(name)) {
            walk(field, value, &path, written, errors);
        } else if let Some(each) = additional.filter(|a| a.is_object()) {
            walk(each, value, &path, written, errors);
        } else if !(flag(schema, "x-kubernetes-preserve-unknown-fields")
            || additional == Some(&Value::Bool(true)))
        {
            errors.push(format!("{path}: no such field"));
        }
    }
    if written {
        missing(schema, fields, path, errors);
    }
}

fn missing(schema: &Value, fields: &Map<String, Value>, path: &str, errors: &mut Vec<String>) {
    let required = schema.get("required").and_then(Value::as_array);
    for name in required.into_iter().flatten().filter_map(Value::as_str) {
        if fields.get(name).is_none_or(Value::is_null) {
            errors.push(format!("{path}.{name}: required"));
        }
    }
}

fn versions(spec: &Value) -> impl Iterator<Item = &Value> {
    spec.get("versions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn flag(schema: &Value, key: &str) -> bool {
    schema.get(key).and_then(Value::as_bool) == Some(true)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{check_read, check_write, served};

    #[test]
    fn every_contract_crd_is_served() {
        let kinds = |group| {
            served(group)
                .unwrap()
                .into_iter()
                .map(|s| s.kind)
                .collect::<Vec<_>>()
        };
        for kind in ["User", "Credential", "Invitation", "Grant", "OAuthClient"] {
            assert!(
                kinds("identity.zeabur.com").contains(&kind.to_owned()),
                "{kind}"
            );
        }
        for kind in ["Build", "BuildRun"] {
            assert!(
                kinds("builder.zeabur.com").contains(&kind.to_owned()),
                "{kind}"
            );
        }
    }

    fn build(spec: &serde_json::Value) -> serde_json::Value {
        json!({"apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
               "metadata": {"name": "web", "namespace": "shop"}, "spec": spec})
    }

    #[test]
    fn a_field_the_crd_lacks_is_refused() {
        let error = check_read(&build(
            &json!({"source": {"git": {"contextDirectory": "a"}}}),
        ))
        .unwrap_err();
        assert!(
            error.contains(".spec.source.git.contextDirectory: no such field"),
            "{error}"
        );
        assert!(error.contains("Zeabur Builder"), "{error}");
    }

    #[test]
    fn a_value_of_another_type_is_refused() {
        let error = check_read(&build(&json!({"env": {"A": "1"}}))).unwrap_err();
        assert!(error.contains(".spec.env"), "{error}");
    }

    #[test]
    fn a_kind_neither_project_serves_is_refused() {
        let error = check_read(&json!({"apiVersion": "identity.zeabur.com/v1", "kind": "User"}))
            .unwrap_err();
        assert!(error.contains("Zeabur Identity serves no User"), "{error}");
    }

    #[test]
    fn a_written_object_needs_its_required_fields_and_a_read_one_does_not() {
        let user = json!({"apiVersion": "identity.zeabur.com/v1alpha1", "kind": "User",
                          "metadata": {"name": "bob"}, "spec": {"groups": []}});
        assert_eq!(check_read(&user), Ok(()));
        let error = check_write(&user).unwrap_err();
        assert!(error.contains(".spec.username: required"), "{error}");
    }

    #[test]
    fn other_groups_pass() {
        assert_eq!(
            check_write(&json!({"apiVersion": "v1", "kind": "Secret", "x": 1})),
            Ok(())
        );
    }
}
