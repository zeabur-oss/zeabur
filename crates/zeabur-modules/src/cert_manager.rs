//! cert-manager's `ClusterIssuers` as Zeabur lists them. Zeabur reads issuers and does not write
//! them: a DNS-01 issuer is made with `kubectl` (`docs/zh-TW/configure/domains.mdx`).

use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_kube::DynamicObject;

use crate::Read;

/// `cert-manager.io/v1` `ClusterIssuer`.
pub const CLUSTER_ISSUERS: Read = Read {
    group: "cert-manager.io",
    version: "v1",
    resource: "clusterissuers",
    kind: "ClusterIssuer",
};

/// Whether an issuer can issue a wildcard certificate: an ACME one needs a DNS-01 solver; a CA,
/// Vault or self-signed one can.
#[must_use]
pub fn issues_wildcards(issuer: &Value) -> bool {
    let Some(acme) = issuer.pointer("/spec/acme") else {
        return true;
    };
    acme.get("solvers")
        .and_then(Value::as_array)
        .is_some_and(|s| s.iter().any(|s| s.get("dns01").is_some()))
}

/// Whether an issuer is ready, from its `Ready` condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum Readiness {
    /// `Ready` is `True`.
    Ready,
    /// `Ready` is `False`.
    NotReady,
    /// No `Ready` condition yet.
    Unknown,
}

/// One `ClusterIssuer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IssuerView {
    /// Its name.
    pub name: String,
    /// It uses Let's Encrypt's staging server.
    pub staging: bool,
    /// Its challenge solvers: `http01`, or `dns01` with the provider, as `dns01:cloudflare`.
    pub solvers: Vec<String>,
    /// It can issue wildcard certificates.
    pub wildcards: bool,
    /// Its `Ready` condition.
    pub ready: Readiness,
    /// The condition's message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

fn solver_name(solver: &Value) -> Option<String> {
    if solver.get("http01").is_some() {
        return Some("http01".to_owned());
    }
    let dns01 = solver.get("dns01")?.as_object()?;
    let provider = dns01.keys().find(|k| *k != "cnameStrategy")?;
    Some(format!("dns01:{provider}"))
}

/// One `ClusterIssuer` as Zeabur shows it.
#[must_use]
pub fn issuer(object: &DynamicObject) -> IssuerView {
    let data = &object.data;
    let name = object.metadata.name.clone().unwrap_or_default();
    let server = data
        .pointer("/spec/acme/server")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let solvers: &[Value] = data
        .pointer("/spec/acme/solvers")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice);
    let condition = data
        .pointer("/status/conditions")
        .and_then(Value::as_array)
        .and_then(|c| {
            c.iter()
                .find(|c| c.get("type").and_then(Value::as_str) == Some("Ready"))
        });
    let ready = match condition
        .and_then(|c| c.get("status"))
        .and_then(Value::as_str)
    {
        Some("True") => Readiness::Ready,
        Some("False") => Readiness::NotReady,
        _ => Readiness::Unknown,
    };
    IssuerView {
        staging: server.as_deref().is_some_and(|s| s.contains("staging")),
        solvers: solvers.iter().filter_map(solver_name).collect(),
        wildcards: issues_wildcards(data),
        ready,
        message: condition
            .and_then(|c| c.get("message"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        name,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn object(value: Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_dns01_issuer_issues_wildcards_and_shows_its_readiness() {
        let view = issuer(&object(json!({
            "apiVersion": "cert-manager.io/v1", "kind": "ClusterIssuer",
            "metadata": {"name": "letsencrypt-dns"},
            "spec": {"acme": {"server": "https://acme-v02.api.letsencrypt.org/directory",
                "solvers": [{"dns01": {"cloudflare": {"apiTokenSecretRef": {"name": "cloudflare-dns"}}}}]}},
            "status": {"conditions": [{"type": "Ready", "status": "True", "message": "registered"}]}
        })));
        assert_eq!(view.solvers, ["dns01:cloudflare"]);
        assert!(view.wildcards);
        assert!(!view.staging);
        assert_eq!(view.ready, Readiness::Ready);
    }

    #[test]
    fn the_charts_http01_issuer_cannot_issue_wildcards() {
        let view = issuer(&object(json!({
            "apiVersion": "cert-manager.io/v1", "kind": "ClusterIssuer",
            "metadata": {"name": "letsencrypt"},
            "spec": {"acme": {"server": "https://acme-staging-v02.api.letsencrypt.org/directory",
                "solvers": [{"http01": {"ingress": {"ingressClassName": "traefik"}}}]}}
        })));
        assert_eq!(view.solvers, ["http01"]);
        assert!(view.staging);
        assert!(!view.wildcards);
        assert_eq!(view.ready, Readiness::Unknown);
    }
}
