//! Helm charts downloaded by the installer and checked against the catalog's digest, so
//! helm-controller installs the checked archive and not what the repository serves later.

use std::collections::BTreeMap;

use http::Response;
use hyper::body::Incoming;
use serde::Deserialize;
use zeabur_http::Https;

use crate::HostError;
use crate::download::{self, Agent, check};

/// The largest chart or repository index read.
const MAX: usize = 64 << 20;
/// The layer that holds an OCI chart's archive [helm-oci].
///
/// [helm-oci]: https://helm.sh/docs/topics/registries/
const CHART_LAYER: &str = "application/vnd.cncf.helm.chart.content.v1.tar+gzip";
const OCI_MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";

/// Downloads a chart's `.tgz` and checks it against `digest` (`sha256:<hex>`): the archive's
/// own digest for a chart in the repository `repo`, the manifest's for an `oci://` chart.
///
/// # Errors
///
/// When a download fails, the chart is not in the repository, or a digest differs.
pub fn fetch(
    agent: &Agent,
    repo: Option<&str>,
    chart: &str,
    version: &str,
    digest: &str,
) -> Result<Vec<u8>, HostError> {
    agent.run(chart, async |https| {
        fetch_with(https, repo, chart, version, digest).await
    })
}

async fn fetch_with(
    https: &Https,
    repo: Option<&str>,
    chart: &str,
    version: &str,
    digest: &str,
) -> Result<Vec<u8>, HostError> {
    if let Some(reference) = chart.strip_prefix("oci://") {
        return oci(https, reference, digest).await;
    }
    let repo = repo.ok_or_else(|| HostError::Parse {
        what: chart.to_owned(),
        reason: "no repository".to_owned(),
    })?;
    let index_url = format!("{}/index.yaml", repo.trim_end_matches('/'));
    let index = read(get(https, &index_url, None, None).await?, &index_url).await?;
    let index = String::from_utf8_lossy(&index).into_owned();
    let url = archive_url(&index, repo, chart, version).map_err(|reason| HostError::Parse {
        what: index_url.clone(),
        reason,
    })?;
    let archive = read(get(https, &url, None, None).await?, &url).await?;
    check(&url, digest, &archive)?;
    Ok(archive)
}

#[derive(Deserialize)]
struct RepoIndex {
    entries: BTreeMap<String, Vec<Entry>>,
}

#[derive(Deserialize)]
struct Entry {
    version: String,
    #[serde(default)]
    urls: Vec<String>,
}

/// The archive URL of `chart` `version` in a repository's `index.yaml`; a relative URL is
/// relative to the repository [helm-repo].
///
/// [helm-repo]: https://helm.sh/docs/topics/chart_repository/#the-index-file
fn archive_url(index: &str, repo: &str, chart: &str, version: &str) -> Result<String, String> {
    let index: RepoIndex = serde_saphyr::from_str(index).map_err(|e| e.to_string())?;
    let url = index
        .entries
        .get(chart)
        .and_then(|e| e.iter().find(|e| e.version == version))
        .and_then(|e| e.urls.first())
        .ok_or_else(|| format!("no {chart} {version}"))?;
    Ok(if url.contains("://") {
        url.clone()
    } else {
        format!("{}/{url}", repo.trim_end_matches('/'))
    })
}

#[derive(Deserialize)]
struct Manifest {
    layers: Vec<Layer>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Layer {
    media_type: String,
    digest: String,
}

#[derive(Deserialize)]
struct Token {
    token: Option<String>,
    access_token: Option<String>,
}

/// Reads the manifest by its digest, then the chart layer by its own [oci-pull].
///
/// [oci-pull]: https://github.com/opencontainers/distribution-spec/blob/main/spec.md#pull
async fn oci(https: &Https, reference: &str, digest: &str) -> Result<Vec<u8>, HostError> {
    let (host, path) = reference.split_once('/').ok_or_else(|| HostError::Parse {
        what: reference.to_owned(),
        reason: "no registry host".to_owned(),
    })?;
    let manifest_url = format!("https://{host}/v2/{path}/manifests/{digest}");
    let mut token = None;
    let manifest = registry(https, &manifest_url, Some(OCI_MANIFEST), &mut token).await?;
    check(reference, digest, &manifest)?;
    let manifest: Manifest = serde_json::from_slice(&manifest).map_err(|e| HostError::Parse {
        what: manifest_url.clone(),
        reason: e.to_string(),
    })?;
    let layer = manifest
        .layers
        .iter()
        .find(|l| l.media_type == CHART_LAYER)
        .ok_or_else(|| HostError::Parse {
            what: manifest_url.clone(),
            reason: "no chart layer".to_owned(),
        })?;
    let blob_url = format!("https://{host}/v2/{path}/blobs/{}", layer.digest);
    let archive = registry(https, &blob_url, None, &mut token).await?;
    check(reference, &layer.digest, &archive)?;
    Ok(archive)
}

/// A registry GET; on 401 it takes an anonymous token from the challenge's realm once
/// [token-auth].
///
/// [token-auth]: https://distribution.github.io/distribution/spec/auth/token/
async fn registry(
    https: &Https,
    url: &str,
    accept: Option<&str>,
    token: &mut Option<String>,
) -> Result<Vec<u8>, HostError> {
    let response = get(https, url, accept, token.as_deref()).await?;
    if response.status() != 401 || token.is_some() {
        return read(response, url).await;
    }
    let challenge = response
        .headers()
        .get("www-authenticate")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let params = challenge_params(challenge);
    let realm = params
        .get("realm")
        .ok_or_else(|| download_error(url, "HTTP 401 without a token realm".to_owned()))?;
    let mut query = Vec::new();
    for key in ["service", "scope"] {
        if let Some(value) = params.get(key) {
            query.push((key, *value));
        }
    }
    let answer = download::get_retrying(https, realm, &[], &query)
        .await
        .map_err(|reason| download_error(realm, reason))?;
    let answer: Token =
        serde_json::from_slice(&read(answer, realm).await?).map_err(|e| HostError::Parse {
            what: (*realm).to_owned(),
            reason: e.to_string(),
        })?;
    *token = answer.token.or(answer.access_token);
    read(get(https, url, accept, token.as_deref()).await?, url).await
}

/// The `key="value"` pairs of a `Bearer` challenge.
fn challenge_params(header: &str) -> BTreeMap<&str, &str> {
    header
        .strip_prefix("Bearer ")
        .unwrap_or(header)
        .split("\",")
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            Some((key.trim(), value.trim().trim_matches('"')))
        })
        .collect()
}

