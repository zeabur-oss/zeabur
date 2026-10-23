//! What a new node needs to join the cluster, the way `zeabur-installer node add` prints it.
//!
//! A worker joins an RKE2 cluster with a bootstrap token: a Secret `bootstrap-token-<id>` of type
//! `bootstrap.kubernetes.io/token` in `kube-system`, which the person creates through a plan. The
//! token string the new node needs is `K10<ca-hash>::<id>.<secret>`; the browser makes the id and
//! secret, so this endpoint gives the rest, read as the person:
//!
//! - the CA hash, SHA-256 of `kube-root-ca.crt`'s `ca.crt` (`get` it in `kube-system`), the same
//!   bytes the supervisor serves at `/cacerts`, hashed as k3s's `hashCA` does;
//! - the supervisor, `https://<InternalIP>:9345` of a Ready control-plane node, and that node's
//!   provider and region labels for `join.expect` (`list nodes`);
//! - the bootstrap tokens that have not expired, without their secrets (`list` and `get` on
//!   `secrets` in `kube-system`).
//!
//! Each read the person may not make is listed in `unreadable` and its part left out. Only an
//! RKE2 cluster qualifies: the installer joins RKE2 agents.

use axum::Json;
use axum::extract::State;
use base64::Engine as _;
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::{ConfigMap, Node, Secret};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ClusterError, ClusterExt as _, ResourceRef};

use crate::error::ApiError;
use crate::handlers::{AppState, Person};
use crate::views::UnreadableView;

/// The type of a bootstrap token Secret.
const BOOTSTRAP_TOKEN: &str = "bootstrap.kubernetes.io/token";
/// The RKE2 supervisor's port.
const SUPERVISOR_PORT: u16 = 9345;
const CONTROL_PLANE: &str = "node-role.kubernetes.io/control-plane";
const PROVIDER: &str = "installer.zeabur.com/provider";
const REGION: &str = "topology.kubernetes.io/region";

/// Why this cluster cannot take a node from Zeabur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum JoinBlocked {
    /// Not RKE2 (k3s, or a distribution the version does not name): the installer joins RKE2
    /// agents only.
    NotRke2,
}

/// Where the server is, from its node's labels, for the new node to compare with its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct JoinExpect {
    /// `installer.zeabur.com/provider`.
    pub provider: String,
    /// `topology.kubernetes.io/region`.
    pub region: String,
}

/// A bootstrap token that has not expired, without its secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct JoinToken {
    /// The token id; the Secret is `bootstrap-token-<id>`.
    pub id: String,
    /// The Secret's name.
    pub secret: String,
    /// Its `description`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Its `expiration` (RFC 3339); absent when it never expires.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    /// Its `app.kubernetes.io/managed-by` label (`zeabur`, `zeabur-installer`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub managed_by: Option<String>,
    /// When the Secret was created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
}

/// What a new node needs to join, as far as the person may read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct JoinInfo {
    /// Why nodes cannot be added; absent when they can.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<JoinBlocked>,
    /// kube-apiserver's version (`v1.34.1+rke2r1`).
    pub kubernetes_version: String,
    /// The supervisor a new node joins, `https://<address>:9345`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    /// 64 hex characters of the cluster CA's SHA-256, for the `K10` token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_hash: Option<String>,
    /// The server's provider and region, when its node is labelled with both.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expect: Option<JoinExpect>,
    /// Bootstrap tokens that have not expired, soonest to expire first.
    pub tokens: Vec<JoinToken>,
    /// The reads that were refused.
    pub unreadable: Vec<UnreadableView>,
}

