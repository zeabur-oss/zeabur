//! Newer templates from Zeabur Catalog, fetched by the server (docs/reference/telemetry.mdx,
//! "Template updates").
//!
//! A template list request starts a fetch in the background once 6 hours have passed since the
//! last one, or 10 minutes after a failure. It asks for `SHA256SUMS` with the last answer's `ETag`,
//! and only a changed list is followed by `SHA256SUMS.sigstore.json` and `bundle.tar.gz`. The
//! list must carry the cosign keyless signature of the catalogue's release workflow, checked
//! against Sigstore's trust root, which [`Verifier::production`] fetches from Sigstore's TUF
//! repository once per process, and the bundle must match its digest in the list. The templates
//! that pass [`Catalog::fetched`] are offered until the server stops; a failure keeps what is
//! offered.

use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use axum::http::{self, Request};
use sha2::{Digest as _, Sha256};
use sigstore::bundle::Bundle;
use sigstore::bundle::verify::policy::{
    self, GitHubWorkflowRepository, OIDCIssuer, SingleX509ExtPolicy as _,
};
use sigstore::bundle::verify::{PolicyError, VerificationPolicy, Verifier};
use tokio::sync::{Mutex, OnceCell};
use x509_cert::ext::pkix::SubjectAltName;
use x509_cert::ext::pkix::name::GeneralName;
use zeabur_catalog::{Catalog, Dropped};

use crate::images::https::Https;

/// How long after a fetch the next one is due.
const EVERY: Duration = Duration::from_hours(6);
/// How long after a failed fetch the next one is due.
const RETRY: Duration = Duration::from_mins(10);
/// How long one download may take.
const DEADLINE: Duration = Duration::from_mins(1);
/// The most `SHA256SUMS` and its signature may hold.
const SMALL: usize = 1024 * 1024;
/// The most `bundle.tar.gz` may hold; Zeabur Catalog v0.4.0's is under 300 KiB.
const BUNDLE_LIMIT: usize = 16 * 1024 * 1024;

/// The OIDC issuer of the signing certificate: GitHub Actions.
const ISSUER: &str = "https://token.actions.githubusercontent.com";
/// The repository whose workflow run signs the catalogue's releases.
const REPOSITORY: &str = "zeabur-oss/catalog";
/// The shared release workflow, at any commit, that the signing certificate names.
const WORKFLOW: &str = "https://github.com/zeabur-oss/.github/.github/workflows/release.yml@";

/// The templates offered, and how newer ones are fetched.
pub struct Templates {
    current: ArcSwap<Catalog>,
    fetch: Option<Fetch>,
}

impl std::fmt::Debug for Templates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Templates")
            .field("fetch", &self.fetch.as_ref().map(|f| &f.base))
            .finish_non_exhaustive()
    }
}

struct Fetch {
    /// Where the release files are, ending in `/`.
    base: String,
    http: Https,
    verifier: OnceCell<Verifier>,
    kept: Mutex<Kept>,
}

#[derive(Default)]
struct Kept {
    etag: Option<String>,
    /// No fetch before this.
    next: Option<Instant>,
}

/// What a fetch found: the templates and the `ETag` of their `SHA256SUMS`.
struct Fetched {
    catalog: Catalog,
    dropped: Vec<Dropped>,
    etag: Option<String>,
}

impl Templates {
    /// The templates this build carries, and no fetching.
    #[must_use]
    pub fn embedded() -> Self {
        Self {
            current: ArcSwap::from_pointee(Catalog::embedded()),
            fetch: None,
        }
    }

    /// The templates this build carries until newer ones are fetched from `base`, a URL ending
    /// in `/`, through `http`.
    #[must_use]
    pub fn fetching(base: String, http: Https) -> Self {
        Self {
            fetch: Some(Fetch {
                base,
                http,
                verifier: OnceCell::new(),
                kept: Mutex::default(),
            }),
            ..Self::embedded()
        }
    }

    /// The templates offered now.
    pub(crate) fn current(&self) -> Arc<Catalog> {
        self.current.load_full()
    }

