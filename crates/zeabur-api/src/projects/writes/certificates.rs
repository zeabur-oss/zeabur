//! What a plan with a certificate choice reads: the `ClusterIssuers` it names, the person's own
//! certificates, checked, and the cert-manager Certificate an own certificate replaces.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::Path;
use rustls::client::verify_server_name;
use rustls::crypto::aws_lc_rs::sign::any_supported_type;
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::server::ParsedCertificate;
use rustls::sign::CertifiedKey;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem, ProblemAt};
use zeabur_graph::Snapshot;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ClusterError, ClusterExt as _, ResourceRef};
use zeabur_modules::cert_manager;
use zeabur_plan::{
    AddDomainChange, CertificateChange, CertificateSource, Change, CheckedCertificate, Fetched,
    IssuerRead, PlanRequest, RouteKind,
};

use crate::error::ApiError;
use crate::handlers::Person;

const CLUSTER_ISSUER: &str = "cert-manager.io/cluster-issuer";

/// The hostname and certificate choice of each change that has one.
fn choices(
    request: &PlanRequest,
) -> impl Iterator<Item = (usize, &str, Option<&CertificateSource>)> {
    request
        .changes
        .iter()
        .enumerate()
        .filter_map(|(at, change)| {
            if let Change::AddDomain(AddDomainChange {
                hostname,
                certificate,
                ..
            }) = change
            {
                Some((at, hostname.as_str(), certificate.as_ref()))
            } else if let Change::Certificate(CertificateChange {
                hostname,
                certificate,
                ..
            }) = change
            {
                Some((at, hostname.as_str(), Some(certificate)))
            } else {
                None
            }
        })
}

/// Reads, as the person, each `ClusterIssuer` the request chooses, and for a wildcard hostname
/// with no choice the issuers that would serve it: the default and those the Ingresses name.
pub(super) async fn issuers(
    cluster: &dyn Cluster,
    request: &PlanRequest,
    snapshot: &Snapshot,
    default: Option<&str>,
) -> BTreeMap<String, IssuerRead> {
    let mut names: Vec<&str> = Vec::new();
    let mut implicit = false;
    for (_, hostname, certificate) in choices(request) {
        match certificate {
            Some(CertificateSource::Issuer { name }) => names.push(name),
            None if hostname.starts_with("*.") => implicit = true,
            _ => {}
        }
    }
    if implicit {
        names.extend(default);
        names.extend(snapshot.ingresses.iter().filter_map(|i| {
            i.metadata
                .annotations
                .as_ref()?
                .get(CLUSTER_ISSUER)
                .map(String::as_str)
        }));
    }
    names.sort_unstable();
    names.dedup();
    let mut out = BTreeMap::new();
    for name in names {
        let read = match cluster
            .find_object(&cert_manager::CLUSTER_ISSUERS.resource(), None, name)
            .await
        {
            Ok(Some(found)) => {
                serde_json::to_value(found).map_or(IssuerRead::Unreadable, |v| IssuerRead::Found {
                    wildcards: cert_manager::issues_wildcards(&v),
                })
            }
            Ok(None) => IssuerRead::Missing,
            Err(_) => IssuerRead::Unreadable,
        };
        out.insert(name.to_owned(), read);
    }
    out
}

/// The cert-manager Certificate each own certificate replaces: the one named after the
/// Secret the hostname's tls entry names now. Absent or unreadable ones are left out; the plan
/// then deletes nothing.
pub(super) async fn issued(
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
    snapshot: &Snapshot,
) -> Vec<Fetched> {
    let resource = zeabur_plan::checks::ResourceRef {
        group: "cert-manager.io".to_owned(),
        version: "v1".to_owned(),
        resource: "certificates".to_owned(),
    };
    let reference = ResourceRef::new("cert-manager.io", "v1", "certificates", "Certificate");
    let mut out = Vec::new();
    for change in &request.changes {
        let Change::Certificate(CertificateChange {
            route,
            hostname,
            certificate: CertificateSource::Own { .. },
        }) = change
        else {
            continue;
        };
        if route.kind != RouteKind::Ingress {
            continue;
        }
        let secret = snapshot
            .ingresses
            .iter()
            .filter(|i| i.metadata.name.as_deref() == Some(route.name.as_str()))
            .filter_map(|i| i.spec.as_ref()?.tls.as_ref())
            .flatten()
            .find(|t| t.hosts.iter().flatten().any(|h| h == hostname))
            .and_then(|t| t.secret_name.clone());
        let Some(secret) = secret else {
            continue;
        };
        let found: Result<Option<Value>, ClusterError> = cluster
            .find_object(&reference, Some(project), &secret)
            .await
            .and_then(|f| {
                f.map(serde_json::to_value)
                    .transpose()
                    .map_err(|e| ClusterError::Decode(e.to_string()))
            });
        if let Ok(Some(object)) = found {
            out.push(Fetched {
                resource: resource.clone(),
                name: secret,
                object,
            });
        }
    }
    out
}

