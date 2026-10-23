//! Zeabur Builder: images built from a Git ref, from its `Build` and `BuildRun` objects
//! [builder].
//!
//! A `Build` follows a ref and keeps an image built from it; `status.latestImage` is the newest
//! successful one, `repository:tag@sha256:…`, and `spec.output.image` the repository it pushes
//! to when set (otherwise the controller's default registry, which only the first image shows).
//! A `BuildRun` is one build of one commit, labelled with its `Build`; its Job and pod are named
//! after it and live next to it.
//!
//! [builder]: https://github.com/zeabur/builder

use serde::Serialize;
use utoipa::ToSchema;
use zeabur_kube::DynamicObject;

use crate::{Module, Read, text};

/// The module.
pub static MODULE: Module = Module {
    id: "zeabur-builder",
    capability: "builds",
    reads: &[BUILDS, BUILD_RUNS],
    http: &[],
};

/// `builder.zeabur.com/v1alpha1` `Build`, in the project's namespace.
pub const BUILDS: Read = Read {
    group: "builder.zeabur.com",
    version: "v1alpha1",
    resource: "builds",
    kind: "Build",
};

/// `builder.zeabur.com/v1alpha1` `BuildRun`, in the project's namespace.
pub const BUILD_RUNS: Read = Read {
    group: "builder.zeabur.com",
    version: "v1alpha1",
    resource: "buildruns",
    kind: "BuildRun",
};

/// The label a `BuildRun` and its pod carry with the `BuildRun`'s name.
pub const RUN_LABEL: &str = "builder.zeabur.com/buildrun";

/// A `Build`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZeaburBuilderBuild {
    /// Its name.
    pub name: String,
    /// The repository it pushes to, without tag or digest; unknown until the first image when
    /// the controller's default registry is used.
    pub repository: Option<String>,
    /// The Git repository.
    pub git_url: Option<String>,
    /// The branch or tag it follows.
    pub git_ref: Option<String>,
    /// The `ClusterBuilder`.
    pub builder: Option<String>,
    /// Whether builds are paused.
    pub suspended: bool,
    /// The newest successful image, `repository:tag@sha256:…`.
    pub latest_image: Option<String>,
    /// The commit it was built from.
    pub latest_revision: Option<String>,
    /// The commit the ref points at.
    pub remote_revision: Option<String>,
    /// The `Ready` condition: `True`, `False` or `Unknown`.
    pub ready: Option<String>,
    /// Its reason and message, as the controller wrote them.
    pub reason: Option<String>,
    /// The message.
    pub message: Option<String>,
    /// The Keel webhooks it tells of new images.
    pub keel_webhooks: Vec<String>,
    /// The value of the rebuild annotation, when set.
    pub rebuild_requested: Option<String>,
}

/// A `BuildRun`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZeaburBuilderRun {
    /// Its name, also its Job's and its pod's label.
    pub name: String,
    /// The `Build`.
    pub build: String,
    /// The commit.
    pub revision: Option<String>,
    /// `webhook`, `poll`, `manual`, `baseImage` or `replan`.
    pub trigger: Option<String>,
    /// `Pending`, `Planning`, `Building`, `Succeeded`, `Failed` or `Cancelled`.
    pub phase: Option<String>,
    /// Why it waits or failed.
    pub reason: Option<String>,
    /// Details, at most 1 KiB of the Job's output.
    pub message: Option<String>,
    /// When it was created.
    pub created: Option<String>,
    /// When it started.
    pub started: Option<String>,
    /// When it finished.
    pub finished: Option<String>,
    /// The tags pushed.
    pub tags: Vec<String>,
    /// The pushed manifest's digest.
    pub digest: Option<String>,
}

/// The repository of an image reference: without its digest and tag.
#[must_use]
pub fn repository(image: &str) -> &str {
    let image = image.split_once('@').map_or(image, |(name, _)| name);
    match image.rsplit_once(':') {
        Some((name, tag)) if !tag.contains('/') => name,
        _ => image,
    }
}