    /// Starts [`Self::refresh`] in the background.
    pub(crate) fn refresh_in_background(self: &Arc<Self>) {
        if self.fetch.is_none() {
            return;
        }
        let templates = Arc::clone(self);
        tokio::spawn(async move { templates.refresh(Instant::now()).await });
    }

    /// Fetches newer templates when a fetch is due. One fetch runs at a time; a request that
    /// comes meanwhile does not wait for it.
    pub(crate) async fn refresh(&self, now: Instant) {
        let Some(fetch) = &self.fetch else {
            return;
        };
        let Ok(mut kept) = fetch.kept.try_lock() else {
            return;
        };
        if kept.next.is_some_and(|next| now < next) {
            return;
        }
        match fetch.fetch(kept.etag.as_deref()).await {
            Ok(None) => kept.next = Some(now + EVERY),
            Ok(Some(fetched)) => {
                for dropped in &fetched.dropped {
                    tracing::warn!(template = %dropped.id, reason = %dropped.reason, "a fetched template is not offered");
                }
                self.current.store(Arc::new(fetched.catalog));
                kept.etag = fetched.etag;
                kept.next = Some(now + EVERY);
                tracing::info!(from = %fetch.base, "fetched templates are offered");
            }
            Err(error) => {
                tracing::warn!(from = %fetch.base, %error, "templates were not fetched");
                kept.next = Some(now + RETRY);
            }
        }
    }
}

impl Fetch {
    /// The templates of the catalogue at `base`; `None` when `SHA256SUMS` still has `etag`.
    async fn fetch(&self, etag: Option<&str>) -> Result<Option<Fetched>, String> {
        let (status, headers, sums) = self.get("SHA256SUMS", etag, SMALL).await?;
        if status == 304 {
            return Ok(None);
        }
        let etag = headers
            .get(http::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let (_, _, signature) = self.get("SHA256SUMS.sigstore.json", None, SMALL).await?;
        let (_, _, bundle) = self.get("bundle.tar.gz", None, BUNDLE_LIMIT).await?;
        let verifier = self
            .verifier
            .get_or_try_init(|| async {
                Verifier::production()
                    .await
                    .map_err(|e| format!("Sigstore's trust root: {e}"))
            })
            .await?;
        let (catalog, dropped) = verified(verifier, &sums, &signature, &bundle).await?;
        Ok(Some(Fetched {
            catalog,
            dropped,
            etag,
        }))
    }

    /// `GET <base><name>`: `200`, or `304` when `etag` is given.
    async fn get(
        &self,
        name: &str,
        etag: Option<&str>,
        limit: usize,
    ) -> Result<(u16, http::HeaderMap, Vec<u8>), String> {
        let url = format!("{}{name}", self.base);
        let mut request = Request::get(&url).header(http::header::USER_AGENT, "zeabur");
        if let Some(etag) = etag {
            request = request.header(http::header::IF_NONE_MATCH, etag);
        }
        let request = request
            .body(http_body_util::Full::default())
            .map_err(|e| e.to_string())?;
        let answer = self.http.exchange_within(request, limit, DEADLINE).await;
        let answer = answer.map_err(|e| format!("{url}: {e}"))?;
        match answer.0 {
            200 => Ok(answer),
            304 if etag.is_some() => Ok(answer),
            status => Err(format!("{url}: answered {status}")),
        }
    }
}

/// The templates of `bundle`, once `signature` shows that the catalogue's release workflow signed
/// `sums` and `bundle` matches its digest there.
async fn verified(
    verifier: &Verifier,
    sums: &[u8],
    signature: &[u8],
    bundle: &[u8],
) -> Result<(Catalog, Vec<Dropped>), String> {
    verify_signature(verifier, sums, signature, REPOSITORY).await?;
    let want = digest_in(sums, "bundle.tar.gz")
        .ok_or_else(|| "SHA256SUMS does not list bundle.tar.gz".to_owned())?;
    if hex::encode(Sha256::digest(bundle)) != want {
        return Err("bundle.tar.gz does not match its digest in SHA256SUMS".to_owned());
    }
    Catalog::fetched(bundle)
}

/// Checks that a release workflow run of `repository` signed `sums`.
async fn verify_signature(
    verifier: &Verifier,
    sums: &[u8],
    signature: &[u8],
    repository: &str,
) -> Result<(), String> {
    let signature: Bundle =
        serde_json::from_slice(signature).map_err(|e| format!("SHA256SUMS.sigstore.json: {e}"))?;
    // Offline: the bundle's own transparency log entry, without asking Rekor.
    verifier
        .verify(sums, signature, &ReleaseRun { repository }, true)
        .await
        .map_err(|e| format!("the signature of SHA256SUMS: {e}"))
}

/// The digest `SHA256SUMS` lists for `name`, in lowercase hexadecimal.
fn digest_in(sums: &[u8], name: &str) -> Option<String> {
    let sums = std::str::from_utf8(sums).ok()?;
    for line in sums.lines() {
        if let Some((digest, file)) = line.split_once("  ")
            && file == name
        {
            return Some(digest.to_ascii_lowercase());
        }
    }
    None
}

/// The signing certificate is GitHub Actions' for a run of [`WORKFLOW`], at any commit, in
/// `repository`.
struct ReleaseRun<'a> {
    repository: &'a str,
}

