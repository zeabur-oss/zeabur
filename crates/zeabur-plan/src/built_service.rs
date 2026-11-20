//! A service whose image Zeabur Builder builds: its `Build`, from an uploaded folder or a Git
//! repository, and the objects of the image service that runs what it builds, as one change.
//!
//! The web interface and `zeaburctl up` both send [`crate::Change::CreateUploadService`], and the
//! web interface sends [`crate::Change::CreateGitService`]. The objects are created as a `create`
//! of each is: the credentials and build variables first, the `Build` last. The Deployment names
//! the `Build`'s repository with a tag no build makes, and Keel moves it to each image the
//! `Build` pushes. An uploaded folder's `Build` carries the annotation that asks Zeabur Builder
//! for an upload token.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{EnvVar, EnvVarSource, Secret, SecretKeySelector};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeabur_model::PodSecurityLevel;

use crate::builds::{
    BUILDER_GROUP, BUILDER_VERSION, GIT_CREDENTIALS_LABEL, KEEL_FORCE, KEEL_POLICY,
    UPLOAD_TOKEN_REQUEST, build_path,
};
use crate::change::{BuildMethod, NameValue, ServiceSize, UploadServiceChange};
use crate::image_service::{
    ImageServiceChange, check_host, check_name, check_variable_names, image_objects, labels_of,
    opaque_secret, plain_variables, refused,
};
use crate::plan::{ErrorKind, PlanError, Planner, error, to_json};

/// The tag the Deployment names until the first build.
const PENDING_TAG: &str = "pending";
/// The webhook Keel is told of a new image at.
const KEEL_WEBHOOK: &str = "http://keel.keel.svc.cluster.local:9300/v1/webhooks/native";
/// The longest upload token request.
const REQUEST_MAX: usize = 63;
/// The ref Zeabur Builder takes for the repository's default branch.
const DEFAULT_BRANCH_REF: &str = "HEAD";
/// A branch as the form takes it, for the API's description. Kubernetes' regular expressions
/// have no lookahead, so a branch ending in `.lock` is refused by the server only.
pub const BRANCH_PATTERN: &str = r"^(\.?[A-Za-z0-9_-])+\.?(/(\.?[A-Za-z0-9_-])+\.?)*$";

/// A [`crate::Change::CreateGitService`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct GitServiceChange {
    /// The service's name, which its `Build`, workload and Service share.
    #[cfg_attr(
        feature = "openapi",
        schema(max_length = 40, pattern = "^[a-z]([-a-z0-9]*[a-z0-9])?$")
    )]
    pub name: String,
    /// The repository: `https://host/owner/repo(.git)`, `git@host:owner/repo.git` or
    /// `ssh://git@host/owner/repo.git`.
    pub url: String,
    /// The branch. Left out: the repository's default branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(
        feature = "openapi",
        schema(pattern = r"^(\.?[A-Za-z0-9_-])+\.?(/(\.?[A-Za-z0-9_-])+\.?)*$")
    )]
    pub branch: Option<String>,
    /// How the folder is built.
    pub method: BuildMethod,
    /// The folder inside the repository to build. Left out: its root.
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
    /// The port the app listens on. Left out: a worker, which takes no connections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// A public hostname for the port, with HTTPS where the cluster has an issuer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Variables of the app, kept in the Secret `<name>-env`. `PORT` is added from `port`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<NameValue>,
    /// Variables of the build, kept in the Secret `<name>-build-env`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build_variables: Vec<NameValue>,
    /// What the app asks for. Left out: small.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<ServiceSize>,
    /// `restricted` or `baseline`. Left out: `restricted` for Railpack, whose images run as a
    /// regular user, and `baseline` for a Dockerfile, which often runs as root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security: Option<PodSecurityLevel>,
    /// How Zeabur Builder reads the repository.
    pub access: GitAccess,
    /// The deploy key the API makes for [`GitAccess::DeployKey`], before the planner sees the
    /// change.
    #[serde(skip)]
    #[cfg_attr(feature = "openapi", schema(ignore))]
    pub deploy_key: Option<DeployKeySecret>,
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

/// What every built service checks, whatever its source.
fn check_built(built: &Built<'_>) -> Result<(), PlanError> {
    check_name(built.name)?;
    for (field, path) in [
        ("directory", built.directory.as_deref()),
        ("dockerfile", built.dockerfile.as_deref()),
    ] {
        if let Some(path) = path
            && !build_path(path)
        {
            return Err(refused(
                "invalid_path",
                "A path in the build has no segment that starts with a dot.",
                field,
            ));
        }
    }
    check_host(built.host, built.port)?;
    check_variable_names(
        built.build_variables.iter().map(|v| v.name.as_str()),
        "buildVariables",
    )
}

