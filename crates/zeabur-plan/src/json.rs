//! JSON helpers: RFC 6901 pointers, the subset of RFC 6902 JSON Patch Zeabur sends, and the
//! key fields of the lists Kubernetes keys by a field.
//!
//! The patch is applied locally to show the object as it will be after the write and to
//! re-project it; kube-apiserver applies the same operations to the stored object.
//!
//! # References
//!
//! - [rfc6901] P. Bryan, K. Zyp, M. Nottingham, "JavaScript Object Notation (JSON) Pointer,"
//!   RFC 6901, 2013. <https://www.rfc-editor.org/rfc/rfc6901>
//! - [rfc6902] P. Bryan, M. Nottingham, "JavaScript Object Notation (JSON) Patch," RFC 6902, 2013. <https://www.rfc-editor.org/rfc/rfc6902>
//! - [k8s-ssa-merge] The Kubernetes Authors, "Merge strategy," in "Server-Side Apply,"
//!   Kubernetes Documentation.
//!   <https://kubernetes.io/docs/reference/using-api/server-side-apply/#merge-strategy>
//!
//! [rfc6901]: https://www.rfc-editor.org/rfc/rfc6901
//! [rfc6902]: https://www.rfc-editor.org/rfc/rfc6902
//! [k8s-ssa-merge]: https://kubernetes.io/docs/reference/using-api/server-side-apply/#merge-strategy

use serde_json::{Value, json};

/// One pointer token, escaped (`~` → `~0`, `/` → `~1`) [rfc6901].
///
/// [rfc6901]: https://www.rfc-editor.org/rfc/rfc6901
#[must_use]
pub fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// A pointer from tokens.
#[must_use]
pub fn pointer(tokens: &[&str]) -> String {
    tokens.iter().fold(String::new(), |mut path, token| {
        path.push('/');
        path.push_str(&escape(token));
        path
    })
}

/// `{"op": "test", "path": …, "value": …}`.
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "callers build the value with `json!` in place"
)]
pub fn test(path: &str, value: Value) -> Value {
    json!({"op": "test", "path": path, "value": value})
}

/// `{"op": "add", "path": …, "value": …}`.
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "callers build the value with `json!` in place"
)]
pub fn add(path: &str, value: Value) -> Value {
    json!({"op": "add", "path": path, "value": value})
}

/// `{"op": "replace", "path": …, "value": …}`.
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "callers build the value with `json!` in place"
)]
pub fn replace(path: &str, value: Value) -> Value {
    json!({"op": "replace", "path": path, "value": value})
}

/// `{"op": "remove", "path": …}`.
#[must_use]
pub fn remove(path: &str) -> Value {
    json!({"op": "remove", "path": path})
}

/// Applied here to show the object as it will be; shared with the in-memory cluster, which
/// applies it to store the object.
pub use zeabur_kernel::json_patch::{PatchError, apply_patch};

/// The key field of a list Kubernetes keys by a field (its merge key), by the list's name.
///
/// For the lists of a pod template: containers, env, volumes and the like by `name`, mounts by
/// `mountPath`, ports by `containerPort`, devices by `devicePath` [k8s-ssa-merge]. `None` for
/// lists replaced whole.
///
/// [k8s-ssa-merge]: https://kubernetes.io/docs/reference/using-api/server-side-apply/#merge-strategy
#[must_use]
pub fn list_key(field: &str) -> Option<&'static str> {
    match field {
        "containers"
        | "initContainers"
        | "ephemeralContainers"
        | "env"
        | "volumes"
        | "imagePullSecrets"
        | "resourceClaims" => Some("name"),
        "volumeMounts" => Some("mountPath"),
        "volumeDevices" => Some("devicePath"),
        "ports" => Some("containerPort"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointers_escape_slashes_and_tildes() {
        assert_eq!(
            pointer(&[
                "metadata",
                "annotations",
                "kubectl.kubernetes.io/restartedAt"
            ]),
            "/metadata/annotations/kubectl.kubernetes.io~1restartedAt"
        );
        assert_eq!(escape("a~b"), "a~0b");
    }
}
