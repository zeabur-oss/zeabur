//! A service whose image Zeabur Builder builds: its `Build`, from an uploaded folder or a Git
//! repository, and the objects of the image service that runs what it builds.
//!
//! Every change that writes a `Build` takes one [`BuildInput`], whatever its source, and goes
//! through one check and one `Build` object.
//!
//! - [`crate::Change::CreateBuiltService`] makes a new service, sent by the web interface and
//!   `zeaburctl up`. The objects are created as a `create` of each is: the credentials and build
//!   variables first, the `Build` last. The Deployment names the `Build`'s repository with a tag no
//!   build makes, and Keel moves it to each image the `Build` pushes.
//! - [`crate::Change::SetBuild`] gives a running service its `Build`, named after the workload. A
//!   service without one gets one, and its container moves to the `Build`'s repository with Keel's
//!   policy, as a new service has. A service with one keeps it, and its source, method and paths
//!   become the new ones: an uploaded folder becomes a repository, a repository another one or an
//!   uploaded folder, and an uploaded folder is uploaded again.
//! - [`crate::Change::RemoveBuild`] stops building a service: its `Build` goes, with the
//!   credentials Secret only it reads, each container that runs what it built keeps the image it
//!   runs, pinned to its digest, and Keel stops when no other `Build` makes the service's images.
//!
//! An uploaded folder's `Build` carries the annotation that asks Zeabur Builder for an upload
//! token; the client uploads the folder after the plan.
//!
//! When Zeabur has a public URL, the `Build` that [`crate::Change::CreateBuiltService`] or
//! [`crate::Change::SetBuild`] writes carries [`DETAILS_URL`]: the service's Builds tab, which
//! Zeabur Builder links its commit statuses to.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{EnvVar, EnvVarSource, Secret, SecretKeySelector};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zeabur_graph::Workload;
use zeabur_model::{ImageUpdates, PodSecurityLevel, UpdateSetting, WorkloadKind, WorkloadRef};

use crate::builds::{
    BUILDER_GROUP, BUILDER_VERSION, DETAILS_URL, GIT_CREDENTIALS_LABEL, KEEL_FORCE, KEEL_POLICY,
    UPLOAD_TOKEN_REQUEST, build_path, builds,
};
use crate::change::{
    AutoUpdateTarget, BuildMethod, DataEdit, DataKind, NameValue, ServiceSize, Update,
};
use crate::checks::ResourceRef;
use crate::fields::Step;
use crate::image_service::{
    ImageServiceChange, check_host, check_name, check_variable_names, image_objects, labels_of,
    opaque_secret, plain_variables, refused,
};
use crate::json::{add, pointer, replace, test};
use crate::plan::{ErrorKind, PlanError, Planner, error, to_json};
use crate::project::{set_key, shown, steps};
use crate::rules::{EnvEdit, EnvValueFrom};

/// The tag the Deployment names until the first build.
const PENDING_TAG: &str = "pending";
/// The webhook Keel is told of a new image at.
const KEEL_WEBHOOK: &str = "http://keel.keel.svc.cluster.local:9300/v1/webhooks/native";
/// The longest upload token request.
const REQUEST_MAX: usize = 63;
/// The ref Zeabur Builder takes for the repository's default branch.
const DEFAULT_BRANCH_REF: &str = "HEAD";
/// The Keel policy that leaves a workload alone.
const KEEL_NEVER: &str = "never";
/// A branch as the form takes it, for the API's description. Kubernetes' regular expressions
/// have no lookahead, so a branch ending in `.lock` is refused by the server only.
pub const BRANCH_PATTERN: &str = r"^(\.?[A-Za-z0-9_-])+\.?(/(\.?[A-Za-z0-9_-])+\.?)*$";

/// One `Build`'s settings, whatever its source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct BuildInput {
    /// What it builds.
    pub source: BuildSource,
    /// How the folder is built.
    pub method: BuildMethod,
    /// The folder inside the repository or upload to build. Left out: its root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(
        feature = "openapi",
        schema(
            max_length = 1024,
            pattern = "^([A-Za-z0-9_@+-][A-Za-z0-9._@+-]*(/[A-Za-z0-9_@+-][A-Za-z0-9._@+-]*)*)?$"
        )
    )]
    pub directory: Option<String>,
    /// The Dockerfile, relative to the directory, for [`BuildMethod::Dockerfile`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(
        feature = "openapi",
        schema(
            max_length = 1024,
            pattern = "^([A-Za-z0-9_@+-][A-Za-z0-9._@+-]*(/[A-Za-z0-9_@+-][A-Za-z0-9._@+-]*)*)?$"
        )
    )]
    pub dockerfile: Option<String>,
    /// Variables of the build, kept in the Secret `<name>-build-env`. A `Build` that exists keeps
    /// the variables it has; these are set beside them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build_variables: Vec<NameValue>,
}

