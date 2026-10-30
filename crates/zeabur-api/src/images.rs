//! What an image says about itself: `POST /projects/{p}/images/inspect` reads its configuration
//! from the registry, so the form that deploys it can offer the port it listens on.
//!
//! Public images are read anonymously. A private one is read with the credentials the person
//! typed, or with a pull Secret of the project, which Zeabur reads as the person; credentials go
//! only to the registry they name. The architectures of the cluster's nodes, where the person
//! may list nodes, choose among an index's platforms; else `amd64`, then `arm64`.
//!
//! A registry that cannot be read is an answer, not an error: deploying never waits on it.
//! Answers are kept for a few minutes, per credential.

pub mod https;
mod reference;
pub mod registry;

use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, State};
use reference::{Reference, registry_name};
use registry::{Credential, Inspected, Transport, Unreadable};
use secrecy::zeroize::Zeroizing;
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::ClusterExt as _;
use zeabur_kube::k8s_openapi::api::core::v1::Node;

use crate::error::ApiError;
use crate::handlers::{AppState, Person};

/// How long the whole read may take.
const DEADLINE: Duration = Duration::from_secs(10);
/// How long an answer is kept.
const KEEP: Duration = Duration::from_mins(5);
/// How many answers are kept.
const KEPT: usize = 256;
/// Read when the nodes' architectures are not known.
const FALLBACK: [&str; 2] = ["amd64", "arm64"];

/// Reads images' configurations, and keeps the answers for a while.
pub struct Inspector {
    transport: Arc<dyn Transport>,
    /// Registries reached over plain HTTP: loopback, where every address may be reached.
    insecure: Vec<String>,
    kept: Mutex<HashMap<String, (Instant, Inspected)>>,
    /// Which addresses the server may connect to.
    reach: https::Reach,
}

impl std::fmt::Debug for Inspector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inspector")
            .field("insecure", &self.insecure)
            .finish_non_exhaustive()
    }
}

impl Inspector {
    /// Reads through `transport`; the registries in `insecure` over plain HTTP.
    #[must_use]
    pub fn new(transport: Arc<dyn Transport>, insecure: Vec<String>) -> Self {
        Self {
            transport,
            insecure,
            kept: Mutex::new(HashMap::new()),
            reach: https::Reach::Any,
        }
    }

    /// Which addresses the server may connect to.
    #[must_use]
    pub const fn reach(&self) -> https::Reach {
        self.reach
    }

    /// Through [`https::Https`] reaching `reach`, trusting what `tls` trusts (see
    /// [`https::tls`]); the registry of Zeabur Builder is read over HTTPS like any other.
    #[must_use]
    pub fn connected(reach: https::Reach, tls: &Result<Arc<rustls::ClientConfig>, String>) -> Self {
        let transport: Arc<dyn Transport> = match tls {
            Ok(tls) => Arc::new(https::Https::new(reach, tls)),
            Err(error) => Arc::new(Broken(error.clone())),
        };
        let mut insecure: Vec<String> = Vec::new();
        if reach == https::Reach::Any {
            insecure.extend(["localhost".to_owned(), "127.0.0.1".to_owned()]);
        }
        Self {
            reach,
            ..Self::new(transport, insecure)
        }
    }

    fn base(&self, reference: &Reference) -> String {
        let host = reference.api_host();
        let bare = host.rsplit_once(':').map_or(host, |(h, _)| h);
        if self.insecure.iter().any(|i| i == host || i == bare) {
            format!("http://{host}")
        } else {
            format!("https://{host}")
        }
    }

    async fn inspect(
        &self,
        reference: &Reference,
        credential: Option<&Credential>,
        architectures: &[String],
    ) -> Result<Inspected, Unreadable> {
        let key = format!(
            "{}/{}@{}|{}|{}",
            reference.registry,
            reference.repository,
            reference.tag,
            architectures.join(","),
            credential.map_or_default(Credential::fingerprint)
        );
        if let Some(found) = self.kept.lock().ok().and_then(|kept| {
            kept.get(&key)
                .filter(|(at, _)| at.elapsed() < KEEP)
                .map(|(_, found)| found.clone())
        }) {
            return Ok(found);
        }
        let read = tokio::time::timeout(
            DEADLINE,
            registry::inspect(
                self.transport.as_ref(),
                reference,
                self.base(reference),
                credential,
                architectures,
            ),
        )
        .await
        .map_err(|_elapsed| {
            Unreadable::Unreachable("the registry did not answer in time".to_owned())
        })?;
        if let (Ok(found), Ok(mut kept)) = (&read, self.kept.lock()) {
            kept.retain(|_, (at, _)| at.elapsed() < KEEP);
            if kept.len() >= KEPT {
                kept.clear();
            }
            kept.insert(key, (Instant::now(), found.clone()));
        }
        read
    }
}

