//! Zeabur Builder in the API: `GET /projects/{p}/builds`, the project's `Build`s and their runs
//! (see `zeabur_modules::zeabur_builder`), read as the person.
//!
//! Which service a `Build` builds for is the page's to decide, by image repository; the answer
//! gives each run the pod whose logs show it, when the pod is still there (its Job is deleted
//! ten minutes after it finishes). It also names the workloads that ask Keel to update them
//! (`keel.sh/policy`), for the automatic-update switch.

use axum::extract::State;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::Pod;
use zeabur_kube::k8s_openapi::jiff::{SignedDuration, Timestamp};
use zeabur_kube::{Cluster, ClusterError, ClusterExt as _, ResourceRef};
use zeabur_model::Health;
use zeabur_modules::Reading;
use zeabur_modules::zeabur_builder::{
    self, BUILD_RUNS, BUILDS, RUN_LABEL, ZeaburBuilderBuild, ZeaburBuilderRun,
};

use crate::error::ApiError;
use crate::extract::{Json, Path};
use crate::handlers::{AppState, Person};
use crate::projects::backups::ProviderState;
use crate::views::UnreadableView;

/// A run and the pod that ran it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBuildRun {
    /// The run.
    #[serde(flatten)]
    pub run: ZeaburBuilderRun,
    /// Its pod, while it is there.
    pub pod: Option<String>,
    /// The pod's containers, in the order they run (`fetch`, `plan`, `build`).
    pub containers: Vec<String>,
    /// `ok` when it succeeded, `fail` when it failed, `progress` while it runs, else `idle`.
    pub health: Health,
}

/// A `Build`, and what its problem and its newest run say.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBuild {
    /// The `Build`.
    #[serde(flatten)]
    pub build: ZeaburBuilderBuild,
    /// The credentials the Git host refused, when it refused them and no run is under way.
    pub refused: Option<RefusedCredentials>,
    /// Its newest build ran out of memory: a higher build memory fixes it.
    pub out_of_memory: bool,
}

/// Which credentials a Git host refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum RefusedCredentials {
    /// A deploy key, over SSH.
    DeployKey,
    /// A token, over HTTPS.
    Token,
}

/// How long a request to build again may wait for its run.
const REBUILD_WINDOW: SignedDuration = SignedDuration::from_mins(2);

/// Whether `run` came to an end.
fn finished(run: &ZeaburBuilderRun) -> bool {
    matches!(
        run.phase.as_deref(),
        Some("Succeeded" | "Failed" | "Cancelled")
    )
}

fn run_health(run: &ZeaburBuilderRun) -> Health {
    match run.phase.as_deref() {
        Some("Succeeded") => Health::Ok,
        Some("Failed") => Health::Fail,
        Some("Pending" | "Planning" | "Building") => Health::Progress,
        _ => Health::Idle,
    }
}

/// Whether Git reaches `url` over SSH: `ssh://…` or the scp-like `user@host:path`.
fn over_ssh(url: &str) -> bool {
    if url.starts_with("ssh://") {
        return true;
    }
    let Some((user, rest)) = url.split_once('@') else {
        return false;
    };
    let user_ok = !user.is_empty()
        && user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    user_ok
        && rest
            .split_once(':')
            .is_some_and(|(host, _)| !host.is_empty() && !host.contains('/'))
}

/// `build` with what its problem and its newest run in `runs` (newest first) say.
fn judged(build: ZeaburBuilderBuild, runs: &[ProjectBuildRun]) -> ProjectBuild {
    let newest = runs.iter().map(|r| &r.run).find(|r| r.build == build.name);
    // A run under way may clear the problem, so it does not count until the run ends.
    let reason = build
        .problem
        .as_ref()
        .filter(|_| newest.is_none_or(finished))
        .and_then(|p| p.reason.as_deref());
    let refused = (reason == Some("AuthFailed") && build.credentials.is_some()).then(|| {
        if over_ssh(build.git_url.as_deref().unwrap_or_default()) {
            RefusedCredentials::DeployKey
        } else {
            RefusedCredentials::Token
        }
    });
    let out_of_memory = reason == Some("OutOfMemory")
        || newest.is_some_and(|r| {
            r.phase.as_deref() == Some("Failed") && r.reason.as_deref() == Some("OutOfMemory")
        });
    ProjectBuild {
        build,
        refused,
        out_of_memory,
    }
}

