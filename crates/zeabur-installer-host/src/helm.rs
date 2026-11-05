//! Helm releases as Helm's Secret driver stores them: one Secret per revision, labelled
//! `owner=helm`, `name`, `status` and `version`, whose `release` key holds the release as
//! base64 of gzipped JSON.

use std::io::Read as _;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use flate2::read::GzDecoder;
use serde_json::Value;
use zeabur_installer_plan::helm::{Revision, revision_secret};

use crate::HostError;
use crate::kubectl::Kubectl;

/// A revision as Helm stored it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// Helm's status.
    pub status: String,
    /// Helm's description, which names the error of a failed revision.
    pub description: String,
    /// The version of the chart it installed.
    pub chart_version: String,
    /// The values it was given, `null` when none.
    pub config: Value,
}

/// The release's revisions; empty when Helm has not installed it.
///
/// # Errors
///
/// When `kubectl` fails.
pub fn history(kubectl: &Kubectl, namespace: &str, name: &str) -> Result<Vec<Revision>, HostError> {
    let selector = format!("--selector=owner=helm,name={name}");
    let out = kubectl.run(
        &[
            "get",
            "secrets",
            "--namespace",
            namespace,
            &selector,
            "--output=jsonpath={range .items[*]}{.metadata.labels.version} {.metadata.labels.status}{\"\\n\"}{end}",
        ],
        None,
    )?;
    Ok(out
        .lines()
        .filter_map(|line| {
            let (version, status) = line.split_once(' ')?;
            Some(Revision {
                version: version.parse().ok()?,
                status: status.to_owned(),
            })
        })
        .collect())
}

/// One revision; `None` when its Secret is gone.
///
/// # Errors
///
/// When `kubectl` fails or the Secret does not hold a release.
pub fn release(
    kubectl: &Kubectl,
    namespace: &str,
    name: &str,
    version: u64,
) -> Result<Option<Release>, HostError> {
    let secret_name = revision_secret(name, version);
    let Some(secret) = kubectl.get("secret", &secret_name, Some(namespace))? else {
        return Ok(None);
    };
    let data = secret
        .pointer("/data/release")
        .and_then(Value::as_str)
        .unwrap_or_default();
    decode(data).map(Some).map_err(|reason| HostError::Parse {
        what: format!("Secret {namespace}/{secret_name}"),
        reason,
    })
}

/// The release from the `release` key of its Secret, as `kubectl` prints it.
fn decode(data: &str) -> Result<Release, String> {
    let helm = STANDARD.decode(data).map_err(|e| e.to_string())?;
    let gzipped = STANDARD.decode(&helm).map_err(|e| e.to_string())?;
    let mut json = Vec::new();
    GzDecoder::new(gzipped.as_slice())
        .read_to_end(&mut json)
        .map_err(|e| e.to_string())?;
    let release: Value = serde_json::from_slice(&json).map_err(|e| e.to_string())?;
    let text = |pointer: &str| {
        release
            .pointer(pointer)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    Ok(Release {
        status: text("/info/status"),
        description: text("/info/description"),
        chart_version: text("/chart/metadata/version"),
        config: release.get("config").cloned().unwrap_or(Value::Null),
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use serde_json::json;

    use super::*;

    #[test]
    fn a_release_reads_back_from_its_secret() {
        let release = json!({
            "name": "zot",
            "version": 3,
            "info": {"status": "failed", "description": "Upgrade \"zot\" failed"},
            "chart": {"metadata": {"name": "zot", "version": "0.1.60"}},
            "config": {"pvc": {"storage": "20Gi"}},
        });
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        gzip.write_all(release.to_string().as_bytes()).unwrap();
        let helm = STANDARD.encode(gzip.finish().unwrap());
        assert_eq!(
            decode(&STANDARD.encode(helm)).unwrap(),
            Release {
                status: "failed".to_owned(),
                description: "Upgrade \"zot\" failed".to_owned(),
                chart_version: "0.1.60".to_owned(),
                config: json!({"pvc": {"storage": "20Gi"}}),
            }
        );
    }

    #[test]
    fn a_secret_that_is_not_a_release_is_an_error() {
        assert!(decode(&STANDARD.encode("not base64 of gzip")).is_err());
    }
}
