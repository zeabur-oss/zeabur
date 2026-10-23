//! cert-manager's `ClusterIssuers` as Zeabur shows them, and the DNS-01 issuer Zeabur writes.
//!
//! # What Zeabur writes
//!
//! - A `ClusterIssuer` for Let's Encrypt (or its staging server) with one DNS-01 solver
//!   [cm-acme-dns01]. Let's Encrypt issues a wildcard certificate only through DNS-01
//!   [le-challenges]. Its ACME account key goes in the Secret `<issuer>-account-key`.
//! - The provider's token or key in the Secret `<issuer>-dns-credentials`, in cert-manager's
//!   cluster resource namespace: a `ClusterIssuer` reads Secrets only there [cm-cluster-issuer].
//!   The chart sets it with `--cluster-resource-namespace`, `$(POD_NAMESPACE)` by default
//!   [cm-chart].
//!
//! Each provider's solver and Secret key follow cert-manager's own page for it [cm-providers].
//!
//! [cm-acme-dns01]: https://cert-manager.io/docs/configuration/acme/dns01/
//! [le-challenges]: https://letsencrypt.org/docs/challenge-types/#dns-01-challenge
//! [cm-cluster-issuer]: https://cert-manager.io/docs/configuration/issuers/#cluster-resource-namespace
//! [cm-chart]: https://artifacthub.io/packages/helm/cert-manager/cert-manager#clusterresourcenamespace
//! [cm-providers]: https://cert-manager.io/docs/configuration/acme/dns01/#supported-dns01-providers

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
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

/// The namespace and Deployment of cert-manager's controller, as the catalog installs it.
pub const NAMESPACE: &str = "cert-manager";
/// See [`NAMESPACE`].
pub const CONTROLLER: &str = "cert-manager";

const PRODUCTION: &str = "https://acme-v02.api.letsencrypt.org/directory";
const STAGING: &str = "https://acme-staging-v02.api.letsencrypt.org/directory";

/// A DNS provider's settings for a DNS-01 solver. A blank secret field keeps the Secret's value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(
    tag = "provider",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Dns01Provider {
    /// Cloudflare, with an API token that may edit the zone's DNS.
    Cloudflare {
        /// The API token.
        #[serde(default)]
        api_token: String,
    },
    /// AWS Route 53, with an access key.
    Route53 {
        /// The region.
        region: String,
        /// The access key ID.
        access_key_id: String,
        /// The secret access key.
        #[serde(default)]
        secret_access_key: String,
        /// The hosted zone; empty to let cert-manager find it.
        #[serde(default)]
        hosted_zone_id: String,
    },
    /// Google Cloud DNS, with a service account key.
    CloudDns {
        /// The Google Cloud project.
        project: String,
        /// The service account's JSON key.
        #[serde(default)]
        service_account_json: String,
    },
    /// `DigitalOcean`, with an API token.
    Digitalocean {
        /// The API token.
        #[serde(default)]
        token: String,
    },
    /// Azure DNS, with a service principal.
    AzureDns {
        /// The subscription.
        subscription_id: String,
        /// The tenant.
        tenant_id: String,
        /// The service principal's client ID.
        client_id: String,
        /// Its client secret.
        #[serde(default)]
        client_secret: String,
        /// The resource group of the zone.
        resource_group_name: String,
        /// The zone.
        hosted_zone_name: String,
    },
}

