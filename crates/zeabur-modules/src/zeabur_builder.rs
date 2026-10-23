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

use k8s_openapi::jiff::{SignedDuration, Timestamp};
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
    /// The branch or tag it follows; `HEAD` (or none) follows the repository's default branch.
    pub git_ref: Option<String>,
    /// The ref the newest commit came from, such as `refs/heads/master`, when the controller
    /// reports it.
    pub resolved_ref: Option<String>,
    /// The Secret it reads the repository's credentials from.
    pub credentials: Option<String>,
    /// The build context inside the repository; none for its root.
    pub context_dir: Option<String>,
    /// The Dockerfile, relative to the context; none for `Dockerfile`.
    pub dockerfile: Option<String>,
    /// The build variables, by name. A literal value is left out.
    pub env: Vec<ZeaburBuilderEnv>,
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
    /// Why it cannot build: `SourceReady` or `BuilderReady` when `False`.
    pub problem: Option<ZeaburBuilderProblem>,
    /// The Keel webhooks it tells of new images.
    pub keel_webhooks: Vec<String>,
    /// The value of the rebuild annotation, when set.
    pub rebuild_requested: Option<String>,
}

/// A build variable of a `Build`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZeaburBuilderEnv {
    /// The name.
    pub name: String,
    /// The Secret the value comes from; none for a literal value.
    pub secret: Option<String>,
    /// The key in that Secret.
    pub key: Option<String>,
}

fn env(object: &DynamicObject) -> Vec<ZeaburBuilderEnv> {
    object
        .data
        .pointer("/spec/env")
        .and_then(serde_json::Value::as_array)
        .map_or_else(Vec::new, |env| {
            env.iter()
                .filter_map(|e| {
                    let from = e.pointer("/valueFrom/secretKeyRef");
                    Some(ZeaburBuilderEnv {
                        name: e["name"].as_str()?.to_owned(),
                        secret: field(from, "name"),
                        key: field(from, "key"),
                    })
                })
                .collect()
        })
}

/// A `Build` that cannot build: the ref cannot be resolved (`SourceReady`) or its
/// `ClusterBuilder` is missing (`BuilderReady`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZeaburBuilderProblem {
    /// `SourceReady` or `BuilderReady`.
    pub condition: String,
    /// The controller's reason, such as `RefNotFound`, `AuthFailed` or `HostKeyMismatch`.
    pub reason: Option<String>,
    /// The controller's message.
    pub message: Option<String>,
    /// When the condition turned `False`.
    pub since: Option<String>,
    /// Whether it needs someone to act. Only `ResolveFailed` (a timeout or a server error) may
    /// clear by itself, so it counts once it has lasted [`TRANSIENT`].
    pub lasting: bool,
}

/// How long a `ResolveFailed` may last before it counts as a failure.
pub const TRANSIENT: SignedDuration = SignedDuration::from_mins(5);

