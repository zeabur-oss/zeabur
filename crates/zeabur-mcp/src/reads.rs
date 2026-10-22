//! The reads the tools may make: `GET`s of the API, from one allowlist.
//!
//! A tool names a path template from [`ALLOWLIST`] and the values for its parameters; nothing
//! else reaches the [`Reader`](crate::Reader). Parameter values come from the model, so each is
//! checked to be one path segment of the characters Kubernetes names, kinds, groups and versions
//! use [k8s-names]: a value cannot add a segment (`/`), climb out of one (`..`) or start a query.
//!
//! [k8s-names]: https://kubernetes.io/docs/concepts/overview/working-with-objects/names/

use serde_json::Value;
use zeabur_core::problem::{Kind, Problem};

/// The cluster: its Kubernetes version and distribution.
pub(crate) const CLUSTER: &str = "/api/v1/cluster";
/// End of life, version skew and newer versions.
pub(crate) const UPGRADES: &str = "/api/v1/cluster/upgrades";
/// What the cluster itself is missing.
pub(crate) const CLUSTER_GAPS: &str = "/api/v1/cluster/gaps";
/// What is installed, and what to install.
pub(crate) const CAPABILITIES: &str = "/api/v1/cluster/capabilities";
/// The projects, with their health.
pub(crate) const PROJECTS: &str = "/api/v1/projects";
/// One project document.
pub(crate) const PROJECT: &str = "/api/v1/projects/{project}";
/// One service of a project.
pub(crate) const SERVICE: &str = "/api/v1/projects/{project}/services/{kind}/{name}";
/// A project's events.
pub(crate) const EVENTS: &str = "/api/v1/projects/{project}/events";
/// A log snapshot (not the `follow` stream).
pub(crate) const LOGS: &str = "/api/v1/projects/{project}/pods/{pod}/logs";
/// One object as YAML, with Secret values hidden by the API.
pub(crate) const OBJECT: &str =
    "/api/v1/projects/{project}/objects/{group}/{version}/{resource}/{name}";
/// What a project's pods use now.
pub(crate) const USAGE: &str = "/api/v1/projects/{project}/usage";
/// Falco's recent alerts about a project's pods.
pub(crate) const ALERTS: &str = "/api/v1/projects/{project}/alerts";
/// What a service's pods used over a range.
pub(crate) const HISTORY: &str = "/api/v1/projects/{project}/services/{kind}/{name}/history";

/// Every API path a tool may read, all with `GET`. None of them reveals a Secret's values,
/// plans or applies a change, or opens a terminal.
pub const ALLOWLIST: &[&str] = &[
    CLUSTER,
    UPGRADES,
    CLUSTER_GAPS,
    CAPABILITIES,
    PROJECTS,
    PROJECT,
    SERVICE,
    EVENTS,
    LOGS,
    OBJECT,
    USAGE,
    HISTORY,
    ALERTS,
];

/// The longest value accepted for a parameter: a DNS subdomain name, the longest Kubernetes
/// name [k8s-names].
///
/// [k8s-names]: https://kubernetes.io/docs/concepts/overview/working-with-objects/names/
const MAX_SEGMENT: usize = 253;

/// The path and query for `template` with `values` in place of its parameters, in order.
///
/// Fails with a `bad-request` problem when a value is not one plain segment, and with an
/// `internal` one when `template` is not in [`ALLOWLIST`] or the values do not fit it.
pub(crate) fn path_and_query(
    template: &'static str,
    values: &[&str],
    query: &[(&str, String)],
) -> Result<String, Value> {
    if !ALLOWLIST.contains(&template) {
        return Err(problem(
            Kind::Internal,
            format!("{template} is not a read MCP may make."),
        ));
    }
    let mut values = values.iter();
    let mut path = Vec::new();
    for piece in template.split('/') {
        match piece.strip_prefix('{').and_then(|p| p.strip_suffix('}')) {
            Some(name) => {
                let value = values.next().ok_or_else(|| {
                    problem(
                        Kind::Internal,
                        format!("{template} needs a value for `{name}`."),
                    )
                })?;
                path.push(segment(name, value)?);
            }
            None => path.push(piece),
        }
    }
    if values.next().is_some() {
        return Err(problem(
            Kind::Internal,
            format!("{template} has fewer parameters."),
        ));
    }
    let mut path = path.join("/");
    if !query.is_empty() {
        path.push('?');
        path.push_str(
            &form_urlencoded::Serializer::new(String::new())
                .extend_pairs(query)
                .finish(),
        );
    }
    Ok(path)
}

/// `value`, when it is one segment of letters, digits, `-` and `.`, other than `.` and `..`.
fn segment<'a>(name: &str, value: &'a str) -> Result<&'a str, Value> {
    let plain = !value.is_empty()
        && value.len() <= MAX_SEGMENT
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.');
    if plain {
        Ok(value)
    } else {
        Err(problem(
            Kind::BadRequest,
            format!(
                "`{name}` must be a Kubernetes name (letters, digits, `-` and `.`), not {value:?}."
            ),
        ))
    }
}

/// A problem of Zeabur's own, in the shape the API answers with.
pub(crate) fn problem(kind: Kind, detail: String) -> Value {
    serde_json::to_value(Problem::new(kind, detail)).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_fill_the_template_and_the_query_is_encoded() {
        assert_eq!(
            path_and_query(LOGS, &["shop", "web-1"], &[("tailLines", "20".to_owned())]).unwrap(),
            "/api/v1/projects/shop/pods/web-1/logs?tailLines=20"
        );
        assert_eq!(
            path_and_query(
                OBJECT,
                &["shop", "cert-manager.io", "v1", "certificates", "tls"],
                &[]
            )
            .unwrap(),
            "/api/v1/projects/shop/objects/cert-manager.io/v1/certificates/tls"
        );
    }

    /// A value from the model cannot reach a path outside the allowlist: not another segment,
    /// not a parent, not a query of its own.
    #[test]
    fn a_value_is_one_plain_segment() {
        for value in [
            "shop/secrets/db/reveal",
            "..",
            ".",
            "",
            "shop?x=1",
            "shop%2Fsecrets",
            "shop#x",
            "sh op",
            &"a".repeat(254),
        ] {
            let error = path_and_query(PROJECT, &[value], &[]).unwrap_err();
            assert_eq!(error["kind"], "bad-request", "{value:?}");
        }
    }

    #[test]
    fn a_path_outside_the_allowlist_is_refused() {
        let error = path_and_query("/api/v1/projects/{project}/plan", &["shop"], &[]).unwrap_err();
        assert_eq!(error["kind"], "internal");
    }
}