impl Dns01Provider {
    /// The key of the secret value in the credentials Secret.
    #[must_use]
    pub const fn secret_key(&self) -> &'static str {
        match self {
            Self::Cloudflare { .. } => "api-token",
            Self::Route53 { .. } => "secret-access-key",
            Self::CloudDns { .. } => "key.json",
            Self::Digitalocean { .. } => "access-token",
            Self::AzureDns { .. } => "client-secret",
        }
    }

    /// The secret value; empty to keep the Secret's.
    #[must_use]
    pub fn secret(&self) -> &str {
        match self {
            Self::Cloudflare { api_token: s }
            | Self::Route53 {
                secret_access_key: s,
                ..
            }
            | Self::CloudDns {
                service_account_json: s,
                ..
            }
            | Self::Digitalocean { token: s }
            | Self::AzureDns {
                client_secret: s, ..
            } => s.trim(),
        }
    }

    /// The fields other than the secret that must not be empty.
    #[must_use]
    pub fn missing(&self) -> Vec<&'static str> {
        let fields: Vec<(&'static str, &str)> = match self {
            Self::Cloudflare { .. } | Self::Digitalocean { .. } => Vec::new(),
            Self::Route53 {
                region,
                access_key_id,
                ..
            } => vec![("region", region), ("accessKeyId", access_key_id)],
            Self::CloudDns { project, .. } => vec![("project", project)],
            Self::AzureDns {
                subscription_id,
                tenant_id,
                client_id,
                resource_group_name,
                hosted_zone_name,
                ..
            } => vec![
                ("subscriptionId", subscription_id),
                ("tenantId", tenant_id),
                ("clientId", client_id),
                ("resourceGroupName", resource_group_name),
                ("hostedZoneName", hosted_zone_name),
            ],
        };
        fields
            .into_iter()
            .filter(|(_, v)| v.trim().is_empty())
            .map(|(k, _)| k)
            .collect()
    }

    /// The `dns01` block of the solver; its Secret is `secret`.
    fn solver(&self, secret: &str) -> Value {
        let reference = json!({"name": secret, "key": self.secret_key()});
        match self {
            Self::Cloudflare { .. } => json!({"cloudflare": {"apiTokenSecretRef": reference}}),
            Self::Route53 {
                region,
                access_key_id,
                hosted_zone_id,
                ..
            } => {
                let mut solver = Map::new();
                solver.insert("region".to_owned(), json!(region.trim()));
                solver.insert("accessKeyID".to_owned(), json!(access_key_id.trim()));
                solver.insert("secretAccessKeySecretRef".to_owned(), reference);
                if !hosted_zone_id.trim().is_empty() {
                    solver.insert("hostedZoneID".to_owned(), json!(hosted_zone_id.trim()));
                }
                json!({"route53": solver})
            }
            Self::CloudDns { project, .. } => json!({"cloudDNS": {
                "project": project.trim(), "serviceAccountSecretRef": reference}}),
            Self::Digitalocean { .. } => json!({"digitalocean": {"tokenSecretRef": reference}}),
            Self::AzureDns {
                subscription_id,
                tenant_id,
                client_id,
                resource_group_name,
                hosted_zone_name,
                ..
            } => json!({"azureDNS": {
                "subscriptionID": subscription_id.trim(),
                "tenantID": tenant_id.trim(),
                "clientID": client_id.trim(),
                "clientSecretSecretRef": reference,
                "resourceGroupName": resource_group_name.trim(),
                "hostedZoneName": hosted_zone_name.trim(),
                "environment": "AzurePublicCloud",
            }}),
        }
    }

    /// The provider of a `dns01` block Zeabur could have written, its secret blank.
    fn read(dns01: &Value) -> Option<Self> {
        let s = |pointer: &str| {
            dns01
                .pointer(pointer)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let name = dns01.as_object()?.keys().find(|k| *k != "cnameStrategy")?;
        Some(match name.as_str() {
            "cloudflare" => Self::Cloudflare {
                api_token: String::new(),
            },
            "route53" => Self::Route53 {
                region: s("/route53/region"),
                access_key_id: s("/route53/accessKeyID"),
                secret_access_key: String::new(),
                hosted_zone_id: s("/route53/hostedZoneID"),
            },
            "cloudDNS" => Self::CloudDns {
                project: s("/cloudDNS/project"),
                service_account_json: String::new(),
            },
            "digitalocean" => Self::Digitalocean {
                token: String::new(),
            },
            "azureDNS" => Self::AzureDns {
                subscription_id: s("/azureDNS/subscriptionID"),
                tenant_id: s("/azureDNS/tenantID"),
                client_id: s("/azureDNS/clientID"),
                client_secret: String::new(),
                resource_group_name: s("/azureDNS/resourceGroupName"),
                hosted_zone_name: s("/azureDNS/hostedZoneName"),
            },
            _ => return None,
        })
    }
}

/// A Let's Encrypt `ClusterIssuer` with a DNS-01 solver, to create or change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IssuerRequest {
    /// The `ClusterIssuer`'s name.
    pub name: String,
    /// Where Let's Encrypt sends expiry notices; may be empty.
    #[serde(default)]
    pub email: String,
    /// Use Let's Encrypt's staging server, whose certificates browsers do not trust.
    #[serde(default)]
    pub staging: bool,
    /// The DNS provider.
    pub dns: Dns01Provider,
}

/// The Secret that holds the provider's token or key for issuer `issuer`.
#[must_use]
pub fn credentials_name(issuer: &str) -> String {
    format!("{issuer}-dns-credentials")
}

/// The credentials Secret in `namespace`, holding `value`.
#[must_use]
pub fn credentials_secret(namespace: &str, request: &IssuerRequest, value: &str) -> Value {
    json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {"name": credentials_name(&request.name), "namespace": namespace},
        "type": "Opaque",
        "stringData": {request.dns.secret_key(): value},
    })
}

/// The `ClusterIssuer` as it will be.
#[must_use]
pub fn issuer_object(request: &IssuerRequest) -> Value {
    let mut acme = Map::new();
    let server = if request.staging { STAGING } else { PRODUCTION };
    acme.insert("server".to_owned(), json!(server));
    if !request.email.trim().is_empty() {
        acme.insert("email".to_owned(), json!(request.email.trim()));
    }
    acme.insert(
        "privateKeySecretRef".to_owned(),
        json!({"name": format!("{}-account-key", request.name)}),
    );
    acme.insert(
        "solvers".to_owned(),
        json!([{"dns01": request.dns.solver(&credentials_name(&request.name))}]),
    );
    json!({
        "apiVersion": "cert-manager.io/v1",
        "kind": "ClusterIssuer",
        "metadata": {"name": request.name},
        "spec": {"acme": acme},
    })
}

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

