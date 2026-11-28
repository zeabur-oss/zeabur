//! What an image says about itself: `POST /projects/{p}/images/inspect` reads its configuration
//! from the registry, so the form that deploys it can offer the port it listens on.
//!
//! Public images are read anonymously. A private one is read with the credentials the person
//! typed, or with a pull Secret of the project, which Zeabur reads as the person; credentials go
//! only to the registry they name. The architectures of the cluster's nodes, where the person
//! may list nodes, choose among an index's platforms; else `amd64`, then `arm64`.
//!
//! An image in the build registry, such as Zeabur Builder's zot, is read trusting, on top, the CA
//! that the catalog component providing it names in `provides.registry.ca`, read as the person.
//! Only connections to that registry's host trust it.
//!
//! A registry that cannot be read is an answer, not an error: deploying never waits on it.
//! Answers are kept for a few minutes, per credential.

use zeabur_model::adapters::image_reference as reference;
pub mod registry;
mod transport;

use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::State;
use reference::{Reference, registry_name};
use registry::{Credential, Inspected, Transport, Unreadable};
use secrecy::zeroize::Zeroizing;
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::Node;
use zeabur_kube::{Cluster, ClusterExt as _};

use crate::error::ApiError;
use crate::extract::{Json, Path};
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
    reach: zeabur_http::Reach,
    /// How a read that brings its own CA connects.
    extra: Extra,
}