/// What a new node needs to join the cluster: the supervisor's address, the CA hash for the
/// token, the server's location, and the join tokens that have not expired (never their
/// secrets). Each read is the person's; one they may not make is listed in `unreadable`.
#[utoipa::path(get, path = "/api/v1/cluster/join", tag = "cluster", operation_id = "getJoin",
    responses((status = 200, body = JoinInfo), (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Result<Json<JoinInfo>, ApiError> {
    let version = cluster
        .version()
        .await
        .map_err(|e| ApiError::from(e.into_problem("the cluster's version")))?;
    let mut info = JoinInfo {
        blocked: None,
        kubernetes_version: version,
        server: None,
        ca_hash: None,
        expect: None,
        tokens: Vec::new(),
        unreadable: Vec::new(),
    };
    if !info.kubernetes_version.contains("+rke2") {
        info.blocked = Some(JoinBlocked::NotRke2);
        return Ok(Json(info));
    }
    let config_maps = ResourceRef::of::<ConfigMap>();
    let (nodes, ca, tokens) = futures_util::join!(
        cluster.list::<Node>(None),
        cluster.find_object(&config_maps, Some("kube-system"), "kube-root-ca.crt"),
        tokens(cluster.as_ref(), (state.clock)()),
    );
    let fail = |what: &'static str| move |e: ClusterError| ApiError::from(e.into_problem(what));
    let nodes = readable(nodes, "nodes", &mut info.unreadable).map_err(fail("nodes"))?;
    if let Some(server) = nodes.as_deref().and_then(server_node) {
        info.server = internal_ip(server).map(|ip| supervisor(&ip));
        let label = |key: &str| {
            server
                .metadata
                .labels
                .as_ref()
                .and_then(|l| l.get(key))
                .filter(|v| !v.is_empty())
                .cloned()
        };
        if let (Some(provider), Some(region)) = (label(PROVIDER), label(REGION)) {
            info.expect = Some(JoinExpect { provider, region });
        }
    }
    if let Some(Some(map)) =
        readable(ca, "configmaps", &mut info.unreadable).map_err(fail("kube-root-ca.crt"))?
    {
        info.ca_hash = map
            .data
            .pointer("/data/ca.crt")
            .and_then(serde_json::Value::as_str)
            .and_then(|pem| hash_ca(pem.as_bytes()));
    }
    if let Some(tokens) =
        readable(tokens, "secrets", &mut info.unreadable).map_err(fail("bootstrap tokens"))?
    {
        info.tokens = tokens;
    }
    Ok(Json(info))
}

fn readable<T>(
    result: Result<T, ClusterError>,
    resource: &str,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<Option<T>, ClusterError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) => {
            unreadable.push(error.into_unreadable(resource)?.into());
            Ok(None)
        }
    }
}

/// The bootstrap tokens in `kube-system` that have not expired at `now`. The list hides values,
/// so each token's own fields are read with a `get`; the secret is dropped here.
async fn tokens(cluster: &dyn Cluster, now: Timestamp) -> Result<Vec<JoinToken>, ClusterError> {
    let secrets = cluster.list::<Secret>(Some("kube-system")).await?;
    let mut out = Vec::new();
    for secret in secrets
        .iter()
        .filter(|s| s.type_.as_deref() == Some(BOOTSTRAP_TOKEN))
    {
        let name = secret.metadata.name.clone().unwrap_or_default();
        let values = match cluster.reveal_secret("kube-system", &name).await {
            Ok(values) => values,
            Err(error) if error.is_not_found() => continue,
            Err(error) => return Err(error),
        };
        let text = |key: &str| {
            values
                .get(key)
                .map(|v| String::from_utf8_lossy(v).trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let expires = text("expiration");
        if let Some(at) = &expires
            && at.parse::<Timestamp>().is_ok_and(|at| at <= now)
        {
            continue;
        }
        out.push(JoinToken {
            id: text("token-id")
                .or_else(|| name.strip_prefix("bootstrap-token-").map(str::to_owned))
                .unwrap_or_default(),
            secret: name,
            description: text("description"),
            expires,
            managed_by: secret
                .metadata
                .labels
                .as_ref()
                .and_then(|l| l.get("app.kubernetes.io/managed-by"))
                .cloned(),
            created: secret
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|t| t.0.to_string()),
        });
    }
    out.sort_by(|a, b| {
        let at = |t: &JoinToken| {
            t.expires
                .as_deref()
                .and_then(|e| e.parse::<Timestamp>().ok())
        };
        match (at(a), at(b)) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.id.cmp(&b.id),
        }
    });
    Ok(out)
}

/// The first Ready control-plane node by name.
fn server_node(nodes: &[Node]) -> Option<&Node> {
    let mut servers: Vec<&Node> = nodes
        .iter()
        .filter(|n| {
            n.metadata
                .labels
                .as_ref()
                .is_some_and(|l| l.get(CONTROL_PLANE).is_some_and(|v| v == "true"))
        })
        .collect();
    servers.sort_by_key(|n| {
        let ready = n
            .status
            .as_ref()
            .and_then(|s| s.conditions.as_ref())
            .is_some_and(|c| c.iter().any(|c| c.type_ == "Ready" && c.status == "True"));
        (!ready, n.metadata.name.clone())
    });
    servers.into_iter().next()
}

fn internal_ip(node: &Node) -> Option<String> {
    node.status
        .as_ref()?
        .addresses
        .as_ref()?
        .iter()
        .find(|a| a.type_ == "InternalIP")
        .map(|a| a.address.clone())
}

fn supervisor(address: &str) -> String {
    if address.contains(':') {
        format!("https://[{address}]:{SUPERVISOR_PORT}")
    } else {
        format!("https://{address}:{SUPERVISOR_PORT}")
    }
}

/// The CA hash of a `K10` token, as k3s's `hashCA`: SHA-256 of the PEM bytes as they are when
/// they hold one certificate, else of the DER of the root of the first certificate's chain.
fn hash_ca(pem: &[u8]) -> Option<String> {
    let certificates = certificates(std::str::from_utf8(pem).ok()?);
    match certificates.as_slice() {
        [] => None,
        [_] => Some(hex(&Sha256::digest(pem))),
        [first, ..] => Some(hex(&Sha256::digest(root(first, &certificates)?))),
    }
}

