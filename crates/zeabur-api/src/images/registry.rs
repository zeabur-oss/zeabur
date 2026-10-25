//! Reading an image's configuration from its registry over the OCI distribution API
//! [oci-distribution].
//!
//! The manifest, the platform's manifest when that is an index [oci-index], then the
//! configuration blob [oci-config]. A registry that answers `401` names
//! how to authenticate in `WWW-Authenticate`: a bearer token from its token service, which
//! Docker Hub, GitHub and Quay hand out without credentials for public images
//! [docker-token], or Basic.
//!
//! [oci-distribution]: https://github.com/opencontainers/distribution-spec/blob/v1.1.1/spec.md
//! [oci-index]: https://github.com/opencontainers/image-spec/blob/v1.1.1/image-index.md
//! [oci-config]: https://github.com/opencontainers/image-spec/blob/v1.1.1/config.md
//! [docker-token]: https://distribution.github.io/distribution/spec/auth/token/

use std::collections::BTreeSet;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

use base64::Engine as _;
use secrecy::zeroize::Zeroizing;
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use zeabur_core::secret;

use super::reference::Reference;

const INDEX_TYPES: [&str; 2] = [
    "application/vnd.oci.image.index.v1+json",
    "application/vnd.docker.distribution.manifest.list.v2+json",
];
const MANIFEST_TYPES: [&str; 2] = [
    "application/vnd.oci.image.manifest.v1+json",
    "application/vnd.docker.distribution.manifest.v2+json",
];
/// The OCI distribution specification lets registries refuse manifests above 4 MiB.
const MANIFEST_LIMIT: usize = 4 * 1024 * 1024;
const CONFIG_LIMIT: usize = 4 * 1024 * 1024;
const TOKEN_LIMIT: usize = 1024 * 1024;
const REDIRECTS: usize = 3;

/// One `GET`.
#[derive(Clone)]
pub struct Get {
    /// The absolute URL.
    pub url: String,
    /// The `Accept` header, if any.
    pub accept: Option<String>,
    /// The `Authorization` header, if any.
    pub authorization: Option<SecretString>,
    /// The largest body to read; a longer one fails.
    pub limit: usize,
}

impl fmt::Debug for Get {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Get")
            .field("url", &self.url)
            .field("accept", &self.accept)
            .field("authorization", &self.authorization.as_ref().map(|_| "…"))
            .finish_non_exhaustive()
    }
}

/// A registry's answer.
#[derive(Debug, Clone, Default)]
pub struct Answer {
    /// The status code.
    pub status: u16,
    /// `WWW-Authenticate`.
    pub www_authenticate: Option<String>,
    /// `Location`.
    pub location: Option<String>,
    /// `Content-Type`.
    pub content_type: Option<String>,
    /// The body.
    pub body: Vec<u8>,
}

/// Sends `GET`s to registries.
pub trait Transport: Send + Sync {
    /// Sends `get`; an error says why no answer came.
    fn get<'a>(
        &'a self,
        get: Get,
    ) -> Pin<Box<dyn Future<Output = Result<Answer, String>> + Send + 'a>>;
}

/// A username and password for one registry.
#[derive(Clone)]
pub(crate) struct Credential {
    pub(crate) username: String,
    pub(crate) password: SecretString,
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credential")
            .field("username", &self.username)
            .finish_non_exhaustive()
    }
}

impl Credential {
    fn basic(&self) -> SecretString {
        let pair = Zeroizing::new(secret::join(&[
            &self.username,
            ":",
            self.password.expose_secret(),
        ]));
        let encoded =
            Zeroizing::new(base64::engine::general_purpose::STANDARD.encode(pair.as_bytes()));
        SecretString::from(secret::join(&["Basic ", &encoded]))
    }

    /// A digest that tells credentials apart without holding them.
    pub(crate) fn fingerprint(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(self.username.as_bytes());
        hash.update([0]);
        hash.update(self.password.expose_secret().as_bytes());
        hex(&hash.finalize())
    }
}

