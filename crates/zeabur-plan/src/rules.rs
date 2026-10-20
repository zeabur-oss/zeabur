//! The small rules a plan is made of.
//!
//! Each is one fixture operation (`fixtures/mutation`): input checks, route names, the
//! managed-object refusal codes, the diff a person confirms, and where a workload keeps its pod
//! template.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use zeabur_adapters::provenance::Tool;

/// A refusal, as a stable `snake_case` code (the fixtures' `error`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Refusal(pub String);

impl Refusal {
    fn of(code: &str) -> Self {
        Self(code.to_owned())
    }
}

// ── Environment variables ────────────────────────────────────────────────────────────────────

/// One requested change to a container's environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnvEdit {
    /// `set` or `remove`.
    pub op: String,
    /// The variable.
    pub name: String,
    /// The literal value, for `set`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Where the value is read from instead, for `set`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_from: Option<EnvValueFrom>,
}

/// A value read from a Secret or ConfigMap key, or a pod field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnvValueFrom {
    /// The Secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_name: Option<String>,
    /// The ConfigMap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_map_name: Option<String>,
    /// The key in that object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// A pod field (the downward API).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_path: Option<String>,
}

/// Checks a list of environment changes (`mutation.validateEnv`).
///
/// Names must be C identifiers, the portable form POSIX defines for environment variable names
/// [posix-env]; Kubernetes accepts more since 1.34, but a shell or a `.env` loader may not.
///
/// [posix-env]: https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap08.html
///
/// # Errors
///
/// `no_changes`, `invalid_env_op`, `invalid_env_name`, `duplicate_env_name`,
/// `ambiguous_env_source`, `missing_env_source_key`.
pub fn validate_env(changes: &[EnvEdit]) -> Result<(), Refusal> {
    if changes.is_empty() {
        return Err(Refusal::of("no_changes"));
    }
    let mut seen = BTreeSet::new();
    for change in changes {
        if !matches!(change.op.as_str(), "set" | "remove") {
            return Err(Refusal::of("invalid_env_op"));
        }
        if !is_c_identifier(&change.name) {
            return Err(Refusal::of("invalid_env_name"));
        }
        if !seen.insert(change.name.as_str()) {
            return Err(Refusal::of("duplicate_env_name"));
        }
        if let Some(from) = &change.value_from {
            if from.secret_name.is_some() && from.config_map_name.is_some() {
                return Err(Refusal::of("ambiguous_env_source"));
            }
            if (from.secret_name.is_some() || from.config_map_name.is_some()) && from.key.is_none()
            {
                return Err(Refusal::of("missing_env_source_key"));
            }
        }
    }
    Ok(())
}

fn is_c_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// The `env` entry for a `set`.
#[must_use]
pub fn env_entry(change: &EnvEdit) -> Value {
    let mut entry = Map::new();
    entry.insert("name".to_owned(), Value::String(change.name.clone()));
    match &change.value_from {
        Some(from) => {
            let source = match (&from.secret_name, &from.config_map_name) {
                (Some(name), _) => {
                    serde_json::json!({"secretKeyRef": {"name": name, "key": from.key}})
                }
                (None, Some(name)) => {
                    serde_json::json!({"configMapKeyRef": {"name": name, "key": from.key}})
                }
                (None, None) => serde_json::json!({"fieldRef": {"fieldPath": from.field_path}}),
            };
            entry.insert("valueFrom".to_owned(), source);
        }
        None => {
            entry.insert(
                "value".to_owned(),
                Value::String(change.value.clone().unwrap_or_default()),
            );
        }
    }
    Value::Object(entry)
}

// ── Hostnames and routes ─────────────────────────────────────────────────────────────────────

/// Checks a hostname (`mutation.validateHostname`).
///
/// DNS labels of letters, digits and inner hyphens, at most 63 characters each and 253 in all,
/// optionally behind one `*.` wildcard: the form Ingress and Gateway API hostnames take
/// [rfc1123] [gateway-hostname].
///
/// [rfc1123]: https://www.rfc-editor.org/rfc/rfc1123#section-2
/// [gateway-hostname]: https://gateway-api.sigs.k8s.io/reference/spec/#hostname
///
/// # Errors
///
/// `hostname_required`, `invalid_hostname`.
pub fn validate_hostname(hostname: &str) -> Result<(), Refusal> {
    if hostname.is_empty() {
        return Err(Refusal::of("hostname_required"));
    }
    let name = hostname.strip_prefix("*.").unwrap_or(hostname);
    let valid = name.len() <= 253
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        });
    if valid {
        Ok(())
    } else {
        Err(Refusal::of("invalid_hostname"))
    }
}