fn check_upload(change: &UploadServiceChange) -> Result<(), PlanError> {
    if change.request.is_empty()
        || change.request.len() > REQUEST_MAX
        || !change
            .request
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err(refused(
            "invalid_request",
            "The upload request must be 1 to 63 letters, digits, dots, dashes or underscores.",
            "request",
        ));
    }
    Ok(())
}

fn check_git(change: &GitServiceChange) -> Result<Repository, PlanError> {
    let repository = parse_repository(&change.url).ok_or_else(|| {
        refused(
            "invalid_repository",
            "Use a repository URL like https://github.com/acme/web.",
            "url",
        )
    })?;
    if let Some(branch) = change.branch.as_deref().map(str::trim)
        && !branch.is_empty()
        && !is_branch(branch)
    {
        return Err(refused(
            "invalid_branch",
            "A branch is letters, digits, dots, dashes, underscores and slashes.",
            "branch",
        ));
    }
    if let GitAccess::Token { username, token } = &change.access
        && (username.trim().is_empty() || token.trim().is_empty())
    {
        return Err(refused(
            "invalid_token",
            "A token needs the user name the host takes with it.",
            "access",
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
    source: BuildSource,
    output: BuildOutput,
    notify: [BuildNotify; 1],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    env: Vec<EnvVar>,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    archive: Option<Paths>,
    #[serde(skip_serializing_if = "Option::is_none")]
    git: Option<GitSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    credentials_ref: Option<NameRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    github_app: Option<GitHubApp>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Paths {
    #[serde(skip_serializing_if = "Option::is_none")]
    context_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dockerfile: Option<String>,
}

#[derive(Serialize)]
struct GitSource {
    url: String,
    #[serde(rename = "ref")]
    reference: String,
    #[serde(flatten)]
    paths: Paths,
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

/// What a built service takes from its change, whatever the source.
struct Built<'a> {
    name: &'a str,
    method: BuildMethod,
    directory: Option<String>,
    dockerfile: Option<String>,
    port: Option<u16>,
    host: Option<&'a str>,
    variables: &'a [NameValue],
    build_variables: &'a [NameValue],
    size: Option<ServiceSize>,
    security: Option<PodSecurityLevel>,
}

impl<'a> Built<'a> {
    fn of_upload(change: &'a UploadServiceChange) -> Self {
        Self {
            name: &change.name,
            method: change.method,
            directory: trimmed(change.directory.as_deref()),
            dockerfile: trimmed(change.dockerfile.as_deref()),
            port: change.port,
            host: change.host.as_deref(),
            variables: &change.variables,
            build_variables: &change.build_variables,
            size: change.size,
            security: change.security,
        }
    }

    fn of_git(change: &'a GitServiceChange) -> Self {
        Self {
            name: &change.name,
            method: change.method,
            directory: trimmed(change.directory.as_deref()),
            dockerfile: trimmed(change.dockerfile.as_deref()),
            port: change.port,
            host: change.host.as_deref(),
            variables: &change.variables,
            build_variables: &change.build_variables,
            size: change.size,
            security: change.security,
        }
    }

    fn paths(&self) -> Paths {
        Paths {
            context_dir: self.directory.clone(),
            dockerfile: (self.method == BuildMethod::Dockerfile)
                .then(|| self.dockerfile.clone())
                .flatten(),
        }
    }

    /// The objects in project `project`, in the order they are created: `before`, the build
    /// variables' Secret, the image service, and the `Build` from `source`.
    fn objects(
        &self,
        project: &str,
        registry: &str,
        cluster_issuer: Option<&str>,
        before: Vec<Value>,
        source: BuildSource,
        annotations: Option<BTreeMap<String, String>>,
    ) -> Result<Vec<Value>, PlanError> {
        let name = self.name;
        let labels = labels_of(name);
        let repository = format!("{}/{project}/{name}", registry.trim_end_matches('/'));
        let mut variables = self.variables.to_vec();
        if let Some(port) = self.port
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
            port: self.port,
            more_ports: Vec::new(),
            host: self.host.map(str::to_owned),
            variables: plain_variables(&variables),
            secret_variables: true,
            volumes: Vec::new(),
            size: self.size.unwrap_or(ServiceSize::Small),
            replicas: 1,
            registry: None,
            security: self.security.unwrap_or(match self.method {
                BuildMethod::Railpack => PodSecurityLevel::Restricted,
                BuildMethod::Dockerfile => PodSecurityLevel::Baseline,
            }),
            command: Vec::new(),
            args: Vec::new(),
            wait_for: None,
        };
        let build_secret = format!("{name}-build-env");
        let mut out = before;
        if !self.build_variables.is_empty() {
            let values = self
                .build_variables
                .iter()
                .map(|v| (v.name.as_str(), v.value.as_str()));
            out.push(to_json(&opaque_secret(&build_secret, &labels, values))?);
        }
        let keel = BTreeMap::from([(KEEL_POLICY.to_owned(), KEEL_FORCE.to_owned())]);
        out.extend(image_objects(&service, cluster_issuer, Some(keel))?);
        let env = self
            .build_variables
            .iter()
            .map(|v| EnvVar {
                name: v.name.clone(),
                value: None,
                value_from: Some(EnvVarSource {
                    secret_key_ref: Some(SecretKeySelector {
                        name: build_secret.clone(),
                        key: v.name.clone(),
                        optional: None,
                    }),
                    ..EnvVarSource::default()
                }),
            })
            .collect();
        out.push(to_json(&Build {
            api_version: format!("{BUILDER_GROUP}/{BUILDER_VERSION}"),
            kind: "Build",
            metadata: ObjectMeta {
                name: Some(name.to_owned()),
                labels: Some(labels),
                annotations,
                ..ObjectMeta::default()
            },
            spec: BuildSpec {
                builder_ref: match self.method {
                    BuildMethod::Railpack => "railpack",
                    BuildMethod::Dockerfile => "dockerfile",
                },
                source,
                output: BuildOutput { image: repository },
                notify: [BuildNotify {
                    kind: "keel",
                    url: KEEL_WEBHOOK,
                }],
                env,
            },
        })?);
        Ok(out)
    }
}

/// The credentials Secret of `change`, if its access has one, and the source's fields for it.
fn credentials(
    change: &GitServiceChange,
    repository: &Repository,
) -> Result<(Option<Secret>, BuildSource), PlanError> {
    let name = change.name.as_str();
    let secret_name = format!("{name}-git");
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
    let (secret, url, github_app) = match &change.access {
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
                "kubernetes.io/basic-auth",
                [("username", username.trim()), ("password", token.trim())],
            )),
            https,
            None,
        ),
        GitAccess::DeployKey { .. } => {
            let Some(key) = &change.deploy_key else {
                return Err(error(
                    ErrorKind::BadRequest,
                    "unresolved_deploy_key",
                    "The API makes a deploy key before planning; this change has none.",
                ));
            };
            let ssh = if over_https(&change.url) {
                format!(
                    "git@{}:{}.git",
                    without_port(&repository.host),
                    repository.path
                )
            } else {
                change.url.trim().to_owned()
            };
            (
                Some(secret(
                    "kubernetes.io/ssh-auth",
                    [
                        ("ssh-privatekey", key.private_key.as_str()),
                        ("known_hosts", key.known_hosts.as_str()),
                    ],
                )),
                ssh,
                None,
            )
        }
    };
    let built = Built::of_git(change);
    let source = BuildSource {
        git: Some(GitSource {
            url,
            reference: change
                .branch
                .as_deref()
                .map(str::trim)
                .filter(|b| !b.is_empty())
                .unwrap_or(DEFAULT_BRANCH_REF)
                .to_owned(),
            paths: built.paths(),
        }),
        credentials_ref: secret.as_ref().map(|_| NameRef { name: secret_name }),
        github_app,
        ..BuildSource::default()
    };
    Ok((secret, source))
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl Planner<'_> {
    fn registry(&self) -> Result<&str, PlanError> {
        self.input.build_registry.filter(|r| !r.is_empty()).ok_or_else(|| {
            error(
                ErrorKind::Invalid,
                "no_registry",
                "Zeabur has no registry for Zeabur Builder to push to. Ask an administrator to set builds.registry.",
            )
        })
    }

    pub(crate) fn create_upload_service(
        &mut self,
        change: &UploadServiceChange,
        index: usize,
    ) -> Result<(), PlanError> {
        let built = Built::of_upload(change);
        check_built(&built)?;
        check_upload(change)?;
        let registry = self.registry()?;
        let source = BuildSource {
            archive: Some(built.paths()),
            ..BuildSource::default()
        };
        let annotations =
            BTreeMap::from([(UPLOAD_TOKEN_REQUEST.to_owned(), change.request.clone())]);
        let project = self.input.snapshot.namespace.clone();
        let objects = built.objects(
            &project,
            registry,
            self.input.cluster_issuer,
            Vec::new(),
            source,
            Some(annotations),
        )?;
        for object in objects {
            self.create_object(&object, index)?;
        }
        Ok(())
    }

    pub(crate) fn create_git_service(
        &mut self,
        change: &GitServiceChange,
        index: usize,
    ) -> Result<(), PlanError> {
        let built = Built::of_git(change);
        check_built(&built)?;
        let repository = check_git(change)?;
        let registry = self.registry()?;
        let (secret, source) = credentials(change, &repository)?;
        let before = secret
            .as_ref()
            .map(to_json)
            .transpose()?
            .into_iter()
            .collect();
        let project = self.input.snapshot.namespace.clone();
        let objects = built.objects(
            &project,
            registry,
            self.input.cluster_issuer,
            before,
            source,
            None,
        )?;
        for object in objects {
            self.create_object(&object, index)?;
        }
        Ok(())
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