/// What was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Inspected {
    /// `os/architecture[/variant]` of the configuration read.
    pub(crate) platform: String,
    /// The digest of the platform's manifest.
    pub(crate) digest: String,
    /// `config.ExposedPorts`, as port and upper-case protocol, sorted.
    pub(crate) exposed_ports: Vec<(u16, String)>,
    /// `config.User`, empty when the image sets none.
    pub(crate) user: String,
}

/// Why nothing was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unreadable {
    /// The registry has no such repository or tag.
    NotFound,
    /// The registry wants credentials, or refused the ones given.
    Unauthorized,
    /// The image has no build for the nodes' architectures; the ones it has.
    NoPlatform(String),
    /// No answer, or none in time.
    Unreachable(String),
    /// An answer Zeabur cannot read.
    Unexpected(String),
}

struct Session<'a> {
    transport: &'a dyn Transport,
    reference: &'a Reference,
    base: String,
    credential: Option<&'a Credential>,
    authorization: Option<SecretString>,
}

/// Reads the configuration of `reference` from `base` (`https://host`), choosing among an
/// index's platforms the first of `architectures` it has, on Linux.
pub(crate) async fn inspect(
    transport: &dyn Transport,
    reference: &Reference,
    base: String,
    credential: Option<&Credential>,
    architectures: &[String],
) -> Result<Inspected, Unreadable> {
    let mut session = Session {
        transport,
        reference,
        base,
        credential,
        authorization: None,
    };
    let accept = [INDEX_TYPES, MANIFEST_TYPES].concat().join(", ");
    let pinned = reference.tag.starts_with("sha256:");
    let answer = session
        .fetch(
            &manifest_path(reference, &reference.tag),
            &accept,
            MANIFEST_LIMIT,
        )
        .await?;
    if pinned {
        verify(&answer.body, &reference.tag)?;
    }
    let mut digest = if pinned {
        reference.tag.clone()
    } else {
        digest_of(&answer.body)
    };
    let mut manifest: Manifest = parse(&answer.body)?;
    let mut platform = None;
    if is_index(answer.content_type.as_deref(), &manifest) {
        let chosen = choose(&manifest.manifests, architectures)?;
        platform = Some(chosen.platform_name());
        digest.clone_from(&chosen.digest);
        let accept = MANIFEST_TYPES.join(", ");
        let answer = session
            .fetch(&manifest_path(reference, &digest), &accept, MANIFEST_LIMIT)
            .await?;
        verify(&answer.body, &digest)?;
        manifest = parse(&answer.body)?;
    }
    let config = manifest
        .config
        .ok_or_else(|| Unreadable::Unexpected("the manifest names no configuration".to_owned()))?;
    if !config.digest.starts_with("sha256:") {
        return Err(Unreadable::Unexpected(format!(
            "the configuration's digest {} is not SHA-256",
            config.digest
        )));
    }
    let path = format!("/v2/{}/blobs/{}", reference.repository, config.digest);
    let answer = session.fetch(&path, "*/*", CONFIG_LIMIT).await?;
    verify(&answer.body, &config.digest)?;
    let image: ImageConfig = parse(&answer.body)?;
    let platform = platform.unwrap_or_else(|| {
        Platform {
            os: image.os.clone().unwrap_or_default(),
            architecture: image.architecture.clone().unwrap_or_default(),
            variant: image.variant.clone(),
        }
        .name()
    });
    let inner = image.config.unwrap_or_default();
    Ok(Inspected {
        platform,
        digest,
        exposed_ports: ports(inner.exposed_ports.unwrap_or_default().keys()),
        user: inner.user.unwrap_or_default(),
    })
}