async fn get(
    https: &Https,
    url: &str,
    accept: Option<&str>,
    token: Option<&str>,
) -> Result<Response<Incoming>, HostError> {
    let mut headers = Vec::new();
    if let Some(accept) = accept {
        headers.push(("Accept", accept));
    }
    let bearer = token.map(|t| format!("Bearer {t}"));
    if let Some(bearer) = &bearer {
        headers.push(("Authorization", bearer.as_str()));
    }
    download::get_retrying(https, url, &headers, &[])
        .await
        .map_err(|reason| download_error(url, reason))
}

async fn read(response: Response<Incoming>, url: &str) -> Result<Vec<u8>, HostError> {
    let status = response.status();
    if !status.is_success() {
        return Err(download_error(url, format!("HTTP {status}")));
    }
    download::read_all(response, MAX)
        .await
        .map_err(|reason| download_error(url, reason))
}

fn download_error(url: &str, reason: String) -> HostError {
    HostError::Download {
        url: url.to_owned(),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;

    use super::*;

    /// A chart repository on a loopback port whose `index.yaml` names `zot-0.1.128.tgz`, which
    /// holds `abc`.
    fn repository() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let repo = format!("http://{}/charts", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut request = [0_u8; 4096];
                let n = stream.read(&mut request).unwrap_or(0);
                let head = String::from_utf8_lossy(&request[..n]).into_owned();
                let (status, body) = if head.starts_with("GET /charts/index.yaml ") {
                    (
                        "200 OK",
                        "entries:\n  zot:\n  - version: 0.1.128\n    urls: [zot-0.1.128.tgz]\n",
                    )
                } else if head.starts_with("GET /charts/zot-0.1.128.tgz ") {
                    ("200 OK", "abc")
                } else {
                    ("404 Not Found", "")
                };
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                drop(stream.write_all(reply.as_bytes()));
            }
        });
        repo
    }

    #[test]
    fn a_chart_is_read_from_its_repository_and_checked() {
        let repo = repository();
        let agent = download::agent();
        let digest = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let archive = fetch(&agent, Some(&repo), "zot", "0.1.128", digest).unwrap();
        assert_eq!(archive, b"abc");
        let wrong = format!("sha256:{}", "0".repeat(64));
        assert!(matches!(
            fetch(&agent, Some(&repo), "zot", "0.1.128", &wrong),
            Err(HostError::Digest { .. })
        ));
        let missing = fetch(&agent, Some(&repo), "zot", "9.9.9", digest).unwrap_err();
        assert!(missing.to_string().contains("no zot 9.9.9"), "{missing}");
    }

    #[test]
    fn index_entries_resolve_relative_urls() {
        let index = "apiVersion: v1\nentries:\n  zot:\n  - version: 0.1.128\n    urls: [zot-0.1.128.tgz]\n  - version: 0.1.127\n    urls: [https://example.com/zot-0.1.127.tgz]\n";
        let repo = "https://zotregistry.dev/helm-charts/";
        assert_eq!(
            archive_url(index, repo, "zot", "0.1.128").unwrap(),
            "https://zotregistry.dev/helm-charts/zot-0.1.128.tgz"
        );
        assert_eq!(
            archive_url(index, repo, "zot", "0.1.127").unwrap(),
            "https://example.com/zot-0.1.127.tgz"
        );
        assert!(archive_url(index, repo, "zot", "9.9.9").is_err());
    }

    #[test]
    fn bearer_challenge() {
        let params = challenge_params(
            r#"Bearer realm="https://ghcr.io/token",service="ghcr.io",scope="repository:traefik/helm/traefik:pull""#,
        );
        assert_eq!(params.get("realm"), Some(&"https://ghcr.io/token"));
        assert_eq!(
            params.get("scope"),
            Some(&"repository:traefik/helm/traefik:pull")
        );
    }

    #[test]
    fn a_different_archive_is_refused() {
        let digest = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(check("chart", digest, b"abc").is_ok());
        assert!(matches!(
            check("chart", digest, b"abd"),
            Err(HostError::Digest { .. })
        ));
    }
}
