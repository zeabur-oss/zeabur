//! Zeabur Builder: images built from a Git ref, from its `Build` and `BuildRun` objects
//! [builder].
//!
//! A `Build` follows a ref and keeps an image built from it; `status.latestImage` is the newest
//! successful one, `repository:tag@sha256:…`, and `spec.output.image` the repository it pushes
//! to when set (otherwise the controller's default registry, which only the first image shows).
//! A `BuildRun` is one build of one commit, labelled with its `Build`; its Job and pod are named
//! after it and live next to it.
//!
//! [builder]: https://github.com/zeabur-oss/builder

use k8s_openapi::jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use zeabur_builder_api::build::{Build, ConditionReason, NotifyType, SourceKind};
use zeabur_builder_api::build_run::BuildRun;
use zeabur_builder_api::common::{Condition, ConditionStatus, ConditionType};
use zeabur_builder_api::{GROUP, annotations, labels};
use zeabur_kube::DynamicObject;

use crate::{Module, Read, filled};

/// The module.
pub static MODULE: Module = Module {
    id: "zeabur-builder",
    component: Some("zeabur-builder"),
    capability: "builds",
    reads: &[BUILDS, BUILD_RUNS],
    http: &[],
};

/// `builder.zeabur.com/v1alpha1` `Build`, in the project's namespace.
pub const BUILDS: Read = Read {
    group: GROUP,
    version: "v1alpha1",
    resource: "builds",
    kind: "Build",
};

/// `builder.zeabur.com/v1alpha1` `BuildRun`, in the project's namespace.
pub const BUILD_RUNS: Read = Read {
    group: GROUP,
    version: "v1alpha1",
    resource: "buildruns",
    kind: "BuildRun",
};

/// The label a `BuildRun` and its pod carry with the `BuildRun`'s name.
pub const RUN_LABEL: &str = labels::BUILD_RUN;

/// A `Build`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZeaburBuilderBuild {
    /// Its name.
    pub name: String,
    /// The repository it pushes to, without tag or digest; unknown until the first image when
    /// the controller's default registry is used.
    pub repository: Option<String>,
    /// The Git repository; none for an uploaded folder.
    pub git_url: Option<String>,
    /// Built from an uploaded folder (`spec.source.type: archive`) instead of a Git ref.
    pub archive: bool,
    /// The uploaded folder's manifest digest, once it is named.
    pub archive_digest: Option<String>,
    /// An uploaded folder not yet named: the `Build` waits for its upload.
    pub awaiting_upload: bool,
    /// The upload token request the controller last answered
    /// (`status.uploadToken.request`).
    pub upload_request: Option<String>,
    /// When that upload token expires.
    pub upload_expires_at: Option<Timestamp>,
    /// The branch or tag it follows; `HEAD` (or none) follows the repository's default branch.
    pub git_ref: Option<String>,
    /// The ref the newest commit came from, such as `refs/heads/master`, when the controller
    /// reports it.
    pub resolved_ref: Option<String>,
    /// The Secret it reads the repository's credentials from.
    pub credentials: Option<BuildCredentials>,
    /// The build context inside the repository or folder; none for its root.
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
    /// The ports that image declares, `3000/tcp` each (`status.exposedPorts`); none when it
    /// declares none.
    pub exposed_ports: Option<Vec<String>>,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZeaburBuilderEnv {
    /// The name.
    pub name: String,
    /// The Secret the value comes from; none for a literal value.
    pub secret: Option<String>,
    /// The key in that Secret.
    pub key: Option<String>,
}

fn env(build: &Build) -> Vec<ZeaburBuilderEnv> {
    build
        .spec
        .env
        .iter()
        .map(|e| {
            let from = e.value_from.as_ref().map(|v| &v.secret_key_ref);
            ZeaburBuilderEnv {
                name: e.name.clone(),
                secret: from.and_then(|r| filled(Some(&r.name))),
                key: from.and_then(|r| filled(Some(&r.key))),
            }
        })
        .collect()
}

/// A `Build` that cannot build: the ref cannot be resolved (`SourceReady`) or its
/// `ClusterBuilder` is missing (`BuilderReady`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZeaburBuilderProblem {
    /// `SourceReady` or `BuilderReady`.
    pub condition: String,
    /// The controller's reason, such as `RefNotFound`, `AuthFailed` or `HostKeyMismatch`.
    pub reason: Option<String>,
    /// The controller's message.
    pub message: Option<String>,
    /// When the condition turned `False`.
    pub started_at: Option<Timestamp>,
    /// Whether it needs someone to act. Only `ResolveFailed` (a timeout or a server error) may
    /// clear by itself, so it counts once it has lasted [`TRANSIENT`].
    pub lasting: bool,
}