struct Broken(String);

impl Transport for Broken {
    fn get<'a>(
        &'a self,
        _get: registry::Get,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<registry::Answer, String>> + Send + 'a>> {
        let error = format!("Zeabur could not set up TLS: {}", self.0);
        Box::pin(async move { Err(error) })
    }
}

/// Credentials for the image's registry.
#[derive(Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegistryLogin {
    /// The registry, such as `ghcr.io`; used only when it is the image's.
    pub server: String,
    /// The username.
    pub username: String,
    /// The password or access token.
    #[schema(value_type = String)]
    pub password: SecretString,
}

impl std::fmt::Debug for RegistryLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegistryLogin")
            .field("server", &self.server)
            .field("username", &self.username)
            .finish_non_exhaustive()
    }
}

/// The image to read.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InspectRequest {
    /// The image reference, as a container's `image`.
    pub image: String,
    /// A `kubernetes.io/dockerconfigjson` or `kubernetes.io/dockercfg` Secret of the project,
    /// read as the person, whose entry for the image's registry is used.
    #[serde(default)]
    pub pull_secret: Option<String>,
    /// Credentials typed in, used instead of `pullSecret`.
    #[serde(default)]
    pub registry: Option<RegistryLogin>,
}

/// A port the image declares (`EXPOSE`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ExposedPort {
    /// The port number.
    pub port: u16,
    /// `TCP`, `UDP` or `SCTP`, as a container port names it.
    pub protocol: String,
}

/// Why an image's configuration could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum UnreadableReason {
    /// The registry has no such repository or tag.
    NotFound,
    /// The registry wants credentials, or refused the ones given.
    Unauthorized,
    /// The image has no build for the nodes' architectures.
    NoPlatform,
    /// The pull Secret could not be read as the person, or holds nothing for the registry.
    Credentials,
    /// The registry did not answer, or not in time.
    Unreachable,
    /// The registry answered something Zeabur cannot read.
    Unexpected,
}

/// What the image's configuration says, or why it could not be read.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImageInspection {
    /// The image as asked.
    pub image: String,
    /// The platform read, such as `linux/amd64`; absent when nothing was read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// The digest of the platform's manifest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    /// `config.ExposedPorts`, sorted by port; empty when the image declares none or was not read.
    pub exposed_ports: Vec<ExposedPort>,
    /// `config.User` as written (`nginx`, `1000:1000`); empty when the image sets none and runs
    /// as root. Kubernetes' `runAsNonRoot` cannot check a name, only a number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Whether the image runs as root: no user, `root` or UID `0`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs_as_root: Option<bool>,
    /// Why nothing was read; absent when it was.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unreadable: Option<UnreadableReason>,
    /// The registry's or the network's own words for `unreadable`, or the platforms the image
    /// has for `no-platform`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl ImageInspection {
    const fn unreadable(image: String, reason: UnreadableReason, detail: Option<String>) -> Self {
        Self {
            image,
            platform: None,
            digest: None,
            exposed_ports: Vec::new(),
            user: None,
            runs_as_root: None,
            unreadable: Some(reason),
            detail,
        }
    }
}

