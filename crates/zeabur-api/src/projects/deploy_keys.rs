//! Deploy keys for Zeabur Builder: `POST /projects/{p}/deploy-keys`.
//!
//! It makes an SSH key pair whose public half the person adds to the repository on GitHub or
//! GitLab; the private half never leaves the server in the clear.
//!
//! The answer carries the private key sealed (AES-256-GCM under the plan key, as plan tokens
//! are), bound to the person, the project and the Git host. A plan's
//! [`Change::DeployKey`](zeabur_plan::Change::DeployKey) hands it back; [`open`] turns it into
//! the `create` of a `kubernetes.io/ssh-auth` Secret with the private key and the host's
//! `known_hosts`, which Zeabur Builder requires. The plan shows the Secret with its values hidden,
//! like every Secret it writes.
//!
//! Only hosts whose SSH host keys are pinned here get a key: Zeabur does not fetch a host key
//! over the network, since a first connection cannot tell a real host from one in between. Any
//! other host takes an HTTPS access token instead.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_plan::{Change, PlanRequest};

use crate::error::ApiError;
use crate::handlers::{AppState, Everyday};
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
pub struct DeployKeyRequest {
    /// `github.com` or `gitlab.com`.
    pub host: String,
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
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    person: String,
    project: String,
    host: String,
    private: String,
    expires_at: i64,
}

fn known_hosts(host: &str) -> Option<&'static str> {
    KNOWN_HOSTS
        .iter()
        .find(|(name, _)| *name == host)
        .map(|(_, line)| *line)
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
    Everyday(everyday): Everyday,
    Path(project): Path<String>,
    Json(request): Json<DeployKeyRequest>,
) -> Result<Json<DeployKeyResponse>, ApiError> {
    let host = request.host.trim().to_ascii_lowercase();
    if known_hosts(&host).is_none() {
        return Err(ApiError::from(Problem::new(
            Kind::BadRequest,
            format!(
                "Zeabur makes deploy keys for GitHub and GitLab only, not {host}; use an access \
                 token over HTTPS."
            ),
        )));
    }
    let sealing = key_of(&state)?;
    let identity = everyday
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
    };
    let key = token::sign_as(&sealing, PREFIX, &claims).map_err(random)?;
    Ok(Json(DeployKeyResponse {
        public_key: pair.public,
        key,
        expires_at: zeabur_kube::k8s_openapi::jiff::Timestamp::from_second(expires_at)
            .unwrap_or(zeabur_kube::k8s_openapi::jiff::Timestamp::UNIX_EPOCH)
            .to_string(),
    }))
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
            let hosts = known_hosts(&claims.host)
                .ok_or_else(|| refused("The deploy key is for a host Zeabur no longer knows."))?;
            Ok(Change::Create {
                object: secret(name, &claims.private, hosts),
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
        "stringData": {"ssh-privatekey": private, "known_hosts": format!("{known_hosts}\n")},
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
    fn the_secret_is_one_zeabur_builder_reads() {
        let object = secret("web-git", "PRIVATE", "github.com ssh-ed25519 AAAA");
        assert_eq!(object["type"], "kubernetes.io/ssh-auth");
        assert_eq!(object["metadata"]["labels"][GIT_CREDENTIALS_LABEL], "true");
        assert_eq!(object["stringData"]["ssh-privatekey"], "PRIVATE");
        assert_eq!(
            object["stringData"]["known_hosts"],
            "github.com ssh-ed25519 AAAA\n"
        );
    }
}