/// What a `Build` builds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BuildSource {
    /// A Git repository.
    #[serde(rename_all = "camelCase")]
    Git {
        /// The repository: `https://host/owner/repo(.git)`, `git@host:owner/repo.git` or
        /// `ssh://git@host/owner/repo.git`.
        url: String,
        /// The branch. Left out: the repository's default branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(
            feature = "openapi",
            schema(pattern = r"^(\.?[A-Za-z0-9_-])+\.?(/(\.?[A-Za-z0-9_-])+\.?)*$")
        )]
        branch: Option<String>,
        /// How Zeabur Builder reads the repository.
        access: GitAccess,
        /// The deploy key the API makes for [`GitAccess::DeployKey`], before the planner sees
        /// the change.
        #[serde(skip)]
        #[cfg_attr(feature = "openapi", schema(ignore))]
        deploy_key: Option<DeployKeySecret>,
    },
    /// A folder the client uploads after the plan.
    #[serde(rename_all = "camelCase")]
    Upload {
        /// A value the `Build` has not been asked with before, for its upload token (see
        /// [`crate::Change::UploadToken`]).
        request: String,
    },
}

/// A [`crate::Change::CreateBuiltService`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct BuiltServiceChange {
    /// The service's name, which its `Build`, workload and Service share.
    #[cfg_attr(
        feature = "openapi",
        schema(max_length = 40, pattern = "^[a-z]([-a-z0-9]*[a-z0-9])?$")
    )]
    pub name: String,
    /// The port the app listens on. Left out: a worker, which takes no connections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// A public hostname for the port, with HTTPS where the cluster has an issuer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Variables of the app, kept in the Secret `<name>-env`. `PORT` is added from `port`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<NameValue>,
    /// What the app asks for. Left out: small.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<ServiceSize>,
    /// `restricted` or `baseline`. Left out: `restricted` for Railpack, whose images run as a
    /// regular user, and `baseline` for a Dockerfile, which often runs as root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security: Option<PodSecurityLevel>,
    /// Its `Build`.
    pub build: BuildInput,
}

/// A [`crate::Change::SetBuild`]. The `Build` is named after the workload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SetBuildChange {
    /// The service: a Deployment, StatefulSet or DaemonSet.
    pub workload: WorkloadRef,
    /// The container that runs what the `Build` makes; may be left out when the workload has
    /// only one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// The `Build`.
    pub build: BuildInput,
}

/// How Zeabur Builder reads a repository. Credentials go in the Secret `<name>-git`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GitAccess {
    /// A public repository, over HTTPS.
    Public,
    /// A user name and an access token, over HTTPS.
    #[serde(rename_all = "camelCase")]
    Token {
        /// The user name the host takes with a token.
        username: String,
        /// The token.
        token: String,
    },
    /// An SSH deploy key the apply makes, over SSH. The person adds its public key to the
    /// repository on the Git host.
    #[serde(rename_all = "camelCase")]
    DeployKey {
        /// The host's `known_hosts` lines, as `ssh-keyscan` prints them; needed for any host but
        /// `github.com` and `gitlab.com`, whose keys Zeabur pins.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        known_hosts: Option<String>,
    },
    /// The cluster's GitHub App, through an installation granted to the project.
    #[serde(rename_all = "camelCase")]
    GithubApp {
        /// The installation.
        installation_id: u64,
    },
}

impl std::fmt::Debug for GitAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Public => f.write_str("Public"),
            Self::Token { username, .. } => f
                .debug_struct("Token")
                .field("username", username)
                .finish_non_exhaustive(),
            Self::DeployKey { .. } => f.write_str("DeployKey"),
            Self::GithubApp { installation_id } => f
                .debug_struct("GithubApp")
                .field("installation_id", installation_id)
                .finish(),
        }
    }
}

/// The values of a deploy key's Secret.
#[derive(Clone, PartialEq, Eq)]
pub struct DeployKeySecret {
    /// The private key, or a stand-in for a plan.
    pub private_key: String,
    /// The host keys the key trusts, as `known_hosts` lines.
    pub known_hosts: String,
}

impl std::fmt::Debug for DeployKeySecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DeployKeySecret")
    }
}

/// A repository's host, with `:port` when it names one, and its path without `.git`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repository {
    /// The host.
    pub host: String,
    /// `owner/repo`, or deeper for GitLab groups.
    pub path: String,
}

fn segment(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn git_host(host: &str) -> bool {
    let (name, port) = host
        .split_once(':')
        .map_or((host, None), |(name, port)| (name, Some(port)));
    let bytes = name.as_bytes();
    let edge = |b: Option<&u8>| b.is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    edge(bytes.first())
        && edge(bytes.last())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'.'))
        && port.is_none_or(|p| (1..=5).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
}

/// The host and path of `https://host/owner/repo(.git)`, `git@host:owner/repo.git` or
/// `ssh://git@host/owner/repo.git`; nothing for anything else.
#[must_use]
pub fn parse_repository(url: &str) -> Option<Repository> {
    let url = url.trim();
    let (host, path) = if let Some((scheme, rest)) = url.split_once("://") {
        let scheme = scheme.to_ascii_lowercase();
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let host = match (scheme.as_str(), authority.rsplit_once('@')) {
            ("ssh", Some((_, host))) => host,
            ("ssh" | "https" | "http", None) => authority,
            _ => return None,
        };
        (host, path)
    } else {
        let (user, rest) = url.split_once('@')?;
        let (host, path) = rest.split_once(':')?;
        if !segment(user) || host.contains('/') || path.starts_with('/') {
            return None;
        }
        (host, path)
    };
    let host = host.to_ascii_lowercase();
    let path = path.trim_start_matches('/').trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let segments: Vec<&str> = path.split('/').collect();
    (git_host(&host) && segments.len() >= 2 && segments.iter().all(|s| segment(s))).then(|| {
        Repository {
            host,
            path: path.to_owned(),
        }
    })
}

