//! A value shown masked is never written.
//!
//! A person sees a Secret's values masked: the YAML view and a plan show [`REDACTED`], the
//! interface a run of `•`. A mask sent back, as it is or base64-encoded under a Secret's `data`,
//! would replace the value it stands for. The planner refuses any write that carries one, so the
//! rule holds whatever asked for the plan.

use base64::Engine as _;
use serde_json::Value;

use crate::change::{Target, Write};
use crate::plan::{ErrorKind, PlanError, error};
use crate::rules::REDACTED;

/// Whether `value` is a mask a person was shown: [`REDACTED`] or a run of `•` and nothing else.
pub(crate) fn is_mask(value: &str) -> bool {
    value == REDACTED || (!value.is_empty() && value.chars().all(|c| c == '\u{2022}'))
}

/// Refuses `write` when a value it puts in `target` is a mask.
pub(crate) fn refuse_masks(target: &Target, write: &Write) -> Result<(), PlanError> {
    let secret = target.api_version == "v1" && target.kind == "Secret";
    let found = match write {
        Write::Create { body } => {
            let data = if secret { body.get("data") } else { None };
            in_data(data, "/data").or_else(|| in_strings(body, String::new()))
        }
        Write::Patch { operations } | Write::EphemeralContainers { operations } => {
            in_operations(operations, secret)
        }
        Write::Delete { .. } => None,
    };
    let Some(path) = found else {
        return Ok(());
    };
    Err(error(
        ErrorKind::BadRequest,
        "masked_value",
        format!(
            "{} {}: the value at {path} is a mask shown in place of a secret value; type the \
             value itself, or leave the key as it was shown to keep it.",
            target.kind, target.name
        ),
    ))
}

/// The path of the first mask the patch's `add` and `replace` operations write.
fn in_operations(operations: &[Value], secret: bool) -> Option<String> {
    for operation in operations {
        let op = operation.get("op").and_then(Value::as_str);
        if !matches!(op, Some("add" | "replace")) {
            continue;
        }
        let path = operation
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(value) = operation.get("value") else {
            continue;
        };
        if secret {
            let found = if path == "/data" {
                in_data(Some(value), path)
            } else if path.starts_with("/data/") {
                encoded_mask(value).then(|| path.to_owned())
            } else {
                None
            };
            if found.is_some() {
                return found;
            }
        }
        if let Some(found) = in_strings(value, path.to_owned()) {
            return Some(found);
        }
    }
    None
}

/// The path of the first value of a Secret's `data` that decodes to a mask.
fn in_data(data: Option<&Value>, path: &str) -> Option<String> {
    let map = data?.as_object()?;
    for (key, value) in map {
        if encoded_mask(value) {
            return Some(format!("{path}/{key}"));
        }
    }
    None
}

fn encoded_mask(value: &Value) -> bool {
    let Some(encoded) = value.as_str() else {
        return false;
    };
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
        return false;
    };
    String::from_utf8(bytes).is_ok_and(|text| is_mask(&text))
}

/// The path of the first string in `value` that is a mask.
fn in_strings(value: &Value, path: String) -> Option<String> {
    match value {
        Value::String(text) => is_mask(text).then_some(path),
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                if let Some(found) = in_strings(item, format!("{path}/{index}")) {
                    return Some(found);
                }
            }
            None
        }
        Value::Object(map) => {
            for (key, item) in map {
                if let Some(found) = in_strings(item, format!("{path}/{key}")) {
                    return Some(found);
                }
            }
            None
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => None,
    }
}