impl Session<'_> {
    async fn fetch(
        &mut self,
        path: &str,
        accept: &str,
        limit: usize,
    ) -> Result<Answer, Unreadable> {
        let mut url = format!("{}{path}", self.base);
        let mut authorization = self.authorization.clone();
        let mut challenged = false;
        let mut redirects = 0;
        loop {
            let get = Get {
                url: url.clone(),
                accept: Some(accept.to_owned()),
                authorization: authorization.clone(),
                limit,
            };
            let answer = self
                .transport
                .get(get)
                .await
                .map_err(Unreadable::Unreachable)?;
            match answer.status {
                200 => return Ok(answer),
                401 if !challenged => {
                    challenged = true;
                    let challenge = answer.www_authenticate.as_deref().unwrap_or_default();
                    self.authorization = Some(self.authorize(challenge).await?);
                    authorization.clone_from(&self.authorization);
                }
                301 | 302 | 303 | 307 | 308 if redirects < REDIRECTS => {
                    redirects += 1;
                    let location = answer.location.ok_or_else(|| {
                        Unreadable::Unexpected("a redirect without a location".to_owned())
                    })?;
                    let next = join(&url, &location)?;
                    // A blob is often served from another host, such as a CDN with a signed URL;
                    // the registry's credentials stay with the registry.
                    if origin(&next) != origin(&url) {
                        authorization = None;
                    }
                    url = next;
                }
                401 | 403 => return Err(Unreadable::Unauthorized),
                404 => return Err(Unreadable::NotFound),
                status => {
                    return Err(Unreadable::Unexpected(format!(
                        "the registry answered {status}"
                    )));
                }
            }
        }
    }

    async fn authorize(&self, challenge: &str) -> Result<SecretString, Unreadable> {
        let (scheme, params) = challenge.trim().split_once(' ').unwrap_or((challenge, ""));
        if scheme.eq_ignore_ascii_case("basic") {
            return self
                .credential
                .map(Credential::basic)
                .ok_or(Unreadable::Unauthorized);
        }
        if !scheme.eq_ignore_ascii_case("bearer") {
            return Err(Unreadable::Unauthorized);
        }
        let params = challenge_params(params);
        let realm = params
            .iter()
            .find(|(k, _)| k == "realm")
            .map(|(_, v)| v.clone())
            .ok_or(Unreadable::Unauthorized)?;
        if !realm.starts_with("https://") && origin(&realm) != origin(&self.base) {
            return Err(Unreadable::Unexpected(format!(
                "the token service {realm} is not HTTPS"
            )));
        }
        let query = {
            let mut query = form_urlencoded::Serializer::new(String::new());
            if let Some((_, service)) = params.iter().find(|(k, _)| k == "service") {
                query.append_pair("service", service);
            }
            query.append_pair(
                "scope",
                &format!("repository:{}:pull", self.reference.repository),
            );
            query.finish()
        };
        let separator = if realm.contains('?') { '&' } else { '?' };
        let get = Get {
            url: format!("{realm}{separator}{query}"),
            accept: Some("application/json".to_owned()),
            authorization: self.credential.map(Credential::basic),
            limit: TOKEN_LIMIT,
        };
        let answer = self
            .transport
            .get(get)
            .await
            .map_err(Unreadable::Unreachable)?;
        match answer.status {
            200 => {}
            401 | 403 => return Err(Unreadable::Unauthorized),
            status => {
                return Err(Unreadable::Unexpected(format!(
                    "the token service answered {status}"
                )));
            }
        }
        let token: Token = parse(&answer.body)?;
        token
            .token
            .or(token.access_token)
            .filter(|t| !t.expose_secret().is_empty())
            .map(|t| SecretString::from(secret::join(&["Bearer ", t.expose_secret()])))
            .ok_or_else(|| Unreadable::Unexpected("the token service gave no token".to_owned()))
    }
}

fn manifest_path(reference: &Reference, tag_or_digest: &str) -> String {
    format!("/v2/{}/manifests/{tag_or_digest}", reference.repository)
}

/// `key="value"` pairs of a challenge (RFC 9110, §11.2), values unquoted.
fn challenge_params(params: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = params.trim();
    while let Some((key, after)) = rest.split_once('=') {
        let key = key
            .trim()
            .trim_start_matches(',')
            .trim()
            .to_ascii_lowercase();
        let after = after.trim_start();
        let (value, next) = after.strip_prefix('"').map_or_else(
            || after.split_once(',').unwrap_or((after, "")),
            |quoted| quoted.split_once('"').unwrap_or((quoted, "")),
        );
        out.push((key, value.trim().to_owned()));
        rest = next.trim_start().trim_start_matches(',');
    }
    out
}

