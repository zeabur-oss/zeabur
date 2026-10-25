//! Helm charts downloaded by the installer and checked against the catalog's digest, so
//! helm-controller installs the checked archive and not what the repository serves later.

use std::collections::BTreeMap;

use serde::Deserialize;
use ureq::Agent;
use ureq::http::Response;

use crate::HostError;
use crate::download::check;

/// The largest chart or repository index read.
const MAX: u64 = 64 << 20;
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
    if let Some(reference) = chart.strip_prefix("oci://") {
        return oci(agent, reference, digest);
    }
    let repo = repo.ok_or_else(|| HostError::Parse {
        what: chart.to_owned(),
        reason: "no repository".to_owned(),
    })?;
    let index_url = format!("{}/index.yaml", repo.trim_end_matches('/'));
    let index = String::from_utf8_lossy(&read(get(agent, &index_url, None, None)?, &index_url)?)
        .into_owned();
    let url = archive_url(&index, repo, chart, version).map_err(|reason| HostError::Parse {
        what: index_url.clone(),
        reason,
    })?;
    let archive = read(get(agent, &url, None, None)?, &url)?;
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
fn oci(agent: &Agent, reference: &str, digest: &str) -> Result<Vec<u8>, HostError> {
    let (host, path) = reference.split_once('/').ok_or_else(|| HostError::Parse {
        what: reference.to_owned(),
        reason: "no registry host".to_owned(),
    })?;
    let manifest_url = format!("https://{host}/v2/{path}/manifests/{digest}");
    let mut token = None;
    let manifest = registry(agent, &manifest_url, Some(OCI_MANIFEST), &mut token)?;
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
    let archive = registry(agent, &blob_url, None, &mut token)?;
    check(reference, &layer.digest, &archive)?;
    Ok(archive)
}

/// A registry GET; on 401 it takes an anonymous token from the challenge's realm once
/// [token-auth].
///
/// [token-auth]: https://distribution.github.io/distribution/spec/auth/token/
fn registry(
    agent: &Agent,
    url: &str,
    accept: Option<&str>,
    token: &mut Option<String>,
) -> Result<Vec<u8>, HostError> {
    let response = get(agent, url, accept, token.as_deref())?;
    if response.status() != 401 || token.is_some() {
        return read(response, url);
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
    let mut request = agent.get(*realm);
    for key in ["service", "scope"] {
        if let Some(value) = params.get(key) {
            request = request.query(key, *value);
        }
    }
    let answer = request
        .call()
        .map_err(|e| download_error(realm, e.to_string()))?;
    let answer: Token =
        serde_json::from_slice(&read(answer, realm)?).map_err(|e| HostError::Parse {
            what: (*realm).to_owned(),
            reason: e.to_string(),
        })?;
    *token = answer.token.or(answer.access_token);
    read(get(agent, url, accept, token.as_deref())?, url)
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

fn get(
    agent: &Agent,
    url: &str,
    accept: Option<&str>,
    token: Option<&str>,
) -> Result<Response<ureq::Body>, HostError> {
    let mut request = agent.get(url).config().http_status_as_error(false).build();
    if let Some(accept) = accept {
        request = request.header("Accept", accept);
    }
    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    request
        .call()
        .map_err(|e| download_error(url, e.to_string()))
}

fn read(response: Response<ureq::Body>, url: &str) -> Result<Vec<u8>, HostError> {
    let status = response.status();
    if !status.is_success() {
        return Err(download_error(url, format!("HTTP {status}")));
    }
    response
        .into_body()
        .with_config()
        .limit(MAX)
        .read_to_vec()
        .map_err(|e| download_error(url, e.to_string()))
}

fn download_error(url: &str, reason: String) -> HostError {
    HostError::Download {
        url: url.to_owned(),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