/// How long a `ResolveFailed` may last before it counts as a failure.
pub const TRANSIENT: SignedDuration = SignedDuration::from_mins(5);

const fn awaiting_upload(build: &Build) -> bool {
    matches!(&build.spec.source.kind, SourceKind::Archive { archive } if archive.digest.is_none())
}

/// The `Build`'s condition of type `kind`.
fn condition(build: &Build, kind: ConditionType) -> Option<&Condition> {
    build
        .status
        .as_ref()
        .and_then(|s| s.conditions.iter().find(|c| c.kind == kind))
}

fn problem(build: &Build, now: Option<Timestamp>) -> Option<ZeaburBuilderProblem> {
    let waiting = ConditionReason::WaitingForUpload.to_string();
    let transient = ConditionReason::ResolveFailed.to_string();
    [ConditionType::SourceReady, ConditionType::BuilderReady]
        .into_iter()
        .find_map(|kind| {
            let found = condition(build, kind)
                .filter(|c| c.status == ConditionStatus::False && c.reason != waiting)?;
            let since = Some(found.last_transition_time);
            let lasting = found.reason != transient
                || match (now, since) {
                    (Some(now), Some(since)) => now.duration_since(since) >= TRANSIENT,
                    _ => true,
                };
            Some(ZeaburBuilderProblem {
                condition: written(kind).unwrap_or_default(),
                reason: filled(Some(&found.reason)),
                message: filled(Some(&found.message)),
                started_at: since,
                lasting,
            })
        })
}

/// A `BuildRun`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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
    /// `webhook`, `poll` or `manual`.
    pub trigger: Option<String>,
    /// `Pending`, `Planning`, `Building`, `Succeeded`, `Failed` or `Cancelled`.
    pub phase: Option<String>,
    /// Why it waits or failed.
    pub reason: Option<String>,
    /// Details, at most 1 KiB of the Job's output.
    pub message: Option<String>,
    /// When it was created.
    pub created_at: Option<Timestamp>,
    /// When it started.
    pub started_at: Option<Timestamp>,
    /// When it finished.
    pub finished_at: Option<Timestamp>,
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

/// The Secret a `Build` reads its repository's credentials from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BuildCredentials {
    /// Its name.
    pub name: String,
    /// Whether it is the one Zeabur made for this `Build` (`<name>-git`), which goes with it.
    pub own: bool,
}

/// The Secret Zeabur makes for a `Build`'s repository credentials.
#[must_use]
pub fn credentials_name(build: &str) -> String {
    format!("{build}-git")
}

/// How a value of Zeabur Builder's types is written in its objects, such as `Succeeded`.
fn written<T: Serialize>(value: T) -> Option<String> {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(text)) => Some(text),
        _ => None,
    }
}