/// Reads an image's configuration from its registry: the ports it declares and the user it runs
/// as. A registry that cannot be read answers `200` with `unreadable`.
#[utoipa::path(post, path = "/api/v1/projects/{project}/images/inspect", tag = "projects",
    operation_id = "inspectImage",
    params(("project" = String, Path, description = "The project's namespace")),
    request_body = InspectRequest,
    responses((status = 200, body = ImageInspection),
              (status = 400, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn inspect(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(project): Path<String>,
    Json(request): Json<InspectRequest>,
) -> Result<Json<ImageInspection>, ApiError> {
    let image = request.image.trim().to_owned();
    let reference = Reference::parse(&image).ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::BadRequest,
            format!(
                "{image:?} is not an image reference such as nginx:1.29 or ghcr.io/acme/web:1.4."
            ),
        ))
    })?;
    let credential = match (&request.registry, &request.pull_secret) {
        (Some(login), _) => {
            (registry_name(&login.server) == reference.registry).then(|| Credential {
                username: login.username.clone(),
                password: login.password.clone(),
            })
        }
        (None, Some(secret)) => match cluster.reveal_secret(&project, secret).await {
            Ok(values) => match from_pull_secret(&values, &reference.registry) {
                Some(credential) => Some(credential),
                None => {
                    return Ok(Json(ImageInspection::unreadable(
                        image,
                        UnreadableReason::Credentials,
                        Some(format!(
                            "the Secret {secret} has no entry for {}",
                            reference.registry
                        )),
                    )));
                }
            },
            Err(error) => {
                return Ok(Json(ImageInspection::unreadable(
                    image,
                    UnreadableReason::Credentials,
                    Some(error.to_string()),
                )));
            }
        },
        (None, None) => None,
    };
    let architectures = cluster
        .list::<Node>(None)
        .await
        .map_or_default(|nodes| architectures(&nodes));
    let architectures = if architectures.is_empty() {
        FALLBACK.iter().map(|&a| a.to_owned()).collect()
    } else {
        architectures
    };
    let answer = match state
        .images
        .inspect(&reference, credential.as_ref(), &architectures)
        .await
    {
        Ok(read) => {
            let runs_as_root = runs_as_root(&read.user);
            ImageInspection {
                image,
                platform: Some(read.platform),
                digest: Some(read.digest),
                exposed_ports: read
                    .exposed_ports
                    .into_iter()
                    .map(|(port, protocol)| ExposedPort { port, protocol })
                    .collect(),
                user: Some(read.user),
                runs_as_root: Some(runs_as_root),
                unreadable: None,
                detail: None,
            }
        }
        Err(unreadable) => {
            let (reason, detail) = match unreadable {
                Unreadable::NotFound => (UnreadableReason::NotFound, None),
                Unreadable::Unauthorized => (UnreadableReason::Unauthorized, None),
                Unreadable::NoPlatform(offered) => (UnreadableReason::NoPlatform, Some(offered)),
                Unreadable::Unreachable(why) => (UnreadableReason::Unreachable, Some(why)),
                Unreadable::Unexpected(why) => (UnreadableReason::Unexpected, Some(why)),
            };
            tracing::debug!(image = %image, ?reason, detail = ?detail, "image configuration unreadable");
            ImageInspection::unreadable(image, reason, detail)
        }
    };
    Ok(Json(answer))
}

/// The nodes' architectures, the most common first.
fn architectures(nodes: &[Node]) -> Vec<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for node in nodes {
        let architecture = node
            .status
            .as_ref()
            .and_then(|s| s.node_info.as_ref())
            .map(|i| i.architecture.clone())
            .filter(|a| !a.is_empty())
            .or_else(|| {
                node.metadata
                    .labels
                    .as_ref()
                    .and_then(|l| l.get("kubernetes.io/arch").cloned())
            });
        if let Some(architecture) = architecture {
            *counts.entry(architecture).or_default() += 1;
        }
    }
    let mut sorted: Vec<(String, usize)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    sorted.into_iter().map(|(a, _)| a).collect()
}

fn runs_as_root(user: &str) -> bool {
    let name = user.split(':').next().unwrap_or_default().trim();
    name.is_empty() || name == "root" || name == "0"
}