/// Whether a build is under way or about to be at `now`: a run not finished, a `Build` with no
/// image, run or problem yet, or a request to build again, at most [`REBUILD_WINDOW`] old, that no
/// run has answered yet.
fn building(builds: &[ProjectBuild], runs: &[ProjectBuildRun], now: Timestamp) -> bool {
    if runs.iter().any(|r| !finished(&r.run)) {
        return true;
    }
    builds.iter().map(|b| &b.build).any(|build| {
        let newest = runs.iter().map(|r| &r.run).find(|r| r.build == build.name);
        if build.latest_image.is_none() && build.problem.is_none() && newest.is_none() {
            return true;
        }
        let Some(asked) = build
            .rebuild_requested
            .as_deref()
            .and_then(|t| t.parse::<Timestamp>().ok())
        else {
            return false;
        };
        now.duration_since(asked) <= REBUILD_WINDOW
            && newest
                .and_then(|r| r.created_at)
                .is_none_or(|created| created < asked)
    })
}

/// A workload's Keel policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct KeelPolicy {
    /// `Deployment`, `StatefulSet` or `DaemonSet`.
    pub kind: String,
    /// Its name.
    pub name: String,
    /// The `keel.sh/policy` annotation: `force`, `major`, `minor`, `patch`, `all`, `glob:…`.
    pub policy: String,
}

/// A project's builds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBuilds {
    /// Whether Zeabur Builder could be read.
    pub state: ProviderState,
    /// The catalogue component that installs Zeabur Builder.
    pub component: Option<zeabur_catalog::ComponentRef>,
    /// The `Build`s, by name.
    pub builds: Vec<ProjectBuild>,
    /// Their runs, newest first.
    pub runs: Vec<ProjectBuildRun>,
    /// A build is under way or about to be, so a client polls sooner.
    pub building: bool,
    /// The workloads Keel updates; read only when there is a `Build`.
    pub keel: Vec<KeelPolicy>,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// The project's Zeabur Builder `Build`s and their runs, with the pod of each run, read as the