/// Checks each own certificate of the request at `now`.
pub(super) fn checked(
    request: &PlanRequest,
    now: &str,
) -> Result<BTreeMap<String, CheckedCertificate>, ApiError> {
    let now: Timestamp = now.parse().unwrap_or(Timestamp::UNIX_EPOCH);
    let mut out = BTreeMap::new();
    for (at, hostname, certificate) in choices(request) {
        let Some(CertificateSource::Own { certificate, key }) = certificate else {
            continue;
        };
        let checked = check(certificate, key, hostname, now).map_err(|(field, detail)| {
            ApiError::from(Problem {
                at: Some(ProblemAt {
                    change: at,
                    line: None,
                    field: Some(field.to_owned()),
                }),
                ..Problem::new(Kind::Invalid, detail)
            })
        })?;
        out.insert(hostname.to_owned(), checked);
    }
    Ok(out)
}

type Refusal = (&'static str, String);

/// Checks that `certificate` (PEM, leaf first) and `key` (PEM) belong together, cover
/// `hostname` and are valid at `now`.
pub(crate) fn check(
    certificate: &str,
    key: &str,
    hostname: &str,
    now: Timestamp,
) -> Result<CheckedCertificate, Refusal> {
    let chain: Vec<CertificateDer<'static>> =
        CertificateDer::pem_slice_iter(certificate.as_bytes())
            .collect::<Result<_, _>>()
            .map_err(|_error| ("certificate", "The certificate is not PEM.".to_owned()))?;
    let Some(leaf) = chain.first() else {
        return Err((
            "certificate",
            "Paste a PEM certificate, starting with -----BEGIN CERTIFICATE-----.".to_owned(),
        ));
    };
    let parsed = ParsedCertificate::try_from(leaf).map_err(|e| {
        (
            "certificate",
            format!("The certificate cannot be read: {e}."),
        )
    })?;
    let (not_before, not_after) = validity(leaf).ok_or_else(|| {
        (
            "certificate",
            "The certificate's validity dates cannot be read.".to_owned(),
        )
    })?;
    if not_after <= now {
        return Err((
            "certificate",
            format!("The certificate expired at {not_after}."),
        ));
    }
    if not_before > now {
        return Err((
            "certificate",
            format!("The certificate is valid only from {not_before}."),
        ));
    }
    let probe = hostname.strip_prefix("*.").map_or_else(
        || hostname.to_owned(),
        |rest| format!("zeabur-probe.{rest}"),
    );
    let name = ServerName::try_from(probe)
        .map_err(|_error| ("certificate", format!("{hostname} is not a DNS name.")))?;
    verify_server_name(&parsed, &name).map_err(|_error| {
        (
            "certificate",
            format!("The certificate does not cover {hostname}."),
        )
    })?;
    let private = PrivateKeyDer::from_pem_slice(key.as_bytes()).map_err(|_error| {
        (
            "key",
            "The key is not an unencrypted PEM private key (PKCS#8, PKCS#1 or SEC1).".to_owned(),
        )
    })?;
    let signing = any_supported_type(&private)
        .map_err(|_error| ("key", "The key's algorithm is not supported.".to_owned()))?;
    CertifiedKey::new(chain, signing)
        .keys_match()
        .map_err(|_error| {
            (
                "key",
                "The key does not belong to the certificate.".to_owned(),
            )
        })?;
    Ok(CheckedCertificate {
        not_after: not_after.to_string(),
        certificate: pem_text(certificate),
        key: pem_text(key),
    })
}

fn pem_text(pem: &str) -> String {
    format!("{}\n", pem.trim())
}

/// Reads one DER element at the start of `der`: its tag, its contents and the rest.
fn element(der: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, rest) = der.split_first()?;
    let (&first, rest) = rest.split_first()?;
    let (length, rest) = if first < 0x80 {
        (usize::from(first), rest)
    } else {
        let count = usize::from(first & 0x7F);
        if count == 0 || count > 4 {
            return None;
        }
        let (bytes, rest) = rest.split_at_checked(count)?;
        let length = bytes
            .iter()
            .fold(0_usize, |n, &b| (n << 8) | usize::from(b));
        (length, rest)
    };
    let (contents, rest) = rest.split_at_checked(length)?;
    Some((tag, contents, rest))
}

/// The `notBefore` and `notAfter` of an X.509 certificate (RFC 5280 §4.1).
pub(crate) fn validity(der: &[u8]) -> Option<(Timestamp, Timestamp)> {
    const SEQUENCE: u8 = 0x30;
    let (SEQUENCE, certificate, _) = element(der)? else {
        return None;
    };
    let (SEQUENCE, tbs, _) = element(certificate)? else {
        return None;
    };
    let mut rest = tbs;
    let (tag, _, after) = element(rest)?;
    if tag == 0xA0 {
        rest = after;
    }
    // serialNumber, signature, issuer
    for _ in 0..3 {
        rest = element(rest)?.2;
    }
    let (SEQUENCE, validity, _) = element(rest)? else {
        return None;
    };
    let (before_tag, before, after) = element(validity)?;
    let (after_tag, not_after, _) = element(after)?;
    Some((time(before_tag, before)?, time(after_tag, not_after)?))
}

