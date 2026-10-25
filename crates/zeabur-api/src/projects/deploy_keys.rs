//! Deploy keys for Zeabur Builder: `POST /projects/{p}/deploy-keys`.
//!
//! It makes an SSH key pair whose public half the person adds to the repository on its Git host;
//! the private half never leaves the server in the clear.
//!
//! The answer carries the private key sealed (AES-256-GCM under the plan key, as plan tokens
//! are), bound to the person, the project and the Git host. A plan's
//! [`Change::DeployKey`] hands it back; `open` turns it into
//! the `create` of a `kubernetes.io/ssh-auth` Secret with the private key and the host's
//! `known_hosts`, which Zeabur Builder requires. The plan shows the Secret with its values hidden,
//! like every Secret it writes.
//!
//! GitHub's and GitLab's host keys are pinned here. For any other host the person gives its
//! `known_hosts` lines, from `ssh-keyscan` and checked against the fingerprints the host
//! publishes; the answer repeats each key's fingerprint. Zeabur does not fetch a host key over the
//! network, since a first connection cannot tell a real host from one in between.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_plan::{Change, PlanRequest};

use crate::error::{ApiError, get_read, read_problem};
use crate::handlers::{AppState, Person};
use crate::plan_token::{self as token, PlanKey, TokenError};

const PREFIX: &str = "k1.";
/// How long a sealed key may wait for its plan: long enough to add the key on the Git host.
const LIFETIME_SECONDS: i64 = 24 * 60 * 60;
/// The label Zeabur Builder reads credentials by.
pub const GIT_CREDENTIALS_LABEL: &str = "builder.zeabur.com/git-credentials";

/// The Git hosts a deploy key is made for, with their published Ed25519 host keys.
///
/// GitHub's from `https://api.github.com/meta` (`SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU`),
/// GitLab's from its documentation (`SHA256:eUXGGm1YGsMAS7vkcx6JOJdOGHPem5gQp4taiCfCLB8`).
pub const KNOWN_HOSTS: [(&str, &str); 2] = [
    (
        "github.com",
        "github.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl",
    ),
    (
        "gitlab.com",
        "gitlab.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAfuCHKVTjquxvt6CM6tdG4SLp1Btn/nOeHHE5UOzRdf",
    ),
];

/// The Git host a deploy key is for.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeployKeyRequest {
    /// The host as the repository's SSH URL names it, with `:port` when it is not 22
    /// (`git.example.com:2222`, `[2001:db8::1]:2222`).
    pub host: String,
    /// The host's `known_hosts` lines, as `ssh-keyscan` prints them; needed for any host but
    /// `github.com` and `gitlab.com`, whose keys Zeabur pins.
    #[serde(default)]
    pub known_hosts: Option<String>,
}

/// A host key a deploy key's Secret trusts.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrustedHostKey {
    /// The key type, such as `ssh-ed25519`.
    pub algorithm: String,
    /// `SHA256:…`, as `ssh-keygen -l` prints it.
    pub fingerprint: String,
}

/// A new deploy key.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeployKeyResponse {
    /// The public key, one `ssh-ed25519 …` line, to add on the Git host.
    pub public_key: String,
    /// The private key, sealed: give it back in a plan's `deployKey` change.
    pub key: String,
    /// When the sealed key stops working, RFC 3339.
    pub expires_at: String,
    /// The host keys the Secret will trust.
    pub host_keys: Vec<TrustedHostKey>,
}

/// The public half of a deploy key Zeabur made.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeployKeyPublic {
    /// One `ssh-ed25519 …` line, to add on the Git host again.
    pub public_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    person: String,
    project: String,
    host: String,
    private: String,
    expires_at: i64,
    /// The person's `known_hosts` lines, for a host whose keys are not pinned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    known_hosts: Option<String>,
}

fn known_hosts(host: &str) -> Option<&'static str> {
    KNOWN_HOSTS
        .iter()
        .find(|(name, _)| *name == host)
        .map(|(_, line)| *line)
}

/// The name and port of `host:port`, `[v6]:port` or a bare name.
fn split_port(host: &str) -> Option<(&str, u16)> {
    if let Some(rest) = host.strip_prefix('[') {
        let (name, after) = rest.split_once(']')?;
        let port = match after.strip_prefix(':') {
            Some(port) => port.parse().ok()?,
            None if after.is_empty() => 22,
            None => return None,
        };
        return Some((name, port));
    }
    match host.rsplit_once(':') {
        Some((name, port)) if !name.contains(':') => Some((name, port.parse().ok()?)),
        Some(_) => None,
        None => Some((host, 22)),
    }
}