/// person. `state` is `not-installed` when the cluster does not serve `builder.zeabur.com`.
#[utoipa::path(get, path = "/api/v1/projects/{project}/builds", tag = "projects",
    operation_id = "getProjectBuilds",
    params(("project" = crate::rules::ProjectName, Path, description = "The project's namespace")),
    responses((status = 200, body = ProjectBuilds), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<ProjectBuilds>, ApiError> {
    read(
        cluster.as_ref(),
        &state.catalogue,
        &project,
        (state.clock)(),
    )
    .await
    .map(Json)
    .map_err(|e| ApiError::from(e.into_problem(&format!("builds of {project}"))))
}

/// Gives each service that runs a built image its build's status (see
/// [`zeabur_builder::built_image`]). Without Zeabur Builder, or when its objects cannot be read,
/// the services stay as they are.
pub(crate) async fn built_images(
    cluster: &dyn Cluster,
    project: &str,
    services: &mut [zeabur_model::Service],
    now: Timestamp,
) {
    if services.is_empty() {
        return;
    }
    let Ok(Reading::Objects(builds)) =
        crate::cluster::modules::read_in(cluster, &BUILDS, Some(project)).await
    else {
        return;
    };
    let builds = zeabur_builder::builds(&builds, Some(now));
    if builds.is_empty() {
        return;
    }
    let runs = match crate::cluster::modules::read_in(cluster, &BUILD_RUNS, Some(project)).await {
        Ok(Reading::Objects(runs)) => zeabur_builder::runs(&runs),
        _ => Vec::new(),
    };
    for service in services {
        zeabur_builder::built_image(service, &builds, &runs);
    }
}

async fn read(
    cluster: &dyn Cluster,
    catalogue: &[zeabur_catalog::Component],
    project: &str,
    now: Timestamp,
) -> Result<ProjectBuilds, ClusterError> {
    let mut unreadable = Vec::new();
    let component = || crate::cluster::modules::component(catalogue, &zeabur_builder::MODULE);
    let answer = |state| ProjectBuilds {
        state,
        component: component(),
        builds: Vec::new(),
        runs: Vec::new(),
        building: false,
        keel: Vec::new(),
        unreadable: Vec::new(),
    };
    let builds = match crate::cluster::modules::read_in(cluster, &BUILDS, Some(project)).await? {
        Reading::Objects(objects) => objects,
        Reading::NotInstalled => return Ok(answer(ProviderState::NotInstalled)),
        Reading::Refused(refusal) => {
            let mut out = answer(ProviderState::Refused);
            out.unreadable.push(refusal);
            return Ok(out);
        }
    };
    let runs = match crate::cluster::modules::read_in(cluster, &BUILD_RUNS, Some(project)).await? {
        Reading::Objects(objects) => objects,
        Reading::NotInstalled => Vec::new(),
        Reading::Refused(refusal) => {
            unreadable.push(refusal);
            Vec::new()
        }
    };
    let pods: Vec<Pod> = match cluster.list(Some(project)).await {
        Ok(pods) => pods,
        Err(error) => {
            let key = ResourceRef::of::<Pod>().key();
            unreadable.push(error.into_unreadable(&key)?.into());
            Vec::new()
        }
    };
    let runs = zeabur_builder::runs(&runs)
        .into_iter()
        .map(|run| {
            let pod = pods
                .iter()
                .filter(|p| {
                    p.metadata
                        .labels
                        .as_ref()
                        .and_then(|l| l.get(RUN_LABEL))
                        .is_some_and(|name| *name == run.name)
                })
                .max_by(|a, b| {
                    a.metadata
                        .creation_timestamp
                        .cmp(&b.metadata.creation_timestamp)
                });
            let containers = pod.and_then(|p| p.spec.as_ref()).map_or_default(|spec| {
                spec.init_containers
                    .iter()
                    .flatten()
                    .chain(&spec.containers)
                    .map(|c| c.name.clone())
                    .collect()
            });
            ProjectBuildRun {
                health: run_health(&run),
                run,
                pod: pod.and_then(|p| p.metadata.name.clone()),
                containers,
            }
        })
        .collect::<Vec<_>>();
    let keel = if builds.is_empty() {
        Vec::new()
    } else {
        keel_policies(cluster, project, &mut unreadable).await?
    };
    let builds: Vec<ProjectBuild> = zeabur_builder::builds(&builds, Some(now))
        .into_iter()
        .map(|build| judged(build, &runs))
        .collect();
    Ok(ProjectBuilds {
        state: ProviderState::Installed,
        component: component(),
        building: building(&builds, &runs, now),
        builds,
        runs,
        keel,
        unreadable,
    })
}

async fn keel_policies(
    cluster: &dyn Cluster,
    project: &str,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<Vec<KeelPolicy>, ClusterError> {
    let mut out = Vec::new();
    for (resource, kind) in [
        ("deployments", "Deployment"),
        ("statefulsets", "StatefulSet"),
        ("daemonsets", "DaemonSet"),
    ] {
        let reference = ResourceRef::new("apps", "v1", resource, kind);
        let listed = match cluster.list_metadata(&reference, Some(project)).await {
            Ok(listed) => listed.unwrap_or_default(),
            Err(error) => {
                unreadable.push(error.into_unreadable(&reference.key())?.into());
                continue;
            }
        };
        out.extend(listed.into_iter().filter_map(|meta| {
            let policy = meta.annotations?.remove("keel.sh/policy")?;
            Some(KeelPolicy {
                kind: kind.to_owned(),
                name: meta.name?,
                policy,
            })
        }));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn build(extra: &Value) -> ZeaburBuilderBuild {
        let mut value = json!({
            "name": "web", "archive": false, "awaitingUpload": false, "env": [],
            "suspended": false, "keelWebhooks": [],
        });
        if let (Some(fields), Some(extra)) = (value.as_object_mut(), extra.as_object()) {
            fields.extend(extra.clone());
        }
        serde_json::from_value(value).unwrap()
    }

    fn run(phase: Option<&str>, reason: Option<&str>, created: Option<&str>) -> ProjectBuildRun {
        let run: ZeaburBuilderRun = serde_json::from_value(json!({
            "name": "web-1", "build": "web", "phase": phase, "reason": reason,
            "createdAt": created, "tags": [],
        }))
        .unwrap();
        ProjectBuildRun {
            health: run_health(&run),
            run,
            pod: None,
            containers: Vec::new(),
        }
    }

    fn problem(reason: &str) -> Value {
        json!({"condition": "SourceReady", "reason": reason, "lasting": true})
    }

    #[test]
    fn a_run_says_its_health_by_its_phase() {
        assert_eq!(run(Some("Succeeded"), None, None).health, Health::Ok);
        assert_eq!(run(Some("Failed"), None, None).health, Health::Fail);
        assert_eq!(run(Some("Building"), None, None).health, Health::Progress);
        assert_eq!(run(Some("Cancelled"), None, None).health, Health::Idle);
    }

    #[test]
    fn refused_credentials_say_which_by_the_url() {
        let refused = |url: &str| {
            let b = build(&json!({
                "gitUrl": url, "credentials": {"name": "web-git", "own": true},
                "problem": problem("AuthFailed"),
            }));
            judged(b, &[]).refused
        };
        assert_eq!(
            refused("git@github.com:a/b.git"),
            Some(RefusedCredentials::DeployKey)
        );
        assert_eq!(
            refused("ssh://git@host/a/b"),
            Some(RefusedCredentials::DeployKey)
        );
        assert_eq!(
            refused("https://github.com/a/b"),
            Some(RefusedCredentials::Token)
        );
        let anonymous = build(&json!({"gitUrl": "git@h:a", "problem": problem("AuthFailed")}));
        assert_eq!(judged(anonymous, &[]).refused, None);
    }

    #[test]
    fn a_run_under_way_holds_back_the_problem() {
        let b = build(&json!({
            "gitUrl": "https://h/a", "credentials": {"name": "web-git", "own": true},
            "problem": problem("AuthFailed"),
        }));
        assert_eq!(
            judged(b, &[run(Some("Building"), None, None)]).refused,
            None
        );
    }

    #[test]
    fn out_of_memory_comes_from_the_problem_or_the_failed_run() {
        let failed = run(Some("Failed"), Some("OutOfMemory"), None);
        assert!(judged(build(&json!({})), &[failed]).out_of_memory);
        let other = run(Some("Failed"), Some("BuildFailed"), None);
        assert!(!judged(build(&json!({})), &[other]).out_of_memory);
        let problem = build(&json!({"problem": problem("OutOfMemory")}));
        assert!(judged(problem, &[]).out_of_memory);
    }

    #[test]
    fn building_covers_runs_new_builds_and_fresh_rebuild_requests() {
        let now: Timestamp = "2026-10-23T08:01:00Z".parse().unwrap();
        let built = |extra: &Value| vec![judged(build(extra), &[])];
        let done = run(Some("Succeeded"), None, Some("2026-10-23T07:00:00Z"));
        let image = json!({"latestImage": "r/web:1"});
        assert!(!building(&built(&image), std::slice::from_ref(&done), now));
        assert!(building(&built(&image), &[run(None, None, None)], now));
        assert!(building(&built(&json!({})), &[], now));
        let failing = json!({"problem": problem("RefNotFound")});
        assert!(!building(&built(&failing), &[], now));
        let asked = |at: &str| {
            let mut extra = image.clone();
            extra["rebuildRequested"] = json!(at);
            built(&extra)
        };
        let fresh = asked("2026-10-23T08:00:30Z");
        assert!(building(&fresh, std::slice::from_ref(&done), now));
        let stale = asked("2026-10-23T07:30:00Z");
        assert!(!building(&stale, std::slice::from_ref(&done), now));
        let answered = run(Some("Succeeded"), None, Some("2026-10-23T08:00:40Z"));
        assert!(!building(&fresh, &[answered], now));
    }
}
