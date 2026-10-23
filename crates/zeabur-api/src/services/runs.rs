//! A CronJob's runs: the Jobs it controls, newest first, with their pods for the logs.
//!
//! The Jobs come from the project read the document already makes, so the list costs no other
//! request. The CronJob's history limits keep it short; the newest 50 are sent.

use axum::Json;
use axum::extract::Path;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_adapters::controller_of;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::batch::v1::Job;
use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use zeabur_model::WorkloadKind;

use crate::error::ApiError;
use crate::handlers::Person;
use crate::projects::not_found;

const LIMIT: usize = 50;

/// How a run stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum RunStatus {
    /// Its pods run, or wait to.
    Running,
    /// It completed (`Complete`).
    Succeeded,
    /// It failed (`Failed`).
    Failed,
}

/// One run of a CronJob.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskRun {
    /// The Job's name.
    pub name: String,
    /// How it stands.
    pub status: RunStatus,
    /// Someone started it by hand (`cronjob.kubernetes.io/instantiate: manual`).
    pub manual: bool,
    /// When it started, in RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started: Option<String>,
    /// When it ended, in RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished: Option<String>,
    /// Its pods, newest last, for their logs.
    pub pods: Vec<String>,
}

/// A CronJob's runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskRuns {
    /// `projects/<project>/services/cronjob/<name>/runs`.
    pub id: String,
    /// The newest runs first, at most 50.
    pub runs: Vec<TaskRun>,
}

/// A CronJob's runs, newest first.
#[utoipa::path(get, path = "/api/v1/projects/{project}/services/{kind}/{name}/runs", tag = "services",
    operation_id = "taskRuns",
    params(("project" = String, Path),
           ("kind" = String, Path, description = "cronjob; any case"),
           ("name" = String, Path)),
    responses((status = 200, body = TaskRuns), (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn runs(
    Person(cluster): Person,
    Path((project, kind, name)): Path<(String, String, String)>,
) -> Result<Json<TaskRuns>, ApiError> {
    if WorkloadKind::parse(&kind) != Some(WorkloadKind::CronJob) {
        return Err(not_found(&project, &kind, &name));
    }
    let read = crate::projects::read(cluster.as_ref(), &project).await?;
    if !read
        .snapshot
        .cron_jobs
        .iter()
        .any(|c| c.metadata.name.as_deref() == Some(name.as_str()))
    {
        return Err(not_found(&project, "CronJob", &name));
    }
    let mut jobs: Vec<&Job> = read
        .snapshot
        .jobs
        .iter()
        .filter(|j| controlled_by(&j.metadata, "CronJob", &name))
        .collect();
    jobs.sort_by(|a, b| {
        b.metadata
            .creation_timestamp
            .cmp(&a.metadata.creation_timestamp)
    });
    let runs = jobs
        .into_iter()
        .take(LIMIT)
        .map(|job| run(job, &read.snapshot.pods))
        .collect();
    Ok(Json(TaskRuns {
        id: format!("projects/{project}/services/cronjob/{name}/runs"),
        runs,
    }))
}

fn run(job: &Job, pods: &[zeabur_kube::k8s_openapi::api::core::v1::Pod]) -> TaskRun {
    let status = job.status.as_ref();
    let condition = |kind: &str| {
        status
            .and_then(|s| s.conditions.as_ref())
            .into_iter()
            .flatten()
            .any(|c| c.type_ == kind && c.status == "True")
    };
    let state = if condition("Failed") {
        RunStatus::Failed
    } else if condition("Complete") {
        RunStatus::Succeeded
    } else {
        RunStatus::Running
    };
    let name = job.metadata.name.clone().unwrap_or_default();
    let mut owned: Vec<_> = pods
        .iter()
        .filter(|p| controlled_by(&p.metadata, "Job", &name))
        .collect();
    owned.sort_by(|a, b| {
        a.metadata
            .creation_timestamp
            .cmp(&b.metadata.creation_timestamp)
    });
    TaskRun {
        name,
        status: state,
        manual: job
            .metadata
            .annotations
            .as_ref()
            .and_then(|a| a.get("cronjob.kubernetes.io/instantiate"))
            .is_some_and(|v| v == "manual"),
        started: status
            .and_then(|s| s.start_time.as_ref())
            .map(|t| t.0.to_string()),
        finished: status
            .and_then(|s| s.completion_time.as_ref())
            .map(|t| t.0.to_string()),
        pods: owned
            .into_iter()
            .filter_map(|p| p.metadata.name.clone())
            .collect(),
    }
}

/// Owners are matched by kind and name: names are unique in the namespace at any one time.
fn controlled_by(meta: &ObjectMeta, kind: &str, name: &str) -> bool {
    controller_of(meta).is_some_and(|o| o.kind == kind && o.name == name)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_run_reads_its_condition_and_pods() {
        let job: Job = serde_json::from_value(json!({
            "metadata": {"name": "nightly-1",
                "annotations": {"cronjob.kubernetes.io/instantiate": "manual"}},
            "status": {"startTime": "2026-10-23T03:00:00Z",
                "conditions": [{"type": "Failed", "status": "True"}]}
        }))
        .unwrap();
        let pod = |name: &str, owner: &str| {
            serde_json::from_value(json!({"metadata": {"name": name, "ownerReferences": [
                {"apiVersion": "batch/v1", "kind": "Job", "name": owner, "uid": "u", "controller": true}
            ]}}))
            .unwrap()
        };
        let run = run(
            &job,
            &[pod("nightly-1-a", "nightly-1"), pod("other", "nightly-2")],
        );
        assert_eq!(run.status, RunStatus::Failed);
        assert!(run.manual);
        assert_eq!(run.pods, vec!["nightly-1-a"]);
    }
}