/// The `Build`s, sorted by name, read at `now` (see [`ZeaburBuilderProblem::lasting`]). An object
/// that is not a `Build` of the Zeabur Builder release Zeabur is built against is left out.
#[must_use]
pub fn builds(objects: &[DynamicObject], now: Option<Timestamp>) -> Vec<ZeaburBuilderBuild> {
    let mut out: Vec<ZeaburBuilderBuild> = objects
        .iter()
        .filter_map(|object| object.clone().try_parse::<Build>().ok())
        .map(|build| read_build(&build, now))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn read_build(build: &Build, now: Option<Timestamp>) -> ZeaburBuilderBuild {
    let (spec, status) = (&build.spec, build.status.clone().unwrap_or_default());
    let source = &spec.source;
    let git = source.git();
    let archive = match &source.kind {
        SourceKind::Archive { archive } => Some(archive),
        SourceKind::Git { .. } => None,
    };
    let latest_image = filled(status.latest_image.as_ref());
    let ready = condition(build, ConditionType::Ready);
    let name = build.metadata.name.clone().unwrap_or_default();
    let credentials = source
        .credentials_ref()
        .and_then(|c| filled(Some(&c.name)))
        .map(|secret| BuildCredentials {
            own: secret == credentials_name(&name),
            name: secret,
        });
    let upload_token = status.upload_token.as_ref();
    let text = |value: Option<&str>| value.filter(|v| !v.is_empty()).map(str::to_owned);
    ZeaburBuilderBuild {
        credentials,
        name,
        repository: spec
            .output
            .as_ref()
            .and_then(|o| filled(o.image.as_ref()))
            .or_else(|| latest_image.as_deref().map(|i| repository(i).to_owned())),
        git_url: git.and_then(|g| filled(Some(&g.url))),
        git_ref: git.and_then(|g| filled(Some(&g.git_ref))),
        resolved_ref: filled(status.resolved_ref.as_ref()),
        archive: archive.is_some(),
        archive_digest: filled(archive.and_then(|a| a.digest.as_ref())),
        awaiting_upload: awaiting_upload(build),
        upload_request: upload_token.and_then(|t| filled(Some(&t.request))),
        upload_expires_at: upload_token.map(|t| t.expires_at),
        context_dir: text(source.context_dir.as_deref()),
        dockerfile: text(source.dockerfile.as_deref()),
        env: env(build),
        builder: filled(Some(&spec.builder_ref)),
        suspended: spec.suspend,
        latest_image,
        exposed_ports: (!status.exposed_ports.is_empty()).then(|| status.exposed_ports.clone()),
        latest_revision: filled(status.latest_revision.as_ref()),
        remote_revision: filled(status.remote_revision.as_ref()),
        ready: ready.and_then(|c| written(c.status)),
        reason: ready.and_then(|c| filled(Some(&c.reason))),
        message: ready.and_then(|c| filled(Some(&c.message))),
        problem: problem(build, now),
        keel_webhooks: spec
            .notify
            .iter()
            .filter(|t| match t.kind {
                NotifyType::Keel => true,
            })
            .map(|t| t.url.clone())
            .collect(),
        rebuild_requested: build
            .metadata
            .annotations
            .as_ref()
            .and_then(|a| a.get(annotations::REBUILD).cloned()),
    }
}

/// The `BuildRun`s, newest first. An object that is not a `BuildRun` of the Zeabur Builder
/// release Zeabur is built against is left out.
#[must_use]
pub fn runs(objects: &[DynamicObject]) -> Vec<ZeaburBuilderRun> {
    let mut out: Vec<ZeaburBuilderRun> = objects
        .iter()
        .filter_map(|object| object.clone().try_parse::<BuildRun>().ok())
        .filter_map(|run| {
            let BuildRun {
                metadata,
                spec,
                status,
            } = run;
            let status = status.unwrap_or_default();
            Some(ZeaburBuilderRun {
                name: metadata.name?,
                build: filled(Some(&spec.build_ref))?,
                revision: filled(Some(&spec.revision)),
                git_ref: filled(spec.git_ref.as_ref()),
                trigger: spec.trigger.and_then(written),
                phase: status.phase.and_then(written),
                reason: status.reason.and_then(written),
                message: filled(status.message.as_ref()),
                created_at: metadata.creation_timestamp.map(|t| t.0),
                started_at: status.start_time,
                finished_at: status.completion_time,
                tags: status.tags,
                digest: filled(status.image_digest.as_ref()),
            })
        })
        .collect();
    out.sort_by(|a, b| (b.created_at, &b.name).cmp(&(a.created_at, &a.name)));
    out
}

/// The reason a service shows when the build of its image failed.
pub const BUILD_FAILED: &str = "BuildFailed";

/// Gives a service that runs a `Build`'s image the build's status when it matters.
///
/// A container runs a `Build`'s image when the repositories match the `Build`'s own
/// (`spec.output.image`). The build has failed when the newest run failed, or nothing runs and the
/// `Build` is not ready or has a lasting [`ZeaburBuilderProblem`]. A `Build` that waits for its
/// folder to be uploaded has not failed.
///
/// Before the first image the service is building, or has failed with [`BUILD_FAILED`], and its
/// diagnoses go, as the build causes them. After it, a failed build fails an otherwise well service
/// with [`BUILD_FAILED`] too.
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
            && !build.awaiting_upload
            && (build.ready.as_deref() == Some("False")
                || build.problem.as_ref().is_some_and(|p| p.lasting)));
    let build_failed = |word| zeabur_model::ServiceStatus {
        health: zeabur_model::Health::Fail,
        word,
        reason: Some(BUILD_FAILED.to_owned()),
        cause: Some(zeabur_model::StatusCause::BuildFailed),
    };
    if build.latest_image.is_some() {
        if failed && service.status.health != zeabur_model::Health::Fail {
            service.status = build_failed(zeabur_model::StatusWord::RolloutStuck);
        }
        return;
    }
    service.status = if failed {
        build_failed(zeabur_model::StatusWord::Failed)
    } else if build.awaiting_upload && !running {
        zeabur_model::ServiceStatus {
            health: zeabur_model::Health::Idle,
            word: zeabur_model::StatusWord::WaitingUpload,
            reason: None,
            cause: None,
        }
    } else {
        zeabur_model::ServiceStatus {
            health: zeabur_model::Health::Progress,
            word: zeabur_model::StatusWord::Building,
            reason: None,
            cause: None,
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
                "source": {"type": "git", "git": {"url": "https://example.com/acme/web"},
                    "contextDir": "apps/web", "dockerfile": "Dockerfile.prod"},
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
    fn only_the_secret_made_for_a_build_is_its_own() {
        let build = |name: &str, secret: &str| {
            object(serde_json::json!({
                "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
                "metadata": {"name": name, "namespace": "demo"},
                "spec": {"builderRef": "dockerfile",
                    "source": {"type": "git", "git": {"url": "ssh://git@example.com/acme/web",
                        "credentialsRef": {"name": secret}}}}
            }))
        };
        let found = builds(&[build("api", "shared-git"), build("web", "web-git")], None);
        let own = |b: &ZeaburBuilderBuild| b.credentials.as_ref().map(|c| c.own);
        assert_eq!(own(&found[0]), Some(false));
        assert_eq!(own(&found[1]), Some(true));
    }

    #[test]
    fn a_build_waiting_for_its_folder_has_not_failed() {
        let build = object(serde_json::json!({
            "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
            "metadata": {"name": "site", "namespace": "demo"},
            "spec": {"builderRef": "railpack", "source": {"type": "archive", "archive": {}},
                "output": {"image": "zot:5000/demo/site"}},
            "status": {"uploadToken": {"request": "u1", "secretName": "site-upload",
                    "expiresAt": "2026-10-23T08:15:00Z"},
                "conditions": [
                    {"type": "SourceReady", "status": "False", "reason": "WaitingForUpload",
                        "lastTransitionTime": "2026-10-23T08:00:00Z"},
                    {"type": "Ready", "status": "False", "reason": "NotBuilt",
                        "lastTransitionTime": "2026-10-23T08:00:00Z"}]}
        }));
        let found = builds(&[build], None);
        assert!(found[0].archive && found[0].awaiting_upload);
        assert_eq!(found[0].upload_request.as_deref(), Some("u1"));
        assert!(found[0].problem.is_none());
    }

    #[test]
    fn an_object_this_release_does_not_know_is_left_out() {
        let build = |name: &str, notify: &str| {
            object(serde_json::json!({
                "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
                "metadata": {"name": name},
                "spec": {"builderRef": "railpack",
                    "source": {"type": "git", "git": {"url": "https://example.com/acme/web"}},
                    "notify": [{"type": notify, "url": "https://example.com/hook"}]}
            }))
        };
        let found = builds(&[build("new", "slack"), build("web", "keel")], None);
        let names: Vec<&str> = found.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["web"]);
    }

    #[test]
    fn a_built_build_with_keel() {
        let build = object(serde_json::json!({
            "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
            "metadata": {"name": "web", "namespace": "demo",
                "annotations": {"builder.zeabur.com/rebuild": "2026-10-23T08:00:00Z"}},
            "spec": {"builderRef": "railpack",
                "source": {"type": "git", "git": {"url": "https://example.com/acme/web", "ref": "main"}},
                "notify": [{"type": "keel", "url": "http://keel:9300/v1/webhooks/native"}]},
            "status": {"latestImage": "zot:5000/demo/web:main-1-abc@sha256:00",
                "exposedPorts": ["3000/tcp", "53/udp"],
                "conditions": [{"type": "Ready", "status": "True", "reason": "Succeeded",
                    "lastTransitionTime": "2026-10-23T08:00:00Z"}]}
        }));
        let found = builds(&[build], None);
        assert_eq!(found[0].repository.as_deref(), Some("zot:5000/demo/web"));
        assert_eq!(found[0].ready.as_deref(), Some("True"));
        assert_eq!(
            found[0].exposed_ports.as_deref(),
            Some(&["3000/tcp".to_owned(), "53/udp".to_owned()][..])
        );
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
                "source": {"type": "git", "git": {"url": "https://example.com/acme/api", "ref": "main"}}}
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
                    "source": {"type": "git", "git": {"url": "https://example.com/acme/web", "ref": "HEAD"}}},
                "status": {"resolvedRef": "refs/heads/master", "conditions": [
                    {"type": "BuilderReady", "status": "True", "reason": "Found",
                        "lastTransitionTime": "2026-10-23T08:00:00Z"},
                    {"type": "SourceReady", "status": "False", "reason": reason,
                        "message": "no branch or tag `main`", "lastTransitionTime": since},
                    {"type": "Ready", "status": "Unknown", "reason": "NotBuilt",
                        "lastTransitionTime": "2026-10-23T08:00:00Z"}]}
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
            "spec": {"builderRef": "nope", "source": {"type": "git", "git": {"url": "https://example.com/a/b"}}},
            "status": {"conditions": [{"type": "BuilderReady", "status": "False",
                "reason": "BuilderNotFound", "message": "no ClusterBuilder `nope`",
                "lastTransitionTime": "2026-10-23T08:00:00Z"}]}
        }));
        let found = builds(&[missing], Some(now));
        let problem = found[0].problem.as_ref().unwrap();
        assert_eq!(problem.condition, "BuilderReady");
        assert!(problem.lasting);
        assert_eq!(found[0].git_ref.as_deref(), Some("HEAD"));
    }
}