fn certificates(pem: &str) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut body: Option<String> = None;
    for line in pem.lines().map(str::trim) {
        if line == "-----BEGIN CERTIFICATE-----" {
            body = Some(String::new());
        } else if line == "-----END CERTIFICATE-----" {
            if let Some(der) = body
                .take()
                .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
            {
                out.push(der);
            }
        } else if let Some(body) = &mut body {
            body.push_str(line);
        }
    }
    out
}

/// Following issuers from `first` within `bundle` to a self-signed certificate, or to the last
/// one the bundle holds.
fn root<'a>(first: &'a [u8], bundle: &'a [Vec<u8>]) -> Option<&'a [u8]> {
    let mut current = first;
    for _ in 0..bundle.len() {
        let (issuer, subject) = names(current)?;
        if issuer == subject {
            return Some(current);
        }
        match bundle
            .iter()
            .find(|c| names(c).is_some_and(|(_, s)| s == issuer))
        {
            Some(next) => current = next,
            None => return Some(current),
        }
    }
    Some(current)
}

/// The issuer and subject of a DER certificate, as their encoded bytes.
fn names(der: &[u8]) -> Option<(&[u8], &[u8])> {
    let (_, certificate, _) = tlv(der)?;
    let (_, tbs, _) = tlv(certificate)?;
    let mut rest = tbs;
    let (tag, _, after) = tlv(rest)?;
    if tag == 0xA0 {
        rest = after;
    }
    let (_, _, rest) = tlv(rest)?; // serialNumber
    let (_, _, rest) = tlv(rest)?; // signature
    let issuer_len = rest.len() - tlv(rest)?.2.len();
    let (issuer, rest) = rest.split_at(issuer_len);
    let (_, _, rest) = tlv(rest)?; // validity
    let subject_len = rest.len() - tlv(rest)?.2.len();
    Some((issuer, rest.get(..subject_len)?))
}

/// One DER element: its tag, its contents and what follows it.
fn tlv(input: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, rest) = input.split_first()?;
    let (&first, mut rest) = rest.split_first()?;
    let length = if first < 0x80 {
        usize::from(first)
    } else {
        let count = usize::from(first & 0x7F);
        if count == 0 || count > 4 || rest.len() < count {
            return None;
        }
        let (bytes, after) = rest.split_at(count);
        rest = after;
        bytes
            .iter()
            .fold(0_usize, |n, &b| (n << 8) | usize::from(b))
    };
    let (contents, after) = rest.split_at_checked(length)?;
    Some((tag, contents, after))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, b| {
        write!(out, "{b:02x}").unwrap_or_default();
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Two self-signed certificates made with `openssl req -x509` (P-256), and a leaf signed by
    // the first.
    const ROOT: &str = include_str!("../../tests/fixtures/join/root.pem");
    const OTHER: &str = include_str!("../../tests/fixtures/join/other.pem");
    const LEAF: &str = include_str!("../../tests/fixtures/join/leaf.pem");

    #[test]
    fn one_certificate_hashes_its_pem_bytes_as_they_are() {
        let hash = hash_ca(ROOT.as_bytes()).unwrap();
        assert_eq!(hash, hex(&Sha256::digest(ROOT.as_bytes())));
        assert_eq!(hash.len(), 64);
        let without_newline = ROOT.trim_end();
        assert_ne!(hash_ca(without_newline.as_bytes()).unwrap(), hash);
    }

    #[test]
    fn a_bundle_hashes_the_root_of_the_first_chain() {
        let root_der = &certificates(ROOT)[0];
        let bundle = format!("{LEAF}{OTHER}{ROOT}");
        assert_eq!(
            hash_ca(bundle.as_bytes()).unwrap(),
            hex(&Sha256::digest(root_der))
        );
        let other_first = format!("{OTHER}{ROOT}");
        assert_eq!(
            hash_ca(other_first.as_bytes()).unwrap(),
            hex(&Sha256::digest(&certificates(OTHER)[0]))
        );
    }

    #[test]
    fn issuer_and_subject_are_read_from_the_der() {
        let leaf = &certificates(LEAF)[0];
        let root = &certificates(ROOT)[0];
        let (issuer, subject) = names(leaf).unwrap();
        assert_ne!(issuer, subject);
        assert_eq!(issuer, names(root).unwrap().1);
    }

    #[test]
    fn no_certificate_no_hash() {
        assert_eq!(hash_ca(b"not a certificate"), None);
    }

    #[test]
    fn an_ipv6_supervisor_is_bracketed() {
        assert_eq!(supervisor("10.0.0.5"), "https://10.0.0.5:9345");
        assert_eq!(supervisor("fd00::5"), "https://[fd00::5]:9345");
    }
}