/// The entry for `registry` in a `.dockerconfigjson` (`{"auths": {…}}`) or `.dockercfg` (the
/// same map without `auths`).
fn from_pull_secret(values: &BTreeMap<String, Vec<u8>>, registry: &str) -> Option<Credential> {
    use base64::Engine as _;

    #[derive(Deserialize)]
    struct Entry {
        username: Option<String>,
        password: Option<SecretString>,
        auth: Option<SecretString>,
    }
    #[derive(Deserialize)]
    struct Config {
        auths: HashMap<String, Entry>,
    }

    let entries: HashMap<String, Entry> = if let Some(json) = values.get(".dockerconfigjson") {
        serde_json::from_slice::<Config>(json).ok()?.auths
    } else {
        serde_json::from_slice(values.get(".dockercfg")?).ok()?
    };
    let entry = entries
        .into_iter()
        .find(|(server, _)| registry_name(server) == registry)?
        .1;
    let (username, password) = match (entry.username, entry.password) {
        (Some(username), Some(password)) if !username.is_empty() => (username, password),
        _ => {
            let decoded = Zeroizing::new(
                base64::engine::general_purpose::STANDARD
                    .decode(entry.auth?.expose_secret().trim())
                    .ok()?,
            );
            let (username, password) = std::str::from_utf8(&decoded).ok()?.split_once(':')?;
            (username.to_owned(), SecretString::from(password))
        }
    };
    Some(Credential { username, password })
}

#[cfg(test)]
mod tests {
    use super::registry::fake::nginx;
    use super::*;

    #[test]
    fn root_is_no_user_root_or_uid_zero() {
        for user in ["", "root", "0", "0:0", "root:root"] {
            assert!(runs_as_root(user), "{user}");
        }
        for user in ["nginx", "1000", "1000:0", "65532:65532"] {
            assert!(!runs_as_root(user), "{user}");
        }
    }

    #[test]
    fn a_pull_secret_gives_only_its_registrys_entry() {
        let config = serde_json::json!({"auths": {
            "https://index.docker.io/v1/": {"auth": "aHViOnB3"},
            "ghcr.io": {"username": "me", "password": "token"},
        }});
        let values = BTreeMap::from([(
            ".dockerconfigjson".to_owned(),
            serde_json::to_vec(&config).unwrap(),
        )]);
        let hub = from_pull_secret(&values, "docker.io").unwrap();
        assert_eq!(
            (hub.username.as_str(), hub.password.expose_secret()),
            ("hub", "pw")
        );
        let ghcr = from_pull_secret(&values, "ghcr.io").unwrap();
        assert_eq!(ghcr.password.expose_secret(), "token");
        assert!(from_pull_secret(&values, "quay.io").is_none());
        let old = BTreeMap::from([(
            ".dockercfg".to_owned(),
            br#"{"quay.io": {"auth": "dTpw"}}"#.to_vec(),
        )]);
        assert_eq!(from_pull_secret(&old, "quay.io").unwrap().username, "u");
    }

    #[tokio::test]
    async fn answers_are_kept_and_each_credential_has_its_own() {
        let fake = Arc::new(nginx());
        let transport: Arc<dyn Transport> = Arc::<registry::fake::Registry>::clone(&fake);
        let inspector = Inspector::new(transport, Vec::new());
        // The fake answers at registry.test.
        let reference = Reference::parse("registry.test/library/nginx").unwrap();
        let amd64 = vec!["amd64".to_owned()];
        assert!(inspector.inspect(&reference, None, &amd64).await.is_ok());
        let sent = fake.sent().len();
        assert!(inspector.inspect(&reference, None, &amd64).await.is_ok());
        assert_eq!(fake.sent().len(), sent, "read again instead of kept");
        let credential = Credential {
            username: "me".to_owned(),
            password: SecretString::from("pw".to_owned()),
        };
        assert!(
            inspector
                .inspect(&reference, Some(&credential), &amd64)
                .await
                .is_ok()
        );
        assert!(
            fake.sent().len() > sent,
            "another credential's answer was reused"
        );
    }

    #[test]
    fn plain_http_only_for_the_registries_named() {
        let inspector = Inspector::new(
            Arc::new(Broken(String::new())),
            vec!["zot.zot.svc".to_owned()],
        );
        let base = |image: &str| inspector.base(&Reference::parse(image).unwrap());
        assert_eq!(base("zot.zot.svc:5000/shop/web"), "http://zot.zot.svc:5000");
        assert_eq!(base("nginx"), "https://registry-1.docker.io");
        assert_eq!(base("ghcr.io/acme/web"), "https://ghcr.io");
    }
}