fn over_https(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    url.starts_with("https:") || url.starts_with("http:")
}

fn without_port(host: &str) -> &str {
    host.split_once(':').map_or(host, |(name, _)| name)
}

/// The SSH host a deploy key for `url` is for, with `:port` from an `ssh://` URL.
#[must_use]
pub fn deploy_key_host(url: &str) -> Option<String> {
    let repository = parse_repository(url)?;
    Some(if over_https(url) {
        without_port(&repository.host).to_owned()
    } else {
        repository.host
    })
}

/// Whether `branch` is a branch the form takes: [`BRANCH_PATTERN`], and not ending in `.lock`.
fn is_branch(branch: &str) -> bool {
    let lock = branch
        .rsplit_once('.')
        .is_some_and(|(_, extension)| extension == "lock");
    !lock
        && branch.split('/').all(|part| {
            part.bytes().any(|b| b != b'.')
                && !part.contains("..")
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        })
}

fn trimmed(path: Option<&str>) -> Option<String> {
    let path = path?
        .trim()
        .trim_start_matches("./")
        .trim_start_matches('/')
        .trim_end_matches('/');
    (!path.is_empty()).then(|| path.to_owned())
}

fn check_request(request: &str) -> Result<(), PlanError> {
    if request.is_empty()
        || request.len() > REQUEST_MAX
        || !request
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err(refused(
            "invalid_request",
            "The upload request must be 1 to 63 letters, digits, dots, dashes or underscores.",
            "build.source.request",
        ));
    }
    Ok(())
}