/// The `known_hosts` lines a key for `host` writes, and the keys they hold.
fn trusted(host: &str, given: Option<&str>) -> Result<(String, Vec<TrustedHostKey>), ApiError> {
    let (name, port) = split_port(host)
        .filter(|(name, _)| !name.is_empty())
        .ok_or_else(|| refused(&format!("{host:?} is not a host such as git.example.com.")))?;
    let pinned = known_hosts(name).filter(|_| port == 22);
    let text = match (pinned, given) {
        (Some(line), _) => line,
        (None, Some(text)) if !text.trim().is_empty() => text,
        (None, _) => {
            return Err(refused(&format!(
                "Add the known_hosts lines of {host}: Zeabur pins host keys for GitHub and GitLab \
                 only."
            )));
        }
    };
    let keys = zeabur_core::ssh::host_keys(text, name, port).map_err(|e| {
        use zeabur_core::ssh::KnownHostsError;
        let pattern = zeabur_core::ssh::host_pattern(name, port);
        refused(&match e {
            KnownHostsError::TooLong => format!(
                "The known_hosts lines are longer than {} KiB.",
                zeabur_core::ssh::KNOWN_HOSTS_MAX / 1024
            ),
            KnownHostsError::Hashed => format!(
                "The host names are hashed. Run ssh-keyscan without -H, so the lines start with \
                 {pattern}."
            ),
            KnownHostsError::Unsupported => format!(
                "Zeabur Builder takes Ed25519 or ECDSA host keys, and the lines list none for \
                 {pattern}. Run ssh-keyscan -t ed25519,ecdsa."
            ),
            KnownHostsError::NotListed => {
                format!("No line names {pattern}. Paste the lines ssh-keyscan prints for it.")
            }
        })
    })?;
    let lines: String = keys.iter().flat_map(|k| [k.line.as_str(), "\n"]).collect();
    let shown = keys
        .into_iter()
        .map(|k| TrustedHostKey {
            algorithm: k.algorithm,
            fingerprint: k.fingerprint,
        })
        .collect();
    Ok((lines, shown))
}

fn key_of(state: &AppState) -> Result<std::sync::Arc<PlanKey>, ApiError> {
    state.plan_key.clone().ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "Zeabur could not make a token key when it started, so it cannot seal a deploy key.",
        ))
    })
}