/// The name of the route Zeabur creates for `hostname` on `workload` (`mutation.routeName`).
///
/// The workload, then the hostname in lower case with dots as hyphens, so asking again for the
/// same hostname finds the same route.
#[must_use]
pub fn route_name(workload: &str, hostname: &str) -> String {
    let host = hostname
        .to_ascii_lowercase()
        .replace("*.", "wildcard.")
        .replace('.', "-");
    let mut name = format!("{workload}-{host}");
    name.truncate(253);
    name
}

// ── Managed objects ──────────────────────────────────────────────────────────────────────────

/// The display name of a tool.
#[must_use]
pub const fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Helm => "Helm",
        Tool::ArgoCd => "Argo CD",
        Tool::Flux => "Flux",
    }
}

/// The refusal code of an edit to an object `tool` manages.
#[must_use]
pub const fn managed_code(tool: Tool) -> &'static str {
    match tool {
        Tool::Helm => "managed_by_helm",
        Tool::ArgoCd => "managed_by_argo_cd",
        Tool::Flux => "managed_by_flux",
    }
}

// ── Diff ─────────────────────────────────────────────────────────────────────────────────────

/// What a person sees instead of a Secret's value: the same marker as the YAML view.
pub const REDACTED: &str = zeabur_graph::manifest::HIDDEN;

/// One changed field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct FieldChange {
    /// The field, dotted; keyed list entries as `[name=app]`, others by position.
    pub path: String,
    /// The value before, absent when the field is new.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// The value after, absent when the field is removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

/// The fields that differ between `before` and `after` (`mutation.diff`), sorted by path.
///
/// Fields the server rewrites on every write and `status` are left out; a Secret's `data` and
/// `stringData` values are compared as they are but shown as [`REDACTED`].
#[must_use]
pub fn diff(before: Option<&Value>, after: &Value) -> Vec<FieldChange> {
    let secret = after.get("kind").and_then(Value::as_str) == Some("Secret");
    let mut old = Vec::new();
    if let Some(before) = before {
        flatten("", before, &mut old);
    }
    let mut new = Vec::new();
    flatten("", after, &mut new);
    let paths: BTreeSet<&String> = old.iter().chain(&new).map(|(path, _)| path).collect();
    let find = |list: &[(String, String)], path: &str| {
        list.iter().find(|(p, _)| p == path).map(|(_, v)| v.clone())
    };
    let shown = |path: &str, value: Option<String>| {
        if secret && is_secret_value(path) {
            value.map(|_| REDACTED.to_owned())
        } else {
            value
        }
    };
    paths
        .into_iter()
        .filter_map(|path| {
            let (before, after) = (find(&old, path), find(&new, path));
            (before != after).then(|| FieldChange {
                path: path.clone(),
                before: shown(path, before),
                after: shown(path, after),
            })
        })
        .collect()
}

fn is_secret_value(path: &str) -> bool {
    path.starts_with("data.") || path.starts_with("stringData.")
}

/// Server-maintained fields that change on every write.
fn is_noise(path: &str) -> bool {
    path == "status"
        || path.starts_with("status.")
        || [
            "metadata.resourceVersion",
            "metadata.generation",
            "metadata.managedFields",
            "metadata.uid",
            "metadata.creationTimestamp",
            "metadata.selfLink",
        ]
        .iter()
        .any(|noise| {
            path == *noise
                || path.starts_with(&format!("{noise}."))
                || path.starts_with(&format!("{noise}["))
        })
}

fn flatten(path: &str, value: &Value, out: &mut Vec<(String, String)>) {
    if is_noise(path) {
        return;
    }
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (name, child) in map {
                let child_path = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{path}.{name}")
                };
                flatten(&child_path, child, out);
            }
        }
        Value::Array(items) if !items.is_empty() => {
            for (index, item) in items.iter().enumerate() {
                let key = ["name", "mountPath", "containerPort", "devicePath"]
                    .iter()
                    .find_map(|field| item.get(*field).map(|v| format!("[{field}={}]", scalar(v))))
                    .unwrap_or_else(|| format!("[{index}]"));
                flatten(&format!("{path}{key}"), item, out);
            }
        }
        leaf @ (Value::Null
        | Value::Bool(_)
        | Value::Number(_)
        | Value::String(_)
        | Value::Object(_)
        | Value::Array(_)) => out.push((path.to_owned(), scalar(leaf))),
    }
}

fn scalar(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other @ (Value::Null
        | Value::Bool(_)
        | Value::Number(_)
        | Value::Object(_)
        | Value::Array(_)) => other.to_string(),
    }
}