fn check_git(url: &str, branch: Option<&str>, access: &GitAccess) -> Result<Repository, PlanError> {
    let repository = parse_repository(url).ok_or_else(|| {
        refused(
            "invalid_repository",
            "Use a repository URL like https://github.com/acme/web.",
            "build.source.url",
        )
    })?;
    if let Some(branch) = branch.map(str::trim)
        && !branch.is_empty()
        && !is_branch(branch)
    {
        return Err(refused(
            "invalid_branch",
            "A branch is letters, digits, dots, dashes, underscores and slashes.",
            "build.source.branch",
        ));
    }
    if let GitAccess::Token { username, token } = access
        && (username.trim().is_empty() || token.trim().is_empty())
    {
        return Err(refused(
            "invalid_token",
            "A token needs the user name the host takes with it.",
            "build.source.access",
        ));
    }
    Ok(repository)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Build {
    api_version: String,
    kind: &'static str,
    metadata: ObjectMeta,
    spec: BuildSpec,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildSpec {
    builder_ref: &'static str,
    source: SourceSpec,
    output: BuildOutput,
    notify: [BuildNotify; 1],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    env: Vec<EnvVar>,
}

/// `spec.source`, as Zeabur Builder's `SourceSpec` (`crates/zeabur-builder-api/src/build.rs` at
/// the tag in `xtask/pins/zeabur-builder`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceSpec {
    #[serde(flatten)]
    kind: SourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    context_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dockerfile: Option<String>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum SourceKind {
    Git { git: GitSource },
    Archive { archive: ArchiveSource },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitSource {
    url: String,
    #[serde(rename = "ref")]
    reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    credentials_ref: Option<NameRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    github_app: Option<GitHubApp>,
}

/// An uploaded archive. The client uploads it after the plan and names its digest then.
#[derive(Serialize)]
struct ArchiveSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    digest: Option<String>,
}

#[derive(Serialize)]
struct NameRef {
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitHubApp {
    installation_id: u64,
}

#[derive(Serialize)]
struct BuildOutput {
    image: String,
}

#[derive(Serialize)]
struct BuildNotify {
    #[serde(rename = "type")]
    kind: &'static str,
    url: &'static str,
}

/// The fields of a `Build` the plan reads.
#[derive(Default, Deserialize)]
#[serde(default)]
struct BuildRead {
    spec: SpecRead,
    status: StatusRead,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct SpecRead {
    source: SourceRead,
    output: OutputRead,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct SourceRead {
    #[serde(rename = "type")]
    kind: String,
    git: Option<GitRead>,
    archive: Option<ArchiveRead>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct GitRead {
    credentials_ref: Option<NameRead>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct NameRead {
    name: String,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct ArchiveRead {
    digest: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct OutputRead {
    image: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct StatusRead {
    latest_image: Option<String>,
}

impl BuildRead {
    /// The repository it pushes to: `spec.output.image`, else its newest image's.
    fn repository(&self) -> Option<&str> {
        self.spec
            .output
            .image
            .as_deref()
            .filter(|i| !i.is_empty())
            .or_else(|| self.status.latest_image.as_deref().map(repository_of))
    }

    /// The Secret it reads the repository's credentials from.
    fn credentials(&self) -> Option<&str> {
        self.spec
            .source
            .git
            .as_ref()
            .and_then(|g| g.credentials_ref.as_ref())
            .map(|r| r.name.as_str())
    }

    /// Whether a container of `spec`, init containers too, runs what it pushes.
    fn feeds(&self, spec: Option<&k8s_openapi::api::core::v1::PodSpec>) -> bool {
        let Some(repository) = self.repository() else {
            return false;
        };
        spec.into_iter()
            .flat_map(|s| s.init_containers.iter().flatten().chain(&s.containers))
            .any(|c| c.image.as_deref().is_some_and(|i| runs_from(i, repository)))
    }
}

/// The Secret Zeabur makes for a `Build`'s repository credentials.
fn credentials_name(build: &str) -> String {
    format!("{build}-git")
}

/// The Secret that holds a `Build`'s variables.
fn variables_name(build: &str) -> String {
    format!("{build}-build-env")
}

fn secrets() -> ResourceRef {
    ResourceRef {
        group: String::new(),
        version: "v1".to_owned(),
        resource: "secrets".to_owned(),
    }
}

/// The credentials Secret `<name>-git` for `access`, if it needs one, and the Git source that
/// reads `repository` with them.
fn credentials(
    name: &str,
    url: &str,
    branch: Option<&str>,
    access: &GitAccess,
    deploy_key: Option<&DeployKeySecret>,
    repository: &Repository,
) -> Result<(Option<Secret>, GitSource), PlanError> {
    let secret_name = credentials_name(name);
    let mut labels = labels_of(name);
    labels.insert(GIT_CREDENTIALS_LABEL.to_owned(), "true".to_owned());
    let secret = |kind: &str, values: [(&str, &str); 2]| Secret {
        metadata: ObjectMeta {
            name: Some(secret_name.clone()),
            labels: Some(labels.clone()),
            ..ObjectMeta::default()
        },
        type_: Some(kind.to_owned()),
        string_data: Some(
            values
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        ),
        ..Secret::default()
    };
    let https = format!("https://{}/{}.git", repository.host, repository.path);
    let (secret, url, github_app) = match access {
        GitAccess::Public => (None, https, None),
        GitAccess::GithubApp { installation_id } => (
            None,
            https,
            Some(GitHubApp {
                installation_id: *installation_id,
            }),
        ),
        GitAccess::Token { username, token } => (
            Some(secret(
                BASIC_AUTH,
                [("username", username.trim()), ("password", token.trim())],
            )),
            https,
            None,
        ),
        GitAccess::DeployKey { .. } => {
            let Some(key) = deploy_key else {
                return Err(error(
                    ErrorKind::BadRequest,
                    "unresolved_deploy_key",
                    "The API makes a deploy key before planning; this change has none.",
                ));
            };
            let ssh = if over_https(url) {
                format!(
                    "git@{}:{}.git",
                    without_port(&repository.host),
                    repository.path
                )
            } else {
                url.trim().to_owned()
            };
            (
                Some(secret(
                    SSH_AUTH,
                    [
                        (SSH_PRIVATE_KEY, key.private_key.as_str()),
                        ("known_hosts", key.known_hosts.as_str()),
                    ],
                )),
                ssh,
                None,
            )
        }
    };
    let source = GitSource {
        url,
        reference: branch
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .unwrap_or(DEFAULT_BRANCH_REF)
            .to_owned(),
        credentials_ref: secret.as_ref().map(|_| NameRef { name: secret_name }),
        github_app,
    };
    Ok((secret, source))
}

const BASIC_AUTH: &str = "kubernetes.io/basic-auth";
const SSH_AUTH: &str = "kubernetes.io/ssh-auth";
const SSH_PRIVATE_KEY: &str = "ssh-privatekey";

impl BuildInput {
    /// Refuses what the `Build` would refuse.
    fn check(&self) -> Result<(), PlanError> {
        for (field, path) in [
            ("build.directory", self.directory.as_deref()),
            ("build.dockerfile", self.dockerfile.as_deref()),
        ] {
            if let Some(path) = trimmed(path)
                && !build_path(&path)
            {
                return Err(refused(
                    "invalid_path",
                    "A path in the build has no segment that starts with a dot.",
                    field,
                ));
            }
        }
        check_variable_names(
            self.build_variables.iter().map(|v| v.name.as_str()),
            "build.buildVariables",
        )?;
        match &self.source {
            BuildSource::Upload { request } => check_request(request),
            BuildSource::Git {
                url,
                branch,
                access,
                ..
            } => check_git(url, branch.as_deref(), access).map(|_| ()),
        }
    }

    const fn builder_ref(&self) -> &'static str {
        match self.method {
            BuildMethod::Railpack => "railpack",
            BuildMethod::Dockerfile => "dockerfile",
        }
    }

    /// `spec.source` of the `Build` `name`, and the credentials Secret it reads, if any.
    fn source(&self, name: &str) -> Result<(SourceSpec, Option<Secret>), PlanError> {
        let (kind, secret) = match &self.source {
            BuildSource::Upload { .. } => (
                SourceKind::Archive {
                    archive: ArchiveSource { digest: None },
                },
                None,
            ),
            BuildSource::Git {
                url,
                branch,
                access,
                deploy_key,
            } => {
                let repository = check_git(url, branch.as_deref(), access)?;
                let (secret, git) = credentials(
                    name,
                    url,
                    branch.as_deref(),
                    access,
                    deploy_key.as_ref(),
                    &repository,
                )?;
                (SourceKind::Git { git }, secret)
            }
        };
        let spec = SourceSpec {
            kind,
            context_dir: trimmed(self.directory.as_deref()),
            dockerfile: (self.method == BuildMethod::Dockerfile)
                .then(|| trimmed(self.dockerfile.as_deref()))
                .flatten(),
        };
        Ok((spec, secret))
    }

    /// The new `Build` `name` that builds `source` and pushes to `repository`, its variables read
    /// from `<name>-build-env`, linked to `details` (see [`DETAILS_URL`]).
    fn build(
        &self,
        name: &str,
        repository: String,
        source: SourceSpec,
        details: Option<String>,
    ) -> Build {
        let secret = variables_name(name);
        let env = self
            .build_variables
            .iter()
            .map(|v| EnvVar {
                name: v.name.clone(),
                value: None,
                value_from: Some(EnvVarSource {
                    secret_key_ref: Some(SecretKeySelector {
                        name: secret.clone(),
                        key: v.name.clone(),
                        optional: None,
                    }),
                    ..EnvVarSource::default()
                }),
            })
            .collect();
        let annotations: BTreeMap<String, String> = match &self.source {
            BuildSource::Upload { request } => {
                Some((UPLOAD_TOKEN_REQUEST.to_owned(), request.clone()))
            }
            BuildSource::Git { .. } => None,
        }
        .into_iter()
        .chain(details.map(|url| (DETAILS_URL.to_owned(), url)))
        .collect();
        Build {
            api_version: format!("{BUILDER_GROUP}/{BUILDER_VERSION}"),
            kind: "Build",
            metadata: ObjectMeta {
                name: Some(name.to_owned()),
                labels: Some(labels_of(name)),
                annotations: (!annotations.is_empty()).then_some(annotations),
                ..ObjectMeta::default()
            },
            spec: BuildSpec {
                builder_ref: self.builder_ref(),
                source,
                output: BuildOutput { image: repository },
                notify: [BuildNotify {
                    kind: "keel",
                    url: KEEL_WEBHOOK,
                }],
                env,
            },
        }
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl<'a> Planner<'a> {
    /// The repository Zeabur Builder pushes the `Build` `name` to.
    fn repository(&self, name: &str) -> Result<String, PlanError> {
        let registry = self.input.build_registry.filter(|r| !r.is_empty()).ok_or_else(|| {
            error(
                ErrorKind::Invalid,
                "no_registry",
                "Zeabur has no registry for Zeabur Builder to push to. Ask an administrator to set builds.registry.",
            )
        })?;
        let project = &self.input.snapshot.namespace;
        Ok(format!(
            "{}/{project}/{name}",
            registry.trim_end_matches('/')
        ))
    }

    /// The Builds tab of the service `kind`/`name` in the project, when Zeabur has a public URL.
    fn builds_tab(&self, kind: WorkloadKind, name: &str) -> Option<String> {
        self.input.public_url.map(|base| {
            format!(
                "{}/projects/{}/services/{}/{name}?tab=builds",
                base.trim_end_matches('/'),
                self.input.snapshot.namespace,
                kind.as_str(),
            )
        })
    }

    /// Sets [`DETAILS_URL`] of the `Build` at `at` to `url`, unless it has that value.
    fn link_build(&mut self, at: usize, url: &str) -> Result<(), PlanError> {
        let current = self.current(at);
        let path = pointer(&["metadata", "annotations", DETAILS_URL]);
        let before = current.pointer(&path).cloned();
        if before.as_ref().and_then(Value::as_str) == Some(url) {
            return Ok(());
        }
        self.claim(
            at,
            &steps("annotations", DETAILS_URL),
            &shown("annotations", DETAILS_URL),
        )?;
        let operations = before.map_or_else(
            || set_key(&current, "annotations", DETAILS_URL, url),
            |before| vec![test(&path, before), replace(&path, json!(url))],
        );
        self.patch(at, operations)
    }

    pub(crate) fn create_built_service(
        &mut self,
        change: &BuiltServiceChange,
        index: usize,
    ) -> Result<(), PlanError> {
        let name = change.name.as_str();
        check_name(name)?;
        check_host(change.host.as_deref(), change.port)?;
        change.build.check()?;
        let repository = self.repository(name)?;
        let (source, credentials) = change.build.source(name)?;
        if let Some(secret) = credentials {
            self.create_object(&to_json(&secret)?, index)?;
        }
        self.build_variables(name, &change.build.build_variables, false, index)?;
        let mut variables = change.variables.clone();
        if let Some(port) = change.port
            && !variables.iter().any(|v| v.name == "PORT")
        {
            variables.push(NameValue {
                name: "PORT".to_owned(),
                value: port.to_string(),
            });
        }
        let service = ImageServiceChange {
            name: name.to_owned(),
            image: format!("{repository}:{PENDING_TAG}"),
            port: change.port,
            more_ports: Vec::new(),
            host: change.host.clone(),
            variables: plain_variables(&variables),
            secret_variables: true,
            volumes: Vec::new(),
            size: change.size.unwrap_or(ServiceSize::Small),
            replicas: 1,
            registry: None,
            security: change.security.unwrap_or(match change.build.method {
                BuildMethod::Railpack => PodSecurityLevel::Restricted,
                BuildMethod::Dockerfile => PodSecurityLevel::Baseline,
            }),
            command: Vec::new(),
            args: Vec::new(),
            wait_for: None,
        };
        let keel = BTreeMap::from([(KEEL_POLICY.to_owned(), KEEL_FORCE.to_owned())]);
        for object in image_objects(&service, self.input.cluster_issuer, Some(keel))? {
            self.create_object(&object, index)?;
        }
        let details = self.builds_tab(WorkloadKind::Deployment, name);
        let build = change.build.build(name, repository, source, details);
        self.create_object(&to_json(&build)?, index)
    }

    pub(crate) fn set_build(
        &mut self,
        change: &SetBuildChange,
        index: usize,
    ) -> Result<(), PlanError> {
        let reference = &change.workload;
        let name = reference.name.as_str();
        change.build.check()?;
        let found = self.built_workload(reference)?;
        let container = Self::container(found, reference, change.container.as_deref())?;
        // Keel takes new builds only with `force` alone: a same-tag policy or a poll schedule
        // would keep the pending tag.
        let keel =
            found.meta().annotations.as_ref().is_some_and(|a| {
                ImageUpdates::read(a, []).setting == Some(UpdateSetting::NewBuilds)
            });
        let (source, credentials) = change.build.source(name)?;
        let current = self
            .builds_read()?
            .into_iter()
            .find_map(|(n, read)| (n == name).then_some(read));
        let existing = current.is_some();
        // The image it made last, which the container runs until the next build.
        let latest = current.and_then(|read| read.status.latest_image);
        let details = self.builds_tab(reference.kind, name);
        let (pushes_to, build_changed) = if existing {
            self.update_build(
                name,
                &change.build,
                source,
                credentials,
                details.as_deref(),
                index,
            )?
        } else {
            let pushes_to = self.repository(name)?;
            if let Some(secret) = credentials {
                self.create_object(&to_json(&secret)?, index)?;
            }
            self.build_variables(name, &change.build.build_variables, false, index)?;
            let build = change.build.build(name, pushes_to.clone(), source, details);
            self.create_object(&to_json(&build)?, index)?;
            (pushes_to, true)
        };
        let moves = !container
            .spec
            .image
            .as_deref()
            .is_some_and(|i| runs_from(i, &pushes_to));
        if moves {
            let image = latest
                .filter(|i| runs_from(i, &pushes_to))
                .unwrap_or_else(|| format!("{pushes_to}:{PENDING_TAG}"));
            self.image(reference, Some(&container.name), &image)?;
        }
        let new_builds = Update::NewBuilds {
            build: name.to_owned(),
            webhook: KEEL_WEBHOOK.to_owned(),
        };
        match (keel, existing) {
            (true, _) => {}
            (false, true) => {
                self.auto_update(&AutoUpdateTarget::Workload(reference.clone()), &new_builds)?;
            }
            // The new `Build` already tells Keel of its images.
            (false, false) => {
                self.write_keel(reference, &new_builds)?;
            }
        }
        if !build_changed && !moves && keel {
            return Err(error(
                ErrorKind::BadRequest,
                "unchanged",
                format!("{name} already builds this way."),
            ));
        }
        Ok(())
    }

    /// Gives the `Build` `name` that exists `build`'s source, method and paths, writing the
    /// credentials it reads, and links it to `details`; returns the repository it pushes to and
    /// whether it changes anything.
    fn update_build(
        &mut self,
        name: &str,
        build: &BuildInput,
        mut source: SourceSpec,
        credentials: Option<Secret>,
        details: Option<&str>,
        index: usize,
    ) -> Result<(String, bool), PlanError> {
        let at = self.build(name)?;
        let before = self.current(at);
        let read: BuildRead = serde_json::from_value(before.clone())
            .map_err(|e| error(ErrorKind::Invalid, "unreadable_object", e.to_string()))?;
        let reads = read.credentials() == Some(credentials_name(name).as_str());
        let wrote_credentials = self.write_credentials(name, credentials, reads, index)?;
        let Some(pushes_to) = read.spec.output.image.clone().filter(|i| !i.is_empty()) else {
            return Err(error(
                ErrorKind::Invalid,
                "no_output",
                format!("Build {name} names no image to push."),
            ));
        };
        // A folder uploaded again keeps building the one it has until the new one is named.
        if let SourceKind::Archive { archive } = &mut source.kind
            && read.spec.source.kind == "archive"
        {
            archive.digest = read.spec.source.archive.and_then(|a| a.digest);
        }
        let mut operations = Vec::new();
        for (field, value) in [
            ("source", to_json(&source)?),
            ("builderRef", json!(build.builder_ref())),
        ] {
            let path = format!("/spec/{field}");
            let was = before.pointer(&path).cloned();
            if was.as_ref() == Some(&value) {
                continue;
            }
            self.claim(
                at,
                &[Step::field("spec"), Step::field(field)],
                &format!("spec.{field}"),
            )?;
            match was {
                Some(was) => operations.extend([test(&path, was), replace(&path, value)]),
                None => operations.push(add(&path, value)),
            }
        }
        self.patch(at, operations)?;
        if let Some(url) = details {
            self.link_build(at, url)?;
        }
        if let BuildSource::Upload { request } = &build.source {
            self.upload_token(name, request)?;
        }
        if !build.build_variables.is_empty() {
            let secret = variables_name(name);
            let reads = before
                .pointer("/spec/env")
                .and_then(Value::as_array)
                .is_some_and(|env| {
                    env.iter().any(|e| {
                        e.pointer("/valueFrom/secretKeyRef/name")
                            .and_then(Value::as_str)
                            == Some(secret.as_str())
                    })
                });
            self.build_variables(name, &build.build_variables, reads, index)?;
            for variable in &build.build_variables {
                let edit = EnvEdit {
                    op: "set".to_owned(),
                    name: variable.name.clone(),
                    value: None,
                    value_from: Some(EnvValueFrom {
                        secret_name: Some(secret.clone()),
                        config_map_name: None,
                        key: Some(variable.name.clone()),
                        field_path: None,
                    }),
                };
                self.build_env(at, name, &edit)?;
            }
        }
        let changed =
            wrote_credentials || !build.build_variables.is_empty() || self.current(at) != before;
        Ok((pushes_to, changed))
    }

    /// Writes the credentials Secret `<name>-git` the `Build` reads from now on, or deletes it when
    /// it reads none and no other `Build` reads it. A deploy key stays, so the key the person added
    /// to the Git host keeps working. A Secret of that name the `Build` does not read (`reads`) is
    /// not its own: it is left alone, and credentials are refused. Returns whether it writes the
    /// Secret.
    fn write_credentials(
        &mut self,
        name: &str,
        wanted: Option<Secret>,
        reads: bool,
        index: usize,
    ) -> Result<bool, PlanError> {
        let secret_name = credentials_name(name);
        let existing = self
            .input
            .snapshot
            .secret(&secret_name)
            .map(|s| s.type_.clone().unwrap_or_default());
        match (wanted, existing) {
            (Some(secret), None) => {
                self.create_object(&to_json(&secret)?, index)?;
                Ok(true)
            }
            (Some(_), Some(_)) if !reads => Err(refused(
                "credentials_taken",
                &format!(
                    "Secret {secret_name} exists and Build {name} does not read it. Rename or delete the Secret, then connect the repository again."
                ),
                "build.source.access",
            )),
            (None, Some(_)) if !reads => Ok(false),
            (Some(secret), Some(kind)) if secret.type_.as_deref() == Some(kind.as_str()) => {
                let edits: Vec<DataEdit> = secret
                    .string_data
                    .into_iter()
                    .flatten()
                    .filter(|(key, _)| key != SSH_PRIVATE_KEY)
                    .map(|(key, value)| DataEdit {
                        op: "set".to_owned(),
                        key,
                        value: Some(value),
                    })
                    .collect();
                self.data(DataKind::Secret, &secret_name, &edits)?;
                Ok(true)
            }
            (Some(_), Some(kind)) => Err(refused(
                "credentials_kind_changed",
                &format!(
                    "Secret {secret_name} holds {kind} credentials. Stop building from source, then connect the repository again."
                ),
                "build.source.access",
            )),
            (None, Some(_)) => {
                let others = self.builds_read()?;
                if others
                    .iter()
                    .any(|(other, b)| other != name && b.credentials() == Some(&secret_name))
                {
                    return Ok(false);
                }
                self.delete(&secrets(), &secret_name, None)?;
                Ok(true)
            }
            (None, None) => Ok(false),
        }
    }

    /// Sets `variables` in the Secret `<name>-build-env` when the `Build` already reads it
    /// (`reads`), else creates it: a Secret of that name the `Build` does not read is not its own,
    /// and the create fails on it rather than change it.
    fn build_variables(
        &mut self,
        name: &str,
        variables: &[NameValue],
        reads: bool,
        index: usize,
    ) -> Result<(), PlanError> {
        if variables.is_empty() {
            return Ok(());
        }
        let secret = variables_name(name);
        if reads {
            let edits: Vec<DataEdit> = variables
                .iter()
                .map(|v| DataEdit {
                    op: "set".to_owned(),
                    key: v.name.clone(),
                    value: Some(v.value.clone()),
                })
                .collect();
            return self.data(DataKind::Secret, &secret, &edits);
        }
        let values = variables
            .iter()
            .map(|v| (v.name.as_str(), v.value.as_str()));
        let object = opaque_secret(&secret, &labels_of(name), values);
        self.create_object(&to_json(&object)?, index)
    }

    /// The `Build`s the plan read, by name.
    fn builds_read(&self) -> Result<Vec<(String, BuildRead)>, PlanError> {
        let resource = builds();
        self.input
            .objects
            .iter()
            .filter(|f| f.resource == resource)
            .map(|f| {
                serde_json::from_value(f.object.clone())
                    .map(|read| (f.name.clone(), read))
                    .map_err(|e| error(ErrorKind::Invalid, "unreadable_object", e.to_string()))
            })
            .collect()
    }

    /// The long-running service a `Build` feeds. A Job or CronJob runs to completion, so it has
    /// none.
    fn built_workload(&self, reference: &WorkloadRef) -> Result<Workload<'a>, PlanError> {
        let found = self.editable_workload(reference)?;
        if matches!(found.kind(), WorkloadKind::Job | WorkloadKind::CronJob) {
            return Err(error(
                ErrorKind::Invalid,
                "not_updatable",
                format!(
                    "{} {} runs to completion; only a long-running service is built.",
                    reference.kind.as_str(),
                    reference.name,
                ),
            ));
        }
        Ok(found)
    }

    pub(crate) fn remove_build(&mut self, reference: &WorkloadRef) -> Result<(), PlanError> {
        let name = reference.name.as_str();
        let found = self.built_workload(reference)?;
        let mut others = self.builds_read()?;
        let Some(at) = others.iter().position(|(n, _)| n == name) else {
            return Err(error(
                ErrorKind::NotFound,
                "unknown_build",
                format!("There is no Build named {name}."),
            ));
        };
        let (_, build) = others.swap_remove(at);
        let Some(repository) = build.repository().map(str::to_owned) else {
            return Err(error(
                ErrorKind::Invalid,
                "no_output",
                format!("Build {name} names no image to push."),
            ));
        };
        self.delete(&builds(), name, None)?;
        let secret = credentials_name(name);
        if build.credentials() == Some(secret.as_str())
            && self.input.snapshot.secret(&secret).is_some()
            && !others
                .iter()
                .any(|(_, other)| other.credentials() == Some(secret.as_str()))
        {
            self.delete(&secrets(), &secret, None)?;
        }
        let spec = found.pod_spec();
        let latest = build.status.latest_image.as_deref();
        for container in spec.into_iter().flat_map(|s| &s.containers) {
            let Some(image) = container.image.as_deref() else {
                continue;
            };
            if !runs_from(image, &repository) {
                continue;
            }
            let pinned = pinned(image, latest);
            if pinned != image {
                self.image(reference, Some(&container.name), pinned)?;
            }
        }
        let keel = found
            .meta()
            .annotations
            .as_ref()
            .and_then(|a| a.get(KEEL_POLICY))
            .is_some_and(|p| p != KEEL_NEVER);
        if keel && !others.iter().any(|(_, other)| other.feeds(spec)) {
            self.auto_update(&AutoUpdateTarget::Workload(reference.clone()), &Update::Off)?;
        }
        Ok(())
    }
}

/// Whether `image` is in `repository`, whatever its tag or digest.
fn runs_from(image: &str, repository: &str) -> bool {
    repository_of(image).eq_ignore_ascii_case(repository)
}

/// An image's tag, if it names one.
fn tag_of(image: &str) -> Option<&str> {
    let name = image.split_once('@').map_or(image, |(name, _)| name);
    name.rsplit_once(':')
        .map(|(_, tag)| tag)
        .filter(|tag| !tag.contains('/'))
}

/// The image a container keeps once nothing builds it: the `Build`'s newest, `latest`, with its
/// digest when the container runs that tag; else what it runs.
fn pinned<'a>(image: &'a str, latest: Option<&'a str>) -> &'a str {
    match latest {
        Some(latest)
            if !image.contains('@') && tag_of(image).is_some_and(|t| tag_of(latest) == Some(t)) =>
        {
            latest
        }
        _ => image,
    }
}

/// An image's repository: the reference without its digest or tag.
fn repository_of(image: &str) -> &str {
    let name = image.split_once('@').map_or(image, |(name, _)| name);
    match name.rsplit_once(':') {
        Some((repository, tag)) if !tag.contains('/') => repository,
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_branch_rule_is_the_described_pattern_and_no_lock_file() {
        let pattern = regex::Regex::new(BRANCH_PATTERN).unwrap();
        for branch in [
            "main",
            "release/1.4",
            "feature/a.b_c-d",
            "/main",
            "a//b",
            "a/",
            "a..b",
            "a b",
            ".",
        ] {
            assert_eq!(is_branch(branch), pattern.is_match(branch), "{branch}");
        }
        assert!(pattern.is_match("main.lock"));
        assert!(!is_branch("main.lock"));
    }

    #[test]
    fn a_repository_reads_from_each_url_form() {
        let repo = |host: &str, path: &str| {
            Some(Repository {
                host: host.to_owned(),
                path: path.to_owned(),
            })
        };
        assert_eq!(
            parse_repository("https://GitHub.com/acme/web.git/"),
            repo("github.com", "acme/web")
        );
        assert_eq!(
            parse_repository("git@gitlab.com:group/sub/web.git"),
            repo("gitlab.com", "group/sub/web")
        );
        assert_eq!(
            parse_repository("ssh://git@git.example.com:2222/acme/web.git"),
            repo("git.example.com:2222", "acme/web")
        );
        for bad in [
            "https://user:pw@github.com/acme/web",
            "ftp://github.com/acme/web",
            "https://github.com/acme",
            "git@github.com:/acme/web",
            "github.com/acme/web",
        ] {
            assert_eq!(parse_repository(bad), None, "{bad}");
        }
        assert_eq!(
            deploy_key_host("https://git.example.com:8443/acme/web").as_deref(),
            Some("git.example.com")
        );
        assert_eq!(
            deploy_key_host("ssh://git@git.example.com:2222/acme/web").as_deref(),
            Some("git.example.com:2222")
        );
    }
}