/// `scheme://authority` of an absolute URL.
fn origin(url: &str) -> &str {
    let after = url.find("://").map_or(0, |i| i + 3);
    let end = url
        .get(after..)
        .and_then(|rest| rest.find('/'))
        .map_or(url.len(), |i| after + i);
    url.get(..end).unwrap_or(url)
}

/// `location` against `base`: absolute, or a path on the same origin.
fn join(base: &str, location: &str) -> Result<String, Unreadable> {
    if location.starts_with("https://") || location.starts_with("http://") {
        Ok(location.to_owned())
    } else if location.starts_with('/') && !location.starts_with("//") {
        Ok(format!("{}{location}", origin(base)))
    } else {
        Err(Unreadable::Unexpected(format!(
            "a redirect to {location}, which Zeabur does not follow"
        )))
    }
}

fn parse<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, Unreadable> {
    serde_json::from_slice(body)
        .map_err(|e| Unreadable::Unexpected(format!("an answer that is not what was asked: {e}")))
}

fn digest_of(body: &[u8]) -> String {
    format!("sha256:{}", hex(&Sha256::digest(body)))
}

fn verify(body: &[u8], digest: &str) -> Result<(), Unreadable> {
    if digest_of(body) == digest {
        Ok(())
    } else {
        Err(Unreadable::Unexpected(format!(
            "the content does not match its digest {digest}"
        )))
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, b| {
        // Writing to a String cannot fail.
        write!(out, "{b:02x}").unwrap_or_default();
        out
    })
}

fn is_index(content_type: Option<&str>, manifest: &Manifest) -> bool {
    let media_type = content_type
        .map(|c| c.split(';').next().unwrap_or_default().trim())
        .filter(|c| !c.is_empty() && *c != "application/json")
        .or(manifest.media_type.as_deref());
    media_type.map_or_else(|| manifest.config.is_none(), |t| INDEX_TYPES.contains(&t))
}

fn choose<'a>(
    manifests: &'a [Descriptor],
    architectures: &[String],
) -> Result<&'a Descriptor, Unreadable> {
    let linux = || {
        manifests
            .iter()
            .filter(|m| m.platform.as_ref().is_some_and(|p| p.os == "linux"))
    };
    for architecture in architectures {
        let matching: Vec<&Descriptor> = linux()
            .filter(|m| {
                m.platform
                    .as_ref()
                    .is_some_and(|p| &p.architecture == architecture)
            })
            .collect();
        // arm64 without a variant is v8; prefer it over a v9 build where an index has both.
        let plain = matching.iter().copied().find(|m| {
            m.platform
                .as_ref()
                .is_some_and(|p| p.variant.as_deref().is_none_or(|v| v == "v8"))
        });
        if let Some(found) = plain.or_else(|| matching.first().copied()) {
            return Ok(found);
        }
    }
    let offered: BTreeSet<String> = manifests
        .iter()
        .filter_map(|m| m.platform.as_ref())
        .filter(|p| p.os != "unknown")
        .map(Platform::name)
        .collect();
    Err(Unreadable::NoPlatform(
        offered.into_iter().collect::<Vec<_>>().join(", "),
    ))
}

/// `80/tcp` keys as `(80, "TCP")`; a key without a protocol is TCP, as Docker reads it.
fn ports<'a>(keys: impl Iterator<Item = &'a String>) -> Vec<(u16, String)> {
    keys.filter_map(|key| {
        let (port, protocol) = key.split_once('/').unwrap_or((key, "tcp"));
        let port = port.trim().parse::<u16>().ok().filter(|p| *p > 0)?;
        let protocol = protocol.trim().to_ascii_uppercase();
        matches!(protocol.as_str(), "TCP" | "UDP" | "SCTP").then_some((port, protocol))
    })
    .collect::<BTreeSet<_>>()
    .into_iter()
    .collect()
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    media_type: Option<String>,
    #[serde(default)]
    manifests: Vec<Descriptor>,
    config: Option<Descriptor>,
}