impl VerificationPolicy for ReleaseRun<'_> {
    fn verify(&self, cert: &x509_cert::Certificate) -> policy::PolicyResult {
        OIDCIssuer::new(ISSUER).verify(cert)?;
        GitHubWorkflowRepository::new(self.repository).verify(cert)?;
        let (_, names): (bool, SubjectAltName) = cert
            .tbs_certificate()
            .get_extension()
            .ok()
            .flatten()
            .ok_or(PolicyError::ExtensionNotFound)?;
        for name in names.0 {
            if let GeneralName::UniformResourceIdentifier(uri) = name
                && uri.as_str().starts_with(WORKFLOW)
            {
                return Ok(());
            }
        }
        Err(PolicyError::ExtensionNotFound)
    }
}

#[cfg(test)]
mod tests {
    use sigstore::trust::sigstore::SigstoreTrustRoot;

    use super::*;

    /// Zeabur Catalog v0.4.0's signed checksum list, and Sigstore's trust root as sigstore 0.15
    /// ships it.
    const SUMS: &[u8] = include_bytes!("../../tests/catalog-v0.4.0/SHA256SUMS");
    const SIGNATURE: &[u8] = include_bytes!("../../tests/catalog-v0.4.0/SHA256SUMS.sigstore.json");
    const TRUSTED_ROOT: &[u8] = include_bytes!("../../tests/catalog-v0.4.0/trusted_root.json");