/// A `UTCTime` or `GeneralizedTime` in the `Z` form RFC 5280 requires.
fn time(tag: u8, value: &[u8]) -> Option<Timestamp> {
    let text = std::str::from_utf8(value).ok()?.strip_suffix('Z')?;
    let full = match (tag, text.len()) {
        (0x17, 12) => {
            let year: u16 = text.get(..2)?.parse().ok()?;
            let century = if year >= 50 { "19" } else { "20" };
            format!("{century}{text}")
        }
        (0x18, 14) => text.to_owned(),
        _ => return None,
    };
    let iso = format!(
        "{}-{}-{}T{}:{}:{}Z",
        full.get(..4)?,
        full.get(4..6)?,
        full.get(6..8)?,
        full.get(8..10)?,
        full.get(10..12)?,
        full.get(12..14)?
    );
    iso.parse().ok()
}

/// The dates of the certificate in a `kubernetes.io/tls` Secret. Never its key.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CertificateDates {
    /// The Secret.
    pub(crate) secret: String,
    /// From when the certificate is valid, RFC 3339.
    pub(crate) not_before: String,
    /// When it expires, RFC 3339.
    pub(crate) not_after: String,
}

/// The dates of the certificate a domain's tls entry names, read as the person.
#[utoipa::path(get, path = "/api/v1/projects/{project}/certificates/{secret}", tag = "projects",
    operation_id = "getCertificateDates",
    params(("project" = String, Path), ("secret" = String, Path, description = "A kubernetes.io/tls Secret")),
    responses((status = 200, body = CertificateDates),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    Person(cluster): Person,
    Path((project, secret)): Path<(String, String)>,
) -> Result<Json<CertificateDates>, ApiError> {
    let values = cluster
        .reveal_secret(&project, &secret)
        .await
        .map_err(|e| ApiError::from(e.into_problem(&format!("Secret {secret}"))))?;
    let dates = values
        .get("tls.crt")
        .and_then(|pem| CertificateDer::pem_slice_iter(pem).next()?.ok())
        .and_then(|leaf| validity(&leaf));
    let Some((not_before, not_after)) = dates else {
        return Err(ApiError::from(Problem::new(
            Kind::Invalid,
            format!("Secret {secret} holds no readable certificate under tls.crt."),
        )));
    };
    Ok(Json(CertificateDates {
        secret,
        not_before: not_before.to_string(),
        not_after: not_after.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CERT: &str = include_str!("testdata/shop.crt");
    const KEY: &str = include_str!("testdata/shop.key");
    const OTHER_KEY: &str = include_str!("testdata/other.key");

    fn at(time: &str) -> Timestamp {
        time.parse().unwrap()
    }

    fn refused(certificate: &str, key: &str, host: &str, now: &str) -> Refusal {
        check(certificate, key, host, at(now)).unwrap_err()
    }

    #[test]
    fn accepts_a_matching_pair_and_reads_its_expiry() {
        let checked = check(CERT, KEY, "shop.example.test", at("2026-06-01T00:00:00Z")).unwrap();
        assert_eq!(checked.not_after, "2126-01-01T00:00:00Z");
        assert!(checked.key.starts_with("-----BEGIN PRIVATE KEY-----"));
    }

    #[test]
    fn a_wildcard_host_needs_a_wildcard_name() {
        check(CERT, KEY, "*.apps.example.test", at("2026-06-01T00:00:00Z")).unwrap();
        let (field, _) = refused(CERT, KEY, "*.example.test", "2026-06-01T00:00:00Z");
        assert_eq!(field, "certificate");
    }

    #[test]
    fn refuses_other_hosts_keys_and_dates() {
        let now = "2026-06-01T00:00:00Z";
        assert_eq!(refused(CERT, KEY, "www.example.test", now).0, "certificate");
        assert_eq!(refused(CERT, OTHER_KEY, "shop.example.test", now).0, "key");
        assert_eq!(refused(CERT, "", "shop.example.test", now).0, "key");
        assert_eq!(refused("", KEY, "shop.example.test", now).0, "certificate");
        let (field, detail) = refused(CERT, KEY, "shop.example.test", "2126-02-01T00:00:00Z");
        assert_eq!(
            (field, detail.as_str()),
            (
                "certificate",
                "The certificate expired at 2126-01-01T00:00:00Z."
            )
        );
        assert_eq!(
            refused(CERT, KEY, "shop.example.test", "2025-06-01T00:00:00Z").0,
            "certificate"
        );
    }

    #[test]
    fn a_refusal_never_carries_the_key() {
        let (_, detail) = refused(CERT, OTHER_KEY, "shop.example.test", "2026-06-01T00:00:00Z");
        let body = OTHER_KEY.lines().nth(1).unwrap();
        assert!(!detail.contains(body));
    }
}