/// cert-manager's cluster resource namespace from its controller Deployment's arguments.
#[must_use]
pub fn resource_namespace(deployment: &Value) -> Option<String> {
    let namespace = deployment
        .pointer("/metadata/namespace")
        .and_then(Value::as_str)?;
    let containers = deployment
        .pointer("/spec/template/spec/containers")
        .and_then(Value::as_array)?;
    let flag = containers
        .iter()
        .filter_map(|c| c.get("args").and_then(Value::as_array))
        .flatten()
        .filter_map(Value::as_str)
        .find_map(|a| a.strip_prefix("--cluster-resource-namespace="));
    match flag {
        Some("$(POD_NAMESPACE)") | None => Some(namespace.to_owned()),
        Some(other) => Some(other.to_owned()),
    }
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
    /// The ACME server; absent for a CA, Vault or self-signed issuer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
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
    /// Its email.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// The provider's settings, secret blank, where the form can change it: one DNS-01 solver
    /// whose Secret is [`credentials_name`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns: Option<Dns01Provider>,
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
    let dns = match solvers {
        [only] => only
            .get("dns01")
            .filter(|d| {
                d.as_object()
                    .into_iter()
                    .flat_map(|o| o.values())
                    .filter_map(|p| p.as_object())
                    .flat_map(|p| p.values())
                    .filter_map(|r| r.get("name").and_then(Value::as_str))
                    .any(|n| n == credentials_name(&name))
            })
            .and_then(Dns01Provider::read),
        _ => None,
    };
    IssuerView {
        staging: server.as_deref().is_some_and(|s| s.contains("staging")),
        server,
        solvers: solvers.iter().filter_map(solver_name).collect(),
        wildcards: issues_wildcards(data),
        ready,
        message: condition
            .and_then(|c| c.get("message"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        email: data
            .pointer("/spec/acme/email")
            .and_then(Value::as_str)
            .map(str::to_owned),
        dns,
        name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(dns: Dns01Provider) -> IssuerRequest {
        IssuerRequest {
            name: "letsencrypt-dns".to_owned(),
            email: String::new(),
            staging: false,
            dns,
        }
    }

    fn object(value: Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_written_issuer_reads_back_editable_with_its_secret_blank() {
        let written = request(Dns01Provider::Route53 {
            region: "us-east-1".to_owned(),
            access_key_id: "AKIA".to_owned(),
            secret_access_key: "s3cret".to_owned(),
            hosted_zone_id: String::new(),
        });
        let mut issuer_value = issuer_object(&written);
        issuer_value["status"] =
            json!({"conditions": [{"type": "Ready", "status": "True", "message": "registered"}]});
        let view = issuer(&object(issuer_value.clone()));
        assert_eq!(view.solvers, ["dns01:route53"]);
        assert!(view.wildcards);
        assert_eq!(view.ready, Readiness::Ready);
        assert!(!issuer_value.to_string().contains("s3cret"));
        assert_eq!(
            view.dns,
            Some(Dns01Provider::Route53 {
                region: "us-east-1".to_owned(),
                access_key_id: "AKIA".to_owned(),
                secret_access_key: String::new(),
                hosted_zone_id: String::new(),
            })
        );
    }

    #[test]
    fn the_charts_http01_issuer_is_not_editable() {
        let view = issuer(&object(json!({
            "apiVersion": "cert-manager.io/v1", "kind": "ClusterIssuer",
            "metadata": {"name": "letsencrypt"},
            "spec": {"acme": {"server": PRODUCTION,
                "solvers": [{"http01": {"ingress": {"ingressClassName": "traefik"}}}]}}
        })));
        assert_eq!(view.solvers, ["http01"]);
        assert!(!view.wildcards);
        assert_eq!(view.ready, Readiness::Unknown);
        assert_eq!(view.dns, None);
    }

    #[test]
    fn the_resource_namespace_follows_the_flag() {
        let deployment = |args: Value| {
            json!({"metadata": {"namespace": "cert-manager"},
                   "spec": {"template": {"spec": {"containers": [{"args": args}]}}}})
        };
        let default = deployment(json!(["--cluster-resource-namespace=$(POD_NAMESPACE)"]));
        let other = deployment(json!(["--v=2", "--cluster-resource-namespace=certs"]));
        assert_eq!(
            resource_namespace(&default).as_deref(),
            Some("cert-manager")
        );
        assert_eq!(resource_namespace(&other).as_deref(), Some("certs"));
    }
}