/// Makes an SSH deploy key for a repository on `host`: the public key to add there, and the
/// private key sealed for the plan that creates its Secret.
#[utoipa::path(post, path = "/api/v1/projects/{project}/deploy-keys", tag = "projects",
    operation_id = "createDeployKey",
    params(("project" = String, Path, description = "The project's namespace")),
    request_body = DeployKeyRequest,
    responses((status = 200, body = DeployKeyResponse),
              (status = 400, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn create(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(project): Path<String>,
    Json(request): Json<DeployKeyRequest>,
) -> Result<Json<DeployKeyResponse>, ApiError> {
    let host = request.host.trim().to_ascii_lowercase();
    let (lines, host_keys) = trusted(&host, request.known_hosts.as_deref())?;
    let pinned =
        split_port(&host).is_some_and(|(name, port)| port == 22 && known_hosts(name).is_some());
    let sealing = key_of(&state)?;
    let identity = cluster
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let pair = zeabur_core::ssh::deploy_key(&format!("zeabur {project}")).map_err(random)?;
    let now = (state.clock)().as_second();
    let expires_at = now.saturating_add(LIFETIME_SECONDS);
    let claims = Claims {
        person: identity.username,
        project,
        host,
        private: pair.private,
        expires_at,
        known_hosts: (!pinned).then_some(lines),
    };
    let key = token::sign_as(&sealing, PREFIX, &claims).map_err(random)?;
    Ok(Json(DeployKeyResponse {
        public_key: pair.public,
        key,
        expires_at: zeabur_kube::k8s_openapi::jiff::Timestamp::from_second(expires_at)
            .unwrap_or(zeabur_kube::k8s_openapi::jiff::Timestamp::UNIX_EPOCH)
            .to_string(),
        host_keys,
    }))
}

/// The public key of the deploy key in Secret `name`, read as the person, for adding it to the
/// Git host again. Only on the person's request: the Secret holds the private key too.
#[utoipa::path(post, path = "/api/v1/projects/{project}/deploy-keys/{name}/public", tag = "projects",
    operation_id = "deployKeyPublic",
    params(("project" = String, Path, description = "The project's namespace"),
           ("name" = String, Path, description = "The Secret holding the key")),
    responses((status = 200, body = DeployKeyPublic),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn public(
    Person(cluster): Person,
    Path((project, name)): Path<(String, String)>,
) -> Result<Json<DeployKeyPublic>, ApiError> {
    let values = cluster.reveal_secret(&project, &name).await.map_err(|e| {
        read_problem(
            e,
            &format!("secret {name}"),
            get_read("", "secrets", Some(&project), &name),
        )
    })?;
    let public_key = values
        .get("ssh-privatekey")
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .and_then(zeabur_core::ssh::public_key)
        .ok_or_else(|| refused(&format!("Secret {name} holds no SSH key Zeabur can read.")))?;
    Ok(Json(DeployKeyPublic { public_key }))
}

fn random(_error: zeabur_core::crypto::RandomError) -> ApiError {
    ApiError::from(Problem::new(
        Kind::Internal,
        "The system random number generator failed, so Zeabur could not make the key.",
    ))
}

fn refused(detail: &str) -> ApiError {
    ApiError::from(Problem::new(Kind::BadRequest, detail))
}

/// `request` with each `deployKey` change opened into the `create` of its Secret; the request
/// as it is when it has none.
///
/// # Errors
///
/// `400` for a key that does not open, has expired, or was made for another person or project.
pub(crate) fn open(
    state: &AppState,
    person: &str,
    project: &str,
    request: &PlanRequest,
) -> Result<PlanRequest, ApiError> {
    if !request
        .changes
        .iter()
        .any(|c| matches!(c, Change::DeployKey { .. }))
    {
        return Ok(request.clone());
    }
    let sealing = key_of(state)?;
    let now = (state.clock)().as_second();
    let changes = request
        .changes
        .iter()
        .map(|change| {
            let Change::DeployKey { name, key } = change else {
                return Ok(change.clone());
            };
            let claims: Claims = token::open_as(&sealing, PREFIX, key).map_err(|e| match e {
                TokenError::Expired | TokenError::Malformed => {
                    refused("The deploy key is not one Zeabur made. Make a new one.")
                }
                TokenError::Altered => refused(
                    "The deploy key was altered, or is not from this server. Make a new one.",
                ),
            })?;
            if now >= claims.expires_at {
                return Err(refused(
                    "The deploy key expired before it was used. Make a new one and add it again.",
                ));
            }
            if claims.person != person || claims.project != project {
                return Err(refused(
                    "This deploy key was made for someone else or another project.",
                ));
            }
            let hosts = match claims.known_hosts {
                Some(lines) => lines,
                None => trusted(&claims.host, None)?.0,
            };
            Ok(Change::Create {
                object: secret(name, &claims.private, &hosts),
                yaml: None,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    Ok(PlanRequest { changes })
}

fn secret(name: &str, private: &str, known_hosts: &str) -> serde_json::Value {
    let labels = BTreeMap::from([(GIT_CREDENTIALS_LABEL, "true")]);
    json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {"name": name, "labels": labels},
        "type": "kubernetes.io/ssh-auth",
        "stringData": {"ssh-privatekey": private, "known_hosts": known_hosts},
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pinned_host_key_names_its_host() {
        for (host, line) in KNOWN_HOSTS {
            assert!(line.starts_with(&format!("{host} ssh-ed25519 ")), "{line}");
        }
        assert_eq!(known_hosts("example.com"), None);
    }

    #[test]
    fn a_host_names_its_port_when_it_is_not_22() {
        assert_eq!(split_port("git.example.com"), Some(("git.example.com", 22)));
        assert_eq!(
            split_port("git.example.com:2222"),
            Some(("git.example.com", 2222))
        );
        assert_eq!(
            split_port("[2001:db8::1]:2222"),
            Some(("2001:db8::1", 2222))
        );
        assert_eq!(split_port("git.example.com:x"), None);
        assert_eq!(split_port("2001:db8::1"), None);
    }

    #[test]
    fn the_secret_is_one_zeabur_builder_reads() {
        let object = secret("web-git", "PRIVATE", "github.com ssh-ed25519 AAAA\n");
        assert_eq!(object["type"], "kubernetes.io/ssh-auth");
        assert_eq!(object["metadata"]["labels"][GIT_CREDENTIALS_LABEL], "true");
        assert_eq!(object["stringData"]["ssh-privatekey"], "PRIVATE");
        assert_eq!(
            object["stringData"]["known_hosts"],
            "github.com ssh-ed25519 AAAA\n"
        );
    }
}