fn condition<'a>(object: &'a DynamicObject, kind: &str) -> Option<&'a serde_json::Value> {
    object
        .data
        .pointer("/status/conditions")
        .and_then(serde_json::Value::as_array)?
        .iter()
        .find(|c| c["type"] == kind)
}

fn field(value: Option<&serde_json::Value>, key: &str) -> Option<String> {
    value
        .and_then(|v| v[key].as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// The `Build`s, sorted by name.
#[must_use]
pub fn builds(objects: &[DynamicObject]) -> Vec<ZeaburBuilderBuild> {
    let mut out: Vec<ZeaburBuilderBuild> = objects
        .iter()
        .map(|b| {
            let latest_image = text(b, "/status/latestImage");
            let ready = condition(b, "Ready");
            ZeaburBuilderBuild {
                name: b.metadata.name.clone().unwrap_or_default(),
                repository: text(b, "/spec/output/image")
                    .or_else(|| latest_image.as_deref().map(|i| repository(i).to_owned())),
                git_url: text(b, "/spec/source/git/url"),
                git_ref: text(b, "/spec/source/git/ref"),
                builder: text(b, "/spec/builderRef"),
                suspended: b
                    .data
                    .pointer("/spec/suspend")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                latest_image,
                latest_revision: text(b, "/status/latestRevision"),
                remote_revision: text(b, "/status/remoteRevision"),
                ready: field(ready, "status"),
                reason: field(ready, "reason"),
                message: field(ready, "message"),
                keel_webhooks: b
                    .data
                    .pointer("/spec/notify")
                    .and_then(serde_json::Value::as_array)
                    .map_or_else(Vec::new, |targets| {
                        targets
                            .iter()
                            .filter(|t| t["type"] == "keel")
                            .filter_map(|t| t["url"].as_str().map(str::to_owned))
                            .collect()
                    }),
                rebuild_requested: b
                    .metadata
                    .annotations
                    .as_ref()
                    .and_then(|a| a.get("builder.zeabur.com/rebuild").cloned()),
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The `BuildRun`s, newest first.
#[must_use]
pub fn runs(objects: &[DynamicObject]) -> Vec<ZeaburBuilderRun> {
    let mut out: Vec<ZeaburBuilderRun> = objects
        .iter()
        .filter_map(|r| {
            Some(ZeaburBuilderRun {
                name: r.metadata.name.clone()?,
                build: text(r, "/spec/buildRef")?,
                revision: text(r, "/spec/revision"),
                trigger: text(r, "/spec/trigger"),
                phase: text(r, "/status/phase"),
                reason: text(r, "/status/reason"),
                message: text(r, "/status/message"),
                created: r
                    .metadata
                    .creation_timestamp
                    .as_ref()
                    .map(|t| t.0.to_string()),
                started: text(r, "/status/startTime"),
                finished: text(r, "/status/completionTime"),
                tags: crate::strings(r, "/status/tags"),
                digest: text(r, "/status/imageDigest"),
            })
        })
        .collect();
    out.sort_by(|a, b| (b.created.as_deref(), &b.name).cmp(&(a.created.as_deref(), &a.name)));
    out
}

/// The reason a service waiting for its first image shows when the build failed.
pub const BUILD_FAILED: &str = "BuildFailed";

/// Gives a service whose image a `Build` has not pushed yet the build's status.
///
/// It is building, not failing to pull an image that does not exist. A container runs a `Build`'s
/// image when the repositories match, the `Build`'s own (`spec.output.image`), as a service
/// deployed from Git has. When the newest run failed, or the `Build` is not ready and nothing runs,
/// the service has failed with [`BUILD_FAILED`]. Diagnoses of the image pull go: the build is the
/// cause.
pub fn first_image(
    service: &mut zeabur_model::Service,
    builds: &[ZeaburBuilderBuild],
    runs: &[ZeaburBuilderRun],
) {
    let waiting = builds.iter().find(|build| {
        build.latest_image.is_none()
            && build.repository.as_deref().is_some_and(|wanted| {
                service.containers.iter().any(|c| {
                    c.image
                        .as_deref()
                        .is_some_and(|image| repository(image).eq_ignore_ascii_case(wanted))
                })
            })
    });
    let Some(build) = waiting else {
        return;
    };
    let newest = runs.iter().find(|run| run.build == build.name);
    let running = newest.is_some_and(|run| {
        !matches!(
            run.phase.as_deref(),
            Some("Succeeded" | "Failed" | "Cancelled")
        )
    });
    let failed = newest.is_some_and(|run| run.phase.as_deref() == Some("Failed"))
        || (!running && build.ready.as_deref() == Some("False"));
    service.status = if failed {
        zeabur_model::ServiceStatus {
            health: zeabur_model::Health::Fail,
            word: zeabur_model::StatusWord::Failed,
            reason: Some(BUILD_FAILED.to_owned()),
        }
    } else {
        zeabur_model::ServiceStatus {
            health: zeabur_model::Health::Progress,
            word: zeabur_model::StatusWord::Building,
            reason: None,
        }
    };
    service
        .diagnoses
        .retain(|d| d.kind != zeabur_model::DiagnosisKind::ImagePull);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(value: serde_json::Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn repository_drops_tag_and_digest_but_not_a_port() {
        assert_eq!(
            repository("zot.zot.svc:5000/demo/web:main-1-abc@sha256:00"),
            "zot.zot.svc:5000/demo/web"
        );
        assert_eq!(
            repository("zot.zot.svc:5000/demo/web"),
            "zot.zot.svc:5000/demo/web"
        );
        assert_eq!(repository("nginx:1.27"), "nginx");
        assert_eq!(repository("nginx@sha256:00"), "nginx");
    }

    #[test]
    fn a_built_build_with_keel() {
        let build = object(serde_json::json!({
            "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
            "metadata": {"name": "web", "namespace": "demo",
                "annotations": {"builder.zeabur.com/rebuild": "2026-10-23T08:00:00Z"}},
            "spec": {"builderRef": "railpack",
                "source": {"git": {"url": "https://example.com/acme/web", "ref": "main"}},
                "notify": [{"type": "keel", "url": "http://keel:9300/v1/webhooks/native"},
                           {"type": "generic", "url": "https://example.com/hook"}]},
            "status": {"latestImage": "zot:5000/demo/web:main-1-abc@sha256:00",
                "conditions": [{"type": "Ready", "status": "True", "reason": "Succeeded",
                    "lastTransitionTime": "2026-10-23T08:00:00Z"}]}
        }));
        let found = builds(&[build]);
        assert_eq!(found[0].repository.as_deref(), Some("zot:5000/demo/web"));
        assert_eq!(found[0].ready.as_deref(), Some("True"));
        assert_eq!(
            found[0].keel_webhooks,
            ["http://keel:9300/v1/webhooks/native"]
        );
        assert_eq!(
            found[0].rebuild_requested.as_deref(),
            Some("2026-10-23T08:00:00Z")
        );
    }

    #[test]
    fn the_output_repository_wins_and_runs_are_newest_first() {
        let build = object(serde_json::json!({
            "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
            "metadata": {"name": "api"},
            "spec": {"builderRef": "dockerfile", "output": {"image": "ghcr.io/acme/api"},
                "source": {"git": {"url": "https://example.com/acme/api", "ref": "main"}}}
        }));
        assert_eq!(
            builds(&[build])[0].repository.as_deref(),
            Some("ghcr.io/acme/api")
        );
        let run = |name: &str, created: &str| {
            object(serde_json::json!({
                "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "BuildRun",
                "metadata": {"name": name, "creationTimestamp": created},
                "spec": {"buildRef": "api", "revision": "1ce75d01b6978863647da42557a707a479da3a51"},
                "status": {"phase": "Failed", "reason": "BuildFailed"}
            }))
        };
        let found = runs(&[
            run("api-a", "2026-10-21T18:44:23Z"),
            run("api-b", "2026-10-22T18:44:23Z"),
        ]);
        assert_eq!(found[0].name, "api-b");
        assert_eq!(found[1].reason.as_deref(), Some("BuildFailed"));
    }
}