/// How a read that brings the build registry's CA connects.
enum Extra {
    /// Through the given transport, as any other read.
    Given,
    /// Over HTTPS to the registry's host, trusting the platform's roots, `trusted_ca` and the CA.
    Https { trusted_ca: Option<Vec<u8>> },
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
            reach: zeabur_http::Reach::Any,
            extra: Extra::Given,
        }
    }

    /// Which addresses the server may connect to.
    #[must_use]
    pub const fn reach(&self) -> zeabur_http::Reach {
        self.reach
    }

    /// Through [`zeabur_http::Https`] reaching `reach`, trusting what `tls` trusts (see
    /// [`zeabur_http::tls`]), which is the platform's roots and `trusted_ca`.
    #[must_use]
    pub fn connected(
        reach: zeabur_http::Reach,
        tls: &Result<Arc<rustls::ClientConfig>, String>,
        trusted_ca: Option<Vec<u8>>,
    ) -> Self {
        let transport: Arc<dyn Transport> = match tls {
            Ok(tls) => Arc::new(zeabur_http::Https::new(reach, tls)),
            Err(error) => Arc::new(Broken(error.clone())),
        };
        let mut insecure: Vec<String> = Vec::new();
        if reach == zeabur_http::Reach::Any {
            insecure.extend(["localhost".to_owned(), "127.0.0.1".to_owned()]);
        }
        Self {
            reach,
            extra: Extra::Https { trusted_ca },
            ..Self::new(transport, insecure)
        }
    }

    /// The registry's API; a registry with a CA of its own speaks HTTPS.
    fn base(&self, reference: &Reference, has_ca: bool) -> String {
        let host = reference.api_host();
        let bare = host.rsplit_once(':').map_or(host, |(h, _)| h);
        if !has_ca && self.insecure.iter().any(|i| i == host || i == bare) {
            format!("http://{host}")
        } else {
            format!("https://{host}")
        }
    }

    /// The transport for a read of `reference` that also trusts the PEM `ca`, at its host only.
    fn trusting(&self, reference: &Reference, ca: &[u8]) -> Option<Pinned> {
        let Extra::Https { trusted_ca } = &self.extra else {
            return None;
        };
        let bundle = [trusted_ca.as_deref().unwrap_or_default(), b"\n", ca].concat();
        match zeabur_http::tls(Some(&bundle)) {
            Ok(tls) => Some(Pinned {
                host: reference.api_host().to_ascii_lowercase(),
                trusting: zeabur_http::Https::new(self.reach, &tls),
                other: Arc::clone(&self.transport),
            }),
            Err(error) => {
                tracing::debug!(%error, "the build registry's CA is not usable");
                None
            }
        }
    }

    async fn inspect(
        &self,
        reference: &Reference,
        credential: Option<&Credential>,
        architectures: &[String],
        ca: Option<&[u8]>,
    ) -> Result<Inspected, Unreadable> {
        let key = format!(
            "{}/{}@{}|{}|{}|{}",
            reference.registry,
            reference.repository,
            reference.tag,
            architectures.join(","),
            credential.map_or_default(Credential::fingerprint),
            ca.map_or_default(|ca| hex::encode(Sha256::digest(ca)))
        );
        if let Some(found) = self.kept.lock().ok().and_then(|kept| {
            kept.get(&key)
                .filter(|(at, _)| at.elapsed() < KEEP)
                .map(|(_, found)| found.clone())
        }) {
            return Ok(found);
        }
        let pinned = ca.and_then(|ca| self.trusting(reference, ca));
        let transport: &dyn Transport = match &pinned {
            Some(pinned) => pinned,
            None => self.transport.as_ref(),
        };
        let read = tokio::time::timeout(
            DEADLINE,
            registry::inspect(
                transport,
                reference,
                self.base(reference, ca.is_some()),
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

/// Sends a request to `host` through `trusting`, and any other through `other`.
struct Pinned {
    host: String,
    trusting: zeabur_http::Https,
    other: Arc<dyn Transport>,
}

impl Transport for Pinned {
    fn get<'a>(
        &'a self,
        get: registry::Get,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<registry::Answer, String>> + Send + 'a>> {
        let to_host = get
            .url
            .parse::<axum::http::Uri>()
            .ok()
            .and_then(|uri| {
                uri.authority()
                    .map(|a| a.as_str().eq_ignore_ascii_case(&self.host))
            })
            .unwrap_or_default();
        if to_host {
            Transport::get(&self.trusting, get)
        } else {
            self.other.get(get)
        }
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
    pub registry: Option<zeabur_plan::RegistryLogin>,
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
    params(("project" = crate::rules::ProjectName, Path, description = "The project's namespace")),
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
                password: SecretString::from(login.password.clone()),
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
    let ca = published_ca(cluster.as_ref(), &state, &reference.registry).await;
    let answer = match state
        .images
        .inspect(
            &reference,
            credential.as_ref(),
            &architectures,
            ca.as_deref(),
        )
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

/// The CA of the build registry (`builds.registry`), when `server` is it: the Secret key that the
/// catalog component providing that registry names in `provides.registry.ca`, read as the person.
/// The declaration comes from the catalog Zeabur reads. Zeabur Builder lets every signed-in user
/// read its CA-only Secret `registry-ca`, so a Developer of one project reads it too. `None` keeps
/// the server's own trust: no component provides that registry, or the person may not read the
/// Secret, or it is not there.
pub(crate) async fn published_ca(
    cluster: &dyn Cluster,
    state: &AppState,
    server: &str,
) -> Option<Vec<u8>> {
    let registry = registry_name(server);
    if registry_name(state.settings.builds.registry.as_deref()?) != registry {
        return None;
    }
    let snapshot = crate::cluster::catalog::current(state).await.snapshot?;
    let ca = snapshot
        .components
        .values()
        .filter_map(|file| file.declaration.provides.as_ref()?.registry.as_ref())
        .find(|provided| registry_name(&provided.host) == registry)?
        .ca
        .clone();
    match cluster.reveal_secret(&ca.namespace, &ca.name).await {
        Ok(mut values) => values.remove(&ca.key),
        Err(error) => {
            tracing::debug!(%error, namespace = %ca.namespace, secret = %ca.name, "the registry CA is unreadable");
            None
        }
    }
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
            serde_json::json!({"quay.io": {"auth": "dTpw"}})
                .to_string()
                .into_bytes(),
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
        assert!(
            inspector
                .inspect(&reference, None, &amd64, None)
                .await
                .is_ok()
        );
        let sent = fake.sent().len();
        assert!(
            inspector
                .inspect(&reference, None, &amd64, None)
                .await
                .is_ok()
        );
        assert_eq!(fake.sent().len(), sent, "read again instead of kept");
        let credential = Credential {
            username: "me".to_owned(),
            password: SecretString::from("pw".to_owned()),
        };
        assert!(
            inspector
                .inspect(&reference, Some(&credential), &amd64, None)
                .await
                .is_ok()
        );
        assert!(
            fake.sent().len() > sent,
            "another credential's answer was reused"
        );
    }

    /// A token service or redirect on another host is reached with the server's own trust.
    #[tokio::test]
    async fn only_the_build_registrys_host_trusts_its_ca() {
        let fake = Arc::new(nginx());
        let other: Arc<dyn Transport> = Arc::<registry::fake::Registry>::clone(&fake);
        let pinned = Pinned {
            host: "127.0.0.1:1".to_owned(),
            trusting: zeabur_http::Https::new(
                zeabur_http::Reach::Any,
                &zeabur_http::tls(None).unwrap(),
            ),
            other,
        };
        let get = |url: &str| registry::Get {
            url: url.to_owned(),
            accept: None,
            authorization: None,
            limit: 1024,
        };
        assert!(pinned.get(get("https://127.0.0.1:1/v2/")).await.is_err());
        assert!(fake.sent().is_empty(), "the registry's host went elsewhere");
        let _answer = pinned.get(get("https://registry.test/token")).await;
        assert_eq!(fake.sent().len(), 1, "another host trusted the CA");
    }

    #[test]
    fn plain_http_only_for_the_registries_named() {
        let inspector = Inspector::new(
            Arc::new(Broken(String::new())),
            vec!["zot.zot.svc".to_owned()],
        );
        let base = |image: &str| inspector.base(&Reference::parse(image).unwrap(), false);
        assert_eq!(base("zot.zot.svc:5000/shop/web"), "http://zot.zot.svc:5000");
        assert_eq!(base("nginx"), "https://registry-1.docker.io");
        assert_eq!(base("ghcr.io/acme/web"), "https://ghcr.io");
    }
}