    fn verifier() -> Verifier {
        let root = SigstoreTrustRoot::from_trusted_root_json_unchecked(TRUSTED_ROOT).unwrap();
        Verifier::new(
            sigstore::rekor::apis::configuration::Configuration::default(),
            root,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn the_catalogues_release_signature_is_accepted() {
        assert_eq!(
            verify_signature(&verifier(), SUMS, SIGNATURE, REPOSITORY).await,
            Ok(())
        );
    }

    #[tokio::test]
    async fn a_changed_checksum_list_is_refused() {
        let mut sums = SUMS.to_vec();
        sums[0] ^= 1;
        assert!(
            verify_signature(&verifier(), &sums, SIGNATURE, REPOSITORY)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn a_signature_by_another_repository_is_refused() {
        assert!(
            verify_signature(&verifier(), SUMS, SIGNATURE, "zeabur-oss/zeabur")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn a_bundle_that_does_not_match_its_digest_is_refused() {
        let error = verified(&verifier(), SUMS, SIGNATURE, b"another bundle")
            .await
            .unwrap_err();
        assert!(error.contains("does not match"), "{error}");
    }

    /// The files asked for, with the `If-None-Match` each request named.
    type Asked = Arc<std::sync::Mutex<Vec<(String, Option<String>)>>>;

    /// Serves a release under `/v1/` as GitHub Pages does: each file with an `ETag`, and `304`
    /// to a request that names it.
    async fn serve(bundle: &'static [u8]) -> (String, Asked) {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1/", listener.local_addr().unwrap());
        let asked = Asked::default();
        let log = Arc::clone(&asked);
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                while !request.ends_with(b"\r\n\r\n") {
                    let read = socket.read(&mut buffer).await.unwrap();
                    request.extend_from_slice(&buffer[..read]);
                }
                let request = String::from_utf8(request).unwrap();
                let path = request.split(' ').nth(1).unwrap();
                let name = path.trim_start_matches("/v1/").to_owned();
                let etag = request
                    .lines()
                    .find_map(|l| l.strip_prefix("if-none-match: "))
                    .map(str::to_owned);
                log.lock().unwrap().push((name.clone(), etag.clone()));
                let body: &[u8] = match name.as_str() {
                    "SHA256SUMS" => SUMS,
                    "SHA256SUMS.sigstore.json" => SIGNATURE,
                    "bundle.tar.gz" => bundle,
                    _ => b"",
                };
                if etag.as_deref() == Some("\"v0.4.0\"") {
                    let head = "HTTP/1.1 304 Not Modified\r\nconnection: close\r\n\r\n";
                    socket.write_all(head.as_bytes()).await.unwrap();
                    continue;
                }
                let head = format!(
                    "HTTP/1.1 200 OK\r\netag: \"v0.4.0\"\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                socket.write_all(head.as_bytes()).await.unwrap();
                socket.write_all(body).await.unwrap();
            }
        });
        (url, asked)
    }

    fn fetching(url: &str) -> Templates {
        let tls = crate::images::https::tls(None).unwrap();
        let http = Https::new(crate::images::https::Reach::Any, &tls);
        let templates = Templates::fetching(url.to_owned(), http);
        if let Some(fetch) = &templates.fetch {
            assert!(fetch.verifier.set(verifier()).is_ok());
        }
        templates
    }

    fn take(asked: &Asked) -> Vec<(String, Option<String>)> {
        std::mem::take(&mut *asked.lock().unwrap())
    }

    const BUNDLE: &[u8] = include_bytes!("../../tests/catalog-v0.4.0/bundle.tar.gz");

    #[tokio::test]
    async fn a_signed_release_is_offered_then_asked_for_again_with_its_etag() {
        let (url, asked) = serve(BUNDLE).await;
        let templates = fetching(&url);
        let embedded = templates.current();
        let start = Instant::now();

        templates.refresh(start).await;
        let fetched = templates.current();
        assert!(
            !Arc::ptr_eq(&embedded, &fetched),
            "the fetched templates are offered"
        );
        assert_eq!(
            fetched.templates(),
            Catalog::fetched(BUNDLE).unwrap().0.templates()
        );
        let names: Vec<String> = take(&asked).into_iter().map(|(name, _)| name).collect();
        assert!(names.contains(&"bundle.tar.gz".to_owned()), "{names:?}");

        templates
            .refresh(start + EVERY.saturating_sub(Duration::from_secs(1)))
            .await;
        assert_eq!(take(&asked), Vec::new(), "not due yet");

        templates.refresh(start + EVERY).await;
        assert_eq!(
            take(&asked),
            vec![("SHA256SUMS".to_owned(), Some("\"v0.4.0\"".to_owned()))],
            "an unchanged list is all that is fetched"
        );
        assert!(Arc::ptr_eq(&fetched, &templates.current()));
    }

    #[tokio::test]
    async fn a_release_that_fails_its_check_keeps_the_templates_and_is_tried_again() {
        let (url, asked) = serve(b"not the signed bundle").await;
        let templates = fetching(&url);
        let embedded = templates.current();
        let start = Instant::now();

        templates.refresh(start).await;
        assert!(Arc::ptr_eq(&embedded, &templates.current()));
        assert_ne!(take(&asked), Vec::new());

        templates
            .refresh(start + RETRY.saturating_sub(Duration::from_secs(1)))
            .await;
        assert_eq!(take(&asked), Vec::new());
        templates.refresh(start + RETRY).await;
        assert_eq!(
            take(&asked).first(),
            Some(&("SHA256SUMS".to_owned(), None)),
            "a failed check leaves no ETag behind"
        );
    }
}
