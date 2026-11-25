//! Deploy keys for Zeabur Builder.
//!
//! A plan's [`Change::CreateBuiltService`] or [`Change::SetBuild`] from Git with a deploy key
//! names the Git host. The plan shows the `create` of a `kubernetes.io/ssh-auth` Secret with its
//! values hidden, like every Secret it writes; the apply makes the SSH key pair and writes the
//! private key and the host's `known_hosts`, which Zeabur Builder requires, into that Secret. The
//! private key never leaves the server. The person then reads the public key from the Secret (`POST
//! /projects/{p}/builds/{build}/deploy-key`) and adds it to the repository on its Git host.
//!
//! GitHub's and GitLab's host keys are pinned here. For any other host the person gives its
//! `known_hosts` lines, from `ssh-keyscan` and checked against the fingerprints the host
//! publishes (`POST /projects/{p}/deploy-keys/host-keys` repeats each key's fingerprint). Zeabur
//! does not fetch a host key over the network, since a first connection cannot tell a real host
//! from one in between.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_modules::zeabur_builder;
use zeabur_plan::builds::{BUILDER_GROUP, BUILDER_VERSION};
use zeabur_plan::{
    BuildSource, BuiltServiceChange, Change, DeployKeySecret, GitAccess, PlanRequest,
    SetBuildChange,
};

use crate::error::{ApiError, get_read, read_problem};
use crate::extract::{Json, Path};
use crate::handlers::Person;

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

/// The host keys a deploy key's Secret will trust.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeployKeyHost {
    /// Each host key, to compare with the fingerprints the host publishes.
    pub host_keys: Vec<TrustedHostKey>,
}

/// The public half of a deploy key Zeabur made.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeployKeyPublic {
    /// One `ssh-ed25519 …` line, to add on the Git host again.
    pub public_key: String,
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

/// The host keys a deploy key for `host` will trust: the pinned ones for GitHub and GitLab, else
/// those of the `known_hosts` lines given. Makes no key.
#[utoipa::path(post, path = "/api/v1/projects/{project}/deploy-keys/host-keys", tag = "projects",
    operation_id = "inspectDeployKeyHost",
    params(("project" = crate::rules::ProjectName, Path, description = "The project's namespace")),
    request_body = DeployKeyRequest,
    responses((status = 200, body = DeployKeyHost),
              (status = 400, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn host_keys(
    Person(_): Person,
    Path(_project): Path<String>,
    Json(request): Json<DeployKeyRequest>,
) -> Result<Json<DeployKeyHost>, ApiError> {
    let host = request.host.trim().to_ascii_lowercase();
    let (_, host_keys) = trusted(&host, request.known_hosts.as_deref())?;
    Ok(Json(DeployKeyHost { host_keys }))
}

/// The public key of the deploy key `Build` `build` reads its repository with, read as the
/// person, for adding it to the Git host. Only on the person's request: the Secret holds the
/// private key too.
#[utoipa::path(post, path = "/api/v1/projects/{project}/builds/{build}/deploy-key", tag = "projects",
    operation_id = "revealDeployKey",
    params(("project" = crate::rules::ProjectName, Path, description = "The project's namespace"),
           ("build" = String, Path, description = "The `Build`")),
    responses((status = 200, body = DeployKeyPublic),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn public(
    Person(cluster): Person,
    Path((project, build)): Path<(String, String)>,
) -> Result<Json<DeployKeyPublic>, ApiError> {
    let resource = zeabur_kube::ResourceRef::new(BUILDER_GROUP, BUILDER_VERSION, "builds", "Build");
    let found = cluster
        .get_object(&resource, Some(&project), &build)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("Build {build}"),
                get_read(BUILDER_GROUP, "builds", Some(&project), &build),
            )
        })?;
    let name = found
        .as_ref()
        .and_then(|object| zeabur_builder::builds(std::slice::from_ref(object), None).pop())
        .and_then(|b| b.credentials)
        .map(|c| c.name)
        .ok_or_else(|| {
            refused(&format!(
                "Build {build} reads its repository without a key."
            ))
        })?;
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

/// Whether [`open`] makes the key pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyPair {
    /// For a plan: the Secret holds [`PLANNED_KEY`] in place of a key, and its values are hidden
    /// anyway.
    Hidden,
    /// For an apply: a new key pair.
    New,
}

/// What a planned Secret holds in place of the private key. Kubernetes refuses an
/// `kubernetes.io/ssh-auth` Secret with an empty `ssh-privatekey` [k8s-ssh-auth], and the plan's
/// dry run sends it.
///
/// [k8s-ssh-auth]: https://github.com/kubernetes/kubernetes/blob/v1.34.0/pkg/apis/core/validation/validation.go#L7407-L7411
const PLANNED_KEY: &str = "<made at apply>";

/// `request` with a deploy key made for each Git service that reads its repository with one;
/// the request as it is when it has none.
///
/// # Errors
///
/// `400` for a host that is not pinned and has no valid `known_hosts` lines; `500` when the
/// random number generator fails.
pub(crate) fn open(
    project: &str,
    request: &PlanRequest,
    pair: KeyPair,
) -> Result<PlanRequest, ApiError> {
    let changes = request
        .changes
        .iter()
        .map(|change| {
            let mut change = change.clone();
            let (Change::CreateBuiltService(BuiltServiceChange { build, .. })
            | Change::SetBuild(SetBuildChange { build, .. })) = &mut change
            else {
                return Ok(change);
            };
            let BuildSource::Git {
                url,
                access: GitAccess::DeployKey { known_hosts },
                deploy_key: slot,
                ..
            } = &mut build.source
            else {
                return Ok(change);
            };
            let host = zeabur_plan::deploy_key_host(url)
                .ok_or_else(|| refused("Use a repository URL like git@github.com:acme/web.git."))?;
            let (lines, _) = trusted(&host, known_hosts.as_deref())?;
            let private_key = match pair {
                KeyPair::Hidden => PLANNED_KEY.to_owned(),
                KeyPair::New => {
                    zeabur_core::ssh::deploy_key(&format!("zeabur {project}"))
                        .map_err(random)?
                        .private
                }
            };
            *slot = Some(DeployKeySecret {
                private_key,
                known_hosts: lines,
            });
            Ok(change)
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    Ok(PlanRequest { changes })
}

#[cfg(test)]
mod tests {
    use zeabur_plan::{BuildInput, BuiltServiceChange};

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
    fn a_planned_secret_holds_a_key_kubernetes_accepts() {
        let request = PlanRequest {
            changes: vec![Change::CreateBuiltService(BuiltServiceChange {
                name: "web".to_owned(),
                port: None,
                host: None,
                variables: Vec::new(),
                size: None,
                security: None,
                build: BuildInput {
                    source: BuildSource::Git {
                        url: "git@github.com:acme/web.git".to_owned(),
                        branch: None,
                        access: GitAccess::DeployKey { known_hosts: None },
                        deploy_key: None,
                    },
                    method: zeabur_plan::BuildMethod::Railpack,
                    directory: None,
                    dockerfile: None,
                    build_variables: Vec::new(),
                },
            })],
        };
        let planned = open("shop", &request, KeyPair::Hidden).unwrap();
        let [Change::CreateBuiltService(service)] = planned.changes.as_slice() else {
            panic!("{:?}", planned.changes);
        };
        let BuildSource::Git { deploy_key, .. } = &service.build.source else {
            panic!("{service:?}");
        };
        let key = deploy_key.as_ref().unwrap();
        assert_eq!(key.private_key, PLANNED_KEY);
        assert!(key.known_hosts.starts_with("github.com ssh-ed25519 "));
    }
}