#[derive(Debug, Deserialize)]
struct Descriptor {
    digest: String,
    platform: Option<Platform>,
}

impl Descriptor {
    fn platform_name(&self) -> String {
        self.platform.as_ref().map_or_default(Platform::name)
    }
}

#[derive(Debug, Deserialize)]
struct Platform {
    #[serde(default)]
    os: String,
    #[serde(default)]
    architecture: String,
    variant: Option<String>,
}

impl Platform {
    fn name(&self) -> String {
        match &self.variant {
            Some(variant) if !variant.is_empty() => {
                format!("{}/{}/{variant}", self.os, self.architecture)
            }
            _ => format!("{}/{}", self.os, self.architecture),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ImageConfig {
    os: Option<String>,
    architecture: Option<String>,
    variant: Option<String>,
    config: Option<RuntimeConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct RuntimeConfig {
    #[serde(rename = "ExposedPorts")]
    exposed_ports: Option<std::collections::BTreeMap<String, serde_json::Value>>,
    #[serde(rename = "User")]
    user: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Token {
    token: Option<SecretString>,
    access_token: Option<SecretString>,
}

#[cfg(test)]
pub(crate) mod fake {
    //! A registry in memory, answering as Docker Hub does: a bearer challenge, a token service
    //! on another host, and blobs redirected to a third.

    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use super::*;

    /// One image in the fake registry.
    pub(crate) struct Image {
        pub(crate) platform: (&'static str, &'static str),
        pub(crate) config: serde_json::Value,
    }

    #[derive(Default)]
    pub(crate) struct Registry {
        /// `url` → answer, without authentication.
        pub(crate) routes: BTreeMap<String, Answer>,
        /// Credentials the token service wants; `None`: anonymous tokens.
        pub(crate) private: Option<(String, String)>,
        pub(crate) sent: Mutex<Vec<Get>>,
        pub(crate) unreachable: bool,
    }

    pub(crate) const REGISTRY: &str = "https://registry.test";
    pub(crate) const REALM: &str = "https://auth.test/token";
    const CDN: &str = "https://cdn.test";
    const TOKEN: &str = "t0k3n";

    fn json(value: &serde_json::Value, media_type: &str) -> Answer {
        Answer {
            status: 200,
            content_type: Some(media_type.to_owned()),
            body: serde_json::to_vec(value).unwrap_or_default(),
            ..Answer::default()
        }
    }

    impl Registry {
        /// `repository:tag` as an index of `images`.
        pub(crate) fn with_index(mut self, repository: &str, tag: &str, images: &[Image]) -> Self {
            let mut entries = Vec::new();
            for image in images {
                let config = serde_json::to_vec(&image.config).unwrap_or_default();
                let config_digest = digest_of(&config);
                let blob = format!("/v2/{repository}/blobs/{config_digest}");
                self.routes.insert(
                    format!("{REGISTRY}{blob}"),
                    Answer {
                        status: 307,
                        location: Some(format!("{CDN}{blob}")),
                        ..Answer::default()
                    },
                );
                self.routes.insert(
                    format!("{CDN}{blob}"),
                    Answer {
                        status: 200,
                        body: config,
                        ..Answer::default()
                    },
                );
                let manifest = serde_json::json!({
                    "schemaVersion": 2,
                    "mediaType": MANIFEST_TYPES[0],
                    "config": {"mediaType": "application/vnd.oci.image.config.v1+json",
                               "digest": config_digest, "size": 1},
                    "layers": [],
                });
                let answer = json(&manifest, MANIFEST_TYPES[0]);
                let digest = digest_of(&answer.body);
                self.routes.insert(
                    format!("{REGISTRY}/v2/{repository}/manifests/{digest}"),
                    answer,
                );
                entries.push(serde_json::json!({
                    "mediaType": MANIFEST_TYPES[0],
                    "digest": digest,
                    "size": 1,
                    "platform": {"os": image.platform.0, "architecture": image.platform.1},
                }));
            }
            entries.push(serde_json::json!({
                "mediaType": MANIFEST_TYPES[0],
                "digest": format!("sha256:{}", "0".repeat(64)),
                "size": 1,
                "platform": {"os": "unknown", "architecture": "unknown"},
            }));
            let index = serde_json::json!({
                "schemaVersion": 2,
                "mediaType": INDEX_TYPES[0],
                "manifests": entries,
            });
            self.routes.insert(
                format!("{REGISTRY}/v2/{repository}/manifests/{tag}"),
                json(&index, INDEX_TYPES[0]),
            );
            self
        }

        pub(crate) fn sent(&self) -> Vec<Get> {
            self.sent.lock().map_or_default(|s| s.clone())
        }

        fn answer(&self, get: &Get) -> Answer {
            let authorization = get
                .authorization
                .as_ref()
                .map(|a| a.expose_secret().to_owned());
            if let Some(query) = get.url.strip_prefix(&format!("{REALM}?")) {
                let wanted = self.private.as_ref().map(|(user, password)| {
                    Credential {
                        username: user.clone(),
                        password: SecretString::from(password.clone()),
                    }
                    .basic()
                    .expose_secret()
                    .to_owned()
                });
                if wanted.is_some() && authorization != wanted {
                    return Answer {
                        status: 401,
                        ..Answer::default()
                    };
                }
                assert!(query.contains("service=registry.test"), "{query}");
                return json(&serde_json::json!({"token": TOKEN}), "application/json");
            }
            if get.url.starts_with(REGISTRY) && authorization.as_deref() != Some("Bearer t0k3n") {
                return Answer {
                    status: 401,
                    www_authenticate: Some(format!(
                        "Bearer realm=\"{REALM}\",service=\"registry.test\",scope=\"x\""
                    )),
                    ..Answer::default()
                };
            }
            if get.url.starts_with(CDN) {
                assert!(get.authorization.is_none(), "credentials sent to the CDN");
            }
            self.routes
                .get(&get.url)
                .cloned()
                .unwrap_or_else(|| Answer {
                    status: 404,
                    ..Answer::default()
                })
        }
    }

    impl Transport for Registry {
        fn get<'a>(
            &'a self,
            get: Get,
        ) -> Pin<Box<dyn Future<Output = Result<Answer, String>> + Send + 'a>> {
            let answer = if self.unreachable {
                Err("connection refused".to_owned())
            } else {
                Ok(self.answer(&get))
            };
            if let Ok(mut sent) = self.sent.lock() {
                sent.push(get);
            }
            Box::pin(async move { answer })
        }
    }

    pub(crate) fn nginx() -> Registry {
        let config = |arch: &str| {
            serde_json::json!({
                "architecture": arch,
                "os": "linux",
                "config": {"ExposedPorts": {"80/tcp": {}}, "User": ""},
            })
        };
        Registry::default().with_index(
            "library/nginx",
            "latest",
            &[
                Image {
                    platform: ("linux", "amd64"),
                    config: config("amd64"),
                },
                Image {
                    platform: ("linux", "arm64"),
                    config: config("arm64"),
                },
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{Image, REGISTRY, Registry, nginx};
    use super::*;

    fn reference(image: &str) -> Reference {
        Reference::parse(image).unwrap_or_else(|| panic!("{image}"))
    }

    async fn read(
        registry: &Registry,
        image: &str,
        credential: Option<&Credential>,
        architectures: &[&str],
    ) -> Result<Inspected, Unreadable> {
        let architectures: Vec<String> = architectures.iter().map(|&a| a.to_owned()).collect();
        inspect(
            registry,
            &reference(image),
            REGISTRY.to_owned(),
            credential,
            &architectures,
        )
        .await
    }

    #[tokio::test]
    async fn a_public_image_is_read_with_an_anonymous_token_for_the_nodes_platform() {
        let registry = nginx();
        let read = read(&registry, "nginx", None, &["arm64", "amd64"]).await;
        let read = read.unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(read.platform, "linux/arm64");
        assert_eq!(read.exposed_ports, vec![(80, "TCP".to_owned())]);
        assert_eq!(read.user, "");
        // The token is asked for once, for this repository only, and without credentials.
        let tokens: Vec<Get> = registry
            .sent()
            .into_iter()
            .filter(|g| g.url.starts_with(fake::REALM))
            .collect();
        assert_eq!(tokens.len(), 1);
        assert!(
            tokens[0]
                .url
                .contains("scope=repository%3Alibrary%2Fnginx%3Apull")
        );
        assert!(tokens[0].authorization.is_none());
    }

    #[tokio::test]
    async fn without_a_build_for_the_nodes_the_image_says_which_it_has() {
        let registry = nginx();
        assert_eq!(
            read(&registry, "nginx", None, &["s390x"]).await,
            Err(Unreadable::NoPlatform(
                "linux/amd64, linux/arm64".to_owned()
            ))
        );
    }

    #[tokio::test]
    async fn a_private_image_needs_its_credentials_at_the_token_service() {
        let mut registry = Registry::default().with_index(
            "acme/api",
            "1",
            &[Image {
                platform: ("linux", "amd64"),
                config: serde_json::json!({"config": {
                    "ExposedPorts": {"8080/tcp": {}, "9090": {}, "53/udp": {}, "bad/tcp": {}},
                    "User": "1000"}}),
            }],
        );
        registry.private = Some(("me".to_owned(), "secret".to_owned()));
        assert_eq!(
            read(&registry, "registry.test/acme/api:1", None, &["amd64"]).await,
            Err(Unreadable::Unauthorized)
        );
        let credential = Credential {
            username: "me".to_owned(),
            password: SecretString::from("secret".to_owned()),
        };
        let read = read(
            &registry,
            "registry.test/acme/api:1",
            Some(&credential),
            &["amd64"],
        )
        .await
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(
            read.exposed_ports,
            vec![
                (53, "UDP".to_owned()),
                (8080, "TCP".to_owned()),
                (9090, "TCP".to_owned())
            ]
        );
        assert_eq!(read.user, "1000");
    }

    #[tokio::test]
    async fn a_missing_tag_and_a_silent_registry_are_said_as_such() {
        let registry = nginx();
        assert_eq!(
            read(&registry, "nginx:nope", None, &["amd64"]).await,
            Err(Unreadable::NotFound)
        );
        let down = Registry {
            unreachable: true,
            ..Registry::default()
        };
        assert!(matches!(
            read(&down, "nginx", None, &["amd64"]).await,
            Err(Unreadable::Unreachable(_))
        ));
    }

    #[tokio::test]
    async fn content_that_does_not_match_its_digest_is_refused() {
        let mut registry = nginx();
        for (url, answer) in &mut registry.routes {
            if url.starts_with("https://cdn.test") {
                answer.body = br#"{"config":{"ExposedPorts":{"22/tcp":{}}}}"#.to_vec();
            }
        }
        assert!(matches!(
            read(&registry, "nginx", None, &["amd64"]).await,
            Err(Unreadable::Unexpected(_))
        ));
    }

    #[test]
    fn challenges_are_read_with_quoted_and_bare_values() {
        assert_eq!(
            challenge_params(
                r#"realm="https://auth.docker.io/token",service="registry.docker.io", scope=x"#
            ),
            vec![
                (
                    "realm".to_owned(),
                    "https://auth.docker.io/token".to_owned()
                ),
                ("service".to_owned(), "registry.docker.io".to_owned()),
                ("scope".to_owned(), "x".to_owned()),
            ]
        );
    }

    #[test]
    fn redirects_stay_on_http_and_keep_their_origin() {
        assert_eq!(origin("https://a.test:5000/v2/x"), "https://a.test:5000");
        assert_eq!(
            join("https://a.test/v2/x", "/v2/y").ok().as_deref(),
            Some("https://a.test/v2/y")
        );
        assert!(join("https://a.test/v2/x", "file:///etc/passwd").is_err());
        assert!(join("https://a.test/v2/x", "//b.test/y").is_err());
    }
}