fn problem(object: &DynamicObject, now: Option<Timestamp>) -> Option<ZeaburBuilderProblem> {
    ["SourceReady", "BuilderReady"]
        .into_iter()
        .find_map(|kind| {
            let found = condition(object, kind).filter(|c| c["status"] == "False")?;
            let reason = field(Some(found), "reason");
            let since = field(Some(found), "lastTransitionTime");
            let lasting = reason.as_deref() != Some("ResolveFailed")
                || match (
                    now,
                    since.as_deref().and_then(|s| s.parse::<Timestamp>().ok()),
                ) {
                    (Some(now), Some(since)) => now.duration_since(since) >= TRANSIENT,
                    _ => true,
                };
            Some(ZeaburBuilderProblem {
                condition: kind.to_owned(),
                reason,
                message: field(Some(found), "message"),
                since,
                lasting,
            })
        })
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
    /// The ref the commit came from, such as `refs/heads/master`.
    pub git_ref: Option<String>,
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

/// The `Build`s, sorted by name, read at `now` (see [`ZeaburBuilderProblem::lasting`]).
#[must_use]
pub fn builds(objects: &[DynamicObject], now: Option<Timestamp>) -> Vec<ZeaburBuilderBuild> {
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
                resolved_ref: text(b, "/status/resolvedRef"),
                credentials: text(b, "/spec/source/credentialsRef/name"),
                context_dir: text(b, "/spec/source/git/contextDir"),
                dockerfile: text(b, "/spec/source/git/dockerfile"),
                env: env(b),
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
                problem: problem(b, now),
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
                git_ref: text(r, "/spec/ref"),
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

/// The reason a service shows when the build of its image failed.
pub const BUILD_FAILED: &str = "BuildFailed";

/// Gives a service that runs a `Build`'s image the build's status when it matters.
///
/// A container runs a `Build`'s image when the repositories match, the `Build`'s own
/// (`spec.output.image`), as a service deployed from Git has. The build has failed when the
/// newest run failed, or nothing runs and the `Build` is not ready or has a lasting
/// [`ZeaburBuilderProblem`], such as a branch that does not exist or a deploy key the host
/// refuses.
///
/// Before the first image the service is building, not failing to pull an image that does not
/// exist, or it has failed with [`BUILD_FAILED`]; its diagnoses go, since the missing image and
/// the replicas it leaves unready both come from the build. After it, a failed build fails a
/// service that is otherwise well with [`BUILD_FAILED`] too: what it serves stays, but no new
/// commit reaches it until someone acts.
pub fn built_image(
    service: &mut zeabur_model::Service,
    builds: &[ZeaburBuilderBuild],
    runs: &[ZeaburBuilderRun],
) {
    let found = builds.iter().find(|build| {
        build.repository.as_deref().is_some_and(|wanted| {
            service.containers.iter().any(|c| {
                c.image
                    .as_deref()
                    .is_some_and(|image| repository(image).eq_ignore_ascii_case(wanted))
            })
        })
    });
    let Some(build) = found else {
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
        || (!running
            && (build.ready.as_deref() == Some("False")
                || build.problem.as_ref().is_some_and(|p| p.lasting)));
    let build_failed = |word| zeabur_model::ServiceStatus {
        health: zeabur_model::Health::Fail,
        word,
        reason: Some(BUILD_FAILED.to_owned()),
    };
    if build.latest_image.is_some() {
        if failed && service.status.health != zeabur_model::Health::Fail {
            service.status = build_failed(zeabur_model::StatusWord::RolloutStuck);
        }
        return;
    }
    service.status = if failed {
        build_failed(zeabur_model::StatusWord::Failed)
    } else {
        zeabur_model::ServiceStatus {
            health: zeabur_model::Health::Progress,
            word: zeabur_model::StatusWord::Building,
            reason: None,
        }
    };
    service.diagnoses.clear();
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
    fn build_options_show_variable_names_but_never_a_literal_value() {
        let build = object(serde_json::json!({
            "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
            "metadata": {"name": "web", "namespace": "demo"},
            "spec": {"builderRef": "dockerfile",
                "source": {"git": {"url": "https://example.com/acme/web",
                    "contextDir": "apps/web", "dockerfile": "Dockerfile.prod"}},
                "env": [{"name": "NODE_ENV", "value": "production"},
                        {"name": "NPM_TOKEN", "valueFrom": {"secretKeyRef":
                            {"name": "web-build-env", "key": "NPM_TOKEN"}}}]}
        }));
        let found = builds(&[build], None);
        assert_eq!(found[0].context_dir.as_deref(), Some("apps/web"));
        assert_eq!(found[0].dockerfile.as_deref(), Some("Dockerfile.prod"));
        assert_eq!(
            found[0].env,
            [
                ZeaburBuilderEnv {
                    name: "NODE_ENV".to_owned(),
                    secret: None,
                    key: None
                },
                ZeaburBuilderEnv {
                    name: "NPM_TOKEN".to_owned(),
                    secret: Some("web-build-env".to_owned()),
                    key: Some("NPM_TOKEN".to_owned()),
                },
            ]
        );
        assert!(
            !serde_json::to_string(&found[0])
                .unwrap()
                .contains("production")
        );
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
        let found = builds(&[build], None);
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
            builds(&[build], None)[0].repository.as_deref(),
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

    #[test]
    fn a_source_that_cannot_be_resolved_is_a_problem_and_a_timeout_only_once_it_lasts() {
        let build = |reason: &str, since: &str| {
            object(serde_json::json!({
                "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
                "metadata": {"name": "web"},
                "spec": {"builderRef": "railpack",
                    "source": {"git": {"url": "https://example.com/acme/web", "ref": "HEAD"}}},
                "status": {"resolvedRef": "refs/heads/master", "conditions": [
                    {"type": "BuilderReady", "status": "True", "reason": "Found"},
                    {"type": "SourceReady", "status": "False", "reason": reason,
                        "message": "no branch or tag `main`", "lastTransitionTime": since},
                    {"type": "Ready", "status": "Unknown", "reason": "NotBuilt"}]}
            }))
        };
        let now: Timestamp = "2026-10-23T08:10:00Z".parse().unwrap();
        let found = builds(&[build("RefNotFound", "2026-10-23T08:09:00Z")], Some(now));
        assert_eq!(found[0].resolved_ref.as_deref(), Some("refs/heads/master"));
        let problem = found[0].problem.as_ref().unwrap();
        assert_eq!(problem.condition, "SourceReady");
        assert_eq!(problem.reason.as_deref(), Some("RefNotFound"));
        assert_eq!(problem.message.as_deref(), Some("no branch or tag `main`"));
        assert!(problem.lasting);

        let recent = builds(&[build("ResolveFailed", "2026-10-23T08:09:00Z")], Some(now));
        assert!(!recent[0].problem.as_ref().unwrap().lasting);
        let old = builds(&[build("ResolveFailed", "2026-10-23T08:00:00Z")], Some(now));
        assert!(old[0].problem.as_ref().unwrap().lasting);

        let missing = object(serde_json::json!({
            "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
            "metadata": {"name": "web"},
            "spec": {"builderRef": "nope", "source": {"git": {"url": "https://example.com/a/b"}}},
            "status": {"conditions": [{"type": "BuilderReady", "status": "False",
                "reason": "BuilderNotFound", "message": "no ClusterBuilder `nope`"}]}
        }));
        let found = builds(&[missing], Some(now));
        let problem = found[0].problem.as_ref().unwrap();
        assert_eq!(problem.condition, "BuilderReady");
        assert!(problem.lasting);
        assert_eq!(found[0].git_ref, None);
    }
}
