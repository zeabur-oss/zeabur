//! Scheduled tasks: a CronJob made from a service, and a run of one started by hand.
//!
//! - **A new task** is a CronJob whose one container is a copy of the service's: its image,
//!   variables, mounts and resources, with the person's command run by `/bin/sh -c`. The pod keeps
//!   the service's account, security context, pull Secrets and placement, and only the volumes the
//!   container mounts. Ports and probes are dropped. The pod labels are the task's own, so the
//!   service's Kubernetes Services do not select its pods.
//! - **A run now** is a Job made from the CronJob's `jobTemplate`, as `kubectl create job --from=
//!   cronjob/<name>` makes it [kubectl-from]: annotated `cronjob.kubernetes.io/instantiate: manual`
//!   and controlled by the CronJob, so its history limits also clean it up. It follows the
//!   CronJob's `concurrencyPolicy` [k8s-concurrency] against the unfinished Jobs the CronJob
//!   controls. The controller's `status.active` does not list manual runs.
//! - **An edit** sets the fields given on the CronJob; the command only when the one container runs
//!   `/bin/sh -c`.
//!
//! [kubectl-from]: https://github.com/kubernetes/kubectl/blob/master/pkg/cmd/create/create_job.go
//! [k8s-concurrency]: https://kubernetes.io/docs/concepts/workloads/controllers/cron-jobs/#concurrency-policy

use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::Container;
use serde_json::{Map, Value, json};
use zeabur_adapters::LABEL_TASK_OF;
use zeabur_model::{WorkloadKind, WorkloadRef};

use crate::TaskConcurrency;
use crate::change::{Target, Warning};
use crate::checks::ResourceRef;
use crate::fields::Step;
use crate::json::{add, remove, replace};
use crate::owned::SERVICE_LABEL;
use crate::plan::{ErrorKind, PlanError, Planner, Working, error, to_json};

/// The container fields a task keeps from the service's container.
const KEPT_CONTAINER: [&str; 9] = [
    "name",
    "image",
    "workingDir",
    "imagePullPolicy",
    "env",
    "envFrom",
    "volumeMounts",
    "resources",
    "securityContext",
];

/// The pod fields a task keeps from the service's pod; `volumes` is filtered apart.
const KEPT_POD: [&str; 6] = [
    "serviceAccountName",
    "securityContext",
    "imagePullSecrets",
    "nodeSelector",
    "affinity",
    "tolerations",
];

/// The longest CronJob name: its Jobs add 11 characters and must fit a 63-character label.
const NAME_MAX: usize = 52;

/// What a new scheduled task runs and when.
pub(crate) struct TaskRequest<'r> {
    pub(crate) from: &'r WorkloadRef,
    pub(crate) container: Option<&'r str>,
    pub(crate) name: &'r str,
    pub(crate) schedule: &'r str,
    pub(crate) time_zone: Option<&'r str>,
    pub(crate) command: &'r str,
    pub(crate) concurrency: TaskConcurrency,
    pub(crate) keep_succeeded: i32,
    pub(crate) keep_failed: i32,
}

/// What an edit of a scheduled task sets; `None` keeps the field.
pub(crate) struct TaskEdit<'r> {
    pub(crate) cron_job: &'r str,
    pub(crate) schedule: Option<&'r str>,
    pub(crate) time_zone: Option<&'r str>,
    pub(crate) command: Option<&'r str>,
    pub(crate) concurrency: Option<TaskConcurrency>,
    pub(crate) keep_succeeded: Option<i32>,
    pub(crate) keep_failed: Option<i32>,
    pub(crate) paused: Option<bool>,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl<'a> Planner<'a> {
    pub(crate) fn schedule_task(&mut self, task: &TaskRequest<'_>) -> Result<(), PlanError> {
        check_request(task)?;
        let found = self.workload(task.from)?;
        if matches!(found.kind(), WorkloadKind::Job | WorkloadKind::CronJob) {
            return Err(error(
                ErrorKind::Invalid,
                "not_a_service",
                format!(
                    "{} {} is already a task; make a task from a service that keeps running.",
                    task.from.kind.as_str(),
                    task.from.name
                ),
            ));
        }
        if self
            .input
            .snapshot
            .cron_jobs
            .iter()
            .any(|c| c.metadata.name.as_deref() == Some(task.name))
        {
            return Err(field_error(
                "task_exists",
                format!("A CronJob named {} already exists.", task.name),
                "name",
            ));
        }
        let container = Self::container(found, task.from, task.container)?;
        let spec = found
            .pod_spec()
            .map(to_json)
            .transpose()?
            .unwrap_or_else(|| json!({}));
        let pod = task_pod(&spec, &container.spec, task.command, &task.from.name)?;
        let mut labels = Map::new();
        labels.insert(SERVICE_LABEL.to_owned(), json!(task.name));
        labels.insert(LABEL_TASK_OF.to_owned(), json!(task.from.name));
        if let Some(group) = found
            .meta()
            .labels
            .as_ref()
            .and_then(|l| l.get(self.conventions.group_label.as_ref()))
        {
            labels.insert(self.conventions.group_label.to_string(), json!(group));
        }
        let mut cron_spec = json!({
            "schedule": task.schedule.trim(),
            "concurrencyPolicy": task.concurrency.as_kubernetes(),
            "successfulJobsHistoryLimit": task.keep_succeeded,
            "failedJobsHistoryLimit": task.keep_failed,
            "jobTemplate": {"spec": {"template": {
                "metadata": {"labels": {SERVICE_LABEL: task.name}},
                "spec": pod,
            }}},
        });
        if let (Some(zone), Some(map)) = (
            task.time_zone.filter(|z| !z.is_empty()),
            cron_spec.as_object_mut(),
        ) {
            map.insert("timeZone".to_owned(), json!(zone));
        }
        let body = json!({
            "apiVersion": "batch/v1",
            "kind": "CronJob",
            "metadata": {
                "name": task.name,
                "namespace": self.input.snapshot.namespace,
                "labels": labels,
            },
            "spec": cron_spec,
        });
        self.push_created(Target::of::<CronJob>(task.name), body);
        self.warnings.push(Warning::TaskImageCopied {
            from: task.from.clone(),
            image: container.spec.image.unwrap_or_default(),
        });
        Ok(())
    }

    fn cron_job(&self, name: &str) -> Result<&'a CronJob, PlanError> {
        self.input
            .snapshot
            .cron_jobs
            .iter()
            .find(|c| c.metadata.name.as_deref() == Some(name))
            .ok_or_else(|| {
                error(
                    ErrorKind::NotFound,
                    "unknown_workload",
                    format!("There is no CronJob named {name}."),
                )
            })
    }

    pub(crate) fn run_task(&mut self, name: &str) -> Result<(), PlanError> {
        let cron_job = self.cron_job(name)?;
        let Some(uid) = cron_job.metadata.uid.clone() else {
            return Err(error(
                ErrorKind::Invalid,
                "unreadable_object",
                format!("CronJob {name} has no uid."),
            ));
        };
        let policy = TaskConcurrency::from_kubernetes(
            cron_job
                .spec
                .as_ref()
                .and_then(|s| s.concurrency_policy.as_deref()),
        );
        let active: Vec<String> = self
            .input
            .snapshot
            .jobs
            .iter()
            .filter(|job| controlled_by(job, &uid) && !finished(job))
            .filter_map(|job| job.metadata.name.clone())
            .collect();
        match policy {
            TaskConcurrency::Allow => {}
            TaskConcurrency::Forbid => {
                if let Some(running) = active.first() {
                    return Err(error(
                        ErrorKind::Invalid,
                        "task_running",
                        format!(
                            "Job {running} of {name} still runs, and {name} skips a run while \
                             one runs. Wait for it to finish, or let runs overlap."
                        ),
                    ));
                }
            }
            TaskConcurrency::Replace => {
                for job in &active {
                    self.delete(&jobs(), job, None)?;
                }
            }
        }
        let template = cron_job
            .spec
            .as_ref()
            .map(|s| to_json(&s.job_template))
            .transpose()?
            .unwrap_or_else(|| json!({}));
        let mut annotations = template
            .pointer("/metadata/annotations")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        annotations.insert(
            "cronjob.kubernetes.io/instantiate".to_owned(),
            json!("manual"),
        );
        let job = manual_job_name(name, self.input.now);
        let mut metadata = json!({
            "name": job,
            "namespace": self.input.snapshot.namespace,
            "annotations": annotations,
            "ownerReferences": [{
                "apiVersion": "batch/v1",
                "kind": "CronJob",
                "name": name,
                "uid": uid,
                "controller": true,
                "blockOwnerDeletion": true,
            }],
        });
        if let (Some(labels), Some(map)) = (
            template.pointer("/metadata/labels"),
            metadata.as_object_mut(),
        ) {
            map.insert("labels".to_owned(), labels.clone());
        }
        let body = json!({
            "apiVersion": "batch/v1",
            "kind": "Job",
            "metadata": metadata,
            "spec": template.get("spec").cloned().unwrap_or_else(|| json!({})),
        });
        self.push_created(Target::of::<Job>(&job), body);
        self.impact.insert(WorkloadRef {
            kind: WorkloadKind::CronJob,
            name: name.to_owned(),
        });
        Ok(())
    }

    pub(crate) fn edit_task(&mut self, edit: &TaskEdit<'_>) -> Result<(), PlanError> {
        check_edit(edit)?;
        let name = edit.cron_job;
        let cron_job = self.cron_job(name)?;
        Self::refuse_managed(&cron_job.metadata, "CronJob", name)?;
        let container = match edit.command {
            Some(_) => Some(shell_container(cron_job, name)?),
            None => None,
        };
        let index = self.working(Target::of::<CronJob>(name), to_json(cron_job)?);
        let fields = [
            ("schedule", edit.schedule.map(|s| json!(s.trim()))),
            (
                "timeZone",
                edit.time_zone.map(|z| {
                    let zone = z.trim();
                    if zone.is_empty() {
                        Value::Null
                    } else {
                        json!(zone)
                    }
                }),
            ),
            (
                "concurrencyPolicy",
                edit.concurrency.map(|c| json!(c.as_kubernetes())),
            ),
            (
                "successfulJobsHistoryLimit",
                edit.keep_succeeded.map(|n| json!(n)),
            ),
            ("failedJobsHistoryLimit", edit.keep_failed.map(|n| json!(n))),
            ("suspend", edit.paused.map(|p| json!(p))),
        ];
        for (field, value) in fields {
            if let Some(value) = value {
                self.set_spec(index, field, value)?;
            }
        }
        if let (Some(command), Some(container)) = (edit.command, container) {
            let path = [
                Step::field("spec"),
                Step::field("jobTemplate"),
                Step::field("spec"),
                Step::field("template"),
                Step::field("spec"),
                Step::field("containers"),
                Step::named(&container),
                Step::field("command"),
            ];
            self.claim(index, &path, "command")?;
            self.patch(
                index,
                vec![replace(
                    "/spec/jobTemplate/spec/template/spec/containers/0/command",
                    json!(["/bin/sh", "-c", command.trim()]),
                )],
            )?;
        }
        self.impact.insert(WorkloadRef {
            kind: WorkloadKind::CronJob,
            name: name.to_owned(),
        });
        Ok(())
    }

    /// Sets, or with `null` removes, a field of the CronJob's `spec`.
    fn set_spec(&mut self, index: usize, field: &str, value: Value) -> Result<(), PlanError> {
        let path = [Step::field("spec"), Step::field(field)];
        self.claim(index, &path, &format!("spec.{field}"))?;
        let pointer = format!("/spec/{field}");
        let present = self.current(index).pointer(&pointer).is_some();
        let operation = match (present, value.is_null()) {
            (false, true) => return Ok(()),
            (true, true) => remove(&pointer),
            (true, false) => replace(&pointer, value),
            (false, false) => add(&pointer, value),
        };
        self.patch(index, vec![operation])
    }

    fn push_created(&mut self, target: Target, body: Value) {
        self.objects.push(Working {
            target,
            original: None,
            current: body,
            operations: Vec::new(),
            owners: Vec::new(),
            deleted: false,
        });
    }
}

/// The pod a task runs: the service's container with the command, and the volumes it mounts.
fn task_pod(
    spec: &Value,
    container: &Container,
    command: &str,
    from: &str,
) -> Result<Map<String, Value>, PlanError> {
    let mut kept = Map::new();
    let source = to_json(&container)?;
    for field in KEPT_CONTAINER {
        if let Some(value) = source.get(field) {
            kept.insert(field.to_owned(), value.clone());
        }
    }
    kept.insert("command".to_owned(), json!(["/bin/sh", "-c", command]));
    let mounted: Vec<&str> = container
        .volume_mounts
        .iter()
        .flatten()
        .map(|m| m.name.as_str())
        .collect();
    let volumes: Vec<Value> = spec
        .get("volumes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|v| {
            v.get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| mounted.contains(&n))
        })
        .cloned()
        .collect();
    if let Some(missing) = mounted.iter().find(|m| {
        !volumes
            .iter()
            .any(|v| v.get("name").and_then(Value::as_str) == Some(**m))
    }) {
        return Err(error(
            ErrorKind::Invalid,
            "volume_per_pod",
            format!("{from} gives each pod its own volume {missing}; a task cannot mount it."),
        ));
    }
    let mut pod = Map::new();
    for field in KEPT_POD {
        if let Some(value) = spec.get(field) {
            pod.insert(field.to_owned(), value.clone());
        }
    }
    pod.insert("containers".to_owned(), json!([kept]));
    pod.insert("restartPolicy".to_owned(), json!("Never"));
    if !volumes.is_empty() {
        pod.insert("volumes".to_owned(), json!(volumes));
    }
    Ok(pod)
}

/// The checks that need no cluster state.
fn check_request(task: &TaskRequest<'_>) -> Result<(), PlanError> {
    if !dns_label(task.name, NAME_MAX) {
        return Err(field_error(
            "invalid_name",
            format!(
                "{:?} is not a valid task name: use up to {NAME_MAX} lowercase letters, digits and hyphens.",
                task.name
            ),
            "name",
        ));
    }
    if task.schedule.trim().is_empty() {
        return Err(field_error(
            "missing_field",
            "A task needs a schedule.",
            "schedule",
        ));
    }
    if task.command.trim().is_empty() {
        return Err(field_error(
            "missing_field",
            "A task needs a command.",
            "command",
        ));
    }
    if !(0..=100).contains(&task.keep_succeeded) || !(0..=100).contains(&task.keep_failed) {
        return Err(error(
            ErrorKind::BadRequest,
            "invalid_history",
            "Keep between 0 and 100 finished runs.",
        ));
    }
    Ok(())
}

fn check_edit(edit: &TaskEdit<'_>) -> Result<(), PlanError> {
    if edit.schedule.is_some_and(|s| s.trim().is_empty()) {
        return Err(field_error(
            "missing_field",
            "A task needs a schedule.",
            "schedule",
        ));
    }
    if edit.command.is_some_and(|c| c.trim().is_empty()) {
        return Err(field_error(
            "missing_field",
            "A task needs a command.",
            "command",
        ));
    }
    let keeps = [edit.keep_succeeded, edit.keep_failed];
    if keeps.into_iter().flatten().any(|n| !(0..=100).contains(&n)) {
        return Err(error(
            ErrorKind::BadRequest,
            "invalid_history",
            "Keep between 0 and 100 finished runs.",
        ));
    }
    Ok(())
}

/// The name of the CronJob's only container, when it runs `/bin/sh -c <command>`.
fn shell_container(cron_job: &CronJob, name: &str) -> Result<String, PlanError> {
    let containers = cron_job
        .spec
        .as_ref()
        .and_then(|s| s.job_template.spec.as_ref())
        .and_then(|s| s.template.spec.as_ref())
        .map_or(&[][..], |s| s.containers.as_slice());
    match containers {
        [container]
            if container.args.as_ref().is_none_or(Vec::is_empty)
                && container.command.as_deref().is_some_and(
                    |c| matches!(c, [shell, flag, _] if shell == "/bin/sh" && flag == "-c"),
                ) =>
        {
            Ok(container.name.clone())
        }
        _ => Err(field_error(
            "not_a_shell_task",
            format!("CronJob {name} does not run one /bin/sh -c command; edit its YAML instead."),
            "command",
        )),
    }
}

fn jobs() -> ResourceRef {
    ResourceRef {
        group: "batch".to_owned(),
        version: "v1".to_owned(),
        resource: "jobs".to_owned(),
    }
}

fn controlled_by(job: &Job, uid: &str) -> bool {
    job.metadata
        .owner_references
        .iter()
        .flatten()
        .any(|o| o.controller == Some(true) && o.uid == uid)
}

/// A Job is finished once its `Complete` or `Failed` condition is true.
fn finished(job: &Job) -> bool {
    job.status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .is_some_and(|conditions| {
            conditions
                .iter()
                .any(|c| (c.type_ == "Complete" || c.type_ == "Failed") && c.status == "True")
        })
}

/// `<cron job>-manual-<MMDDhhmmss of now>`, kept within the 63 characters of a Job's
/// `job-name` label.
fn manual_job_name(cron_job: &str, now: &str) -> String {
    let digits: String = now.chars().filter(char::is_ascii_digit).collect();
    let stamp = digits.get(4..14).unwrap_or(&digits);
    let mut base = cron_job.to_owned();
    base.truncate(63 - "-manual-".len() - stamp.len());
    format!("{}-manual-{stamp}", base.trim_end_matches('-'))
}

fn dns_label(name: &str, max: usize) -> bool {
    !name.is_empty()
        && name.len() <= max
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

fn field_error(code: &str, detail: impl Into<String>, field: &str) -> PlanError {
    let mut refused = error(ErrorKind::BadRequest, code, detail);
    refused.field = Some(field.to_owned());
    refused
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manual_run_fits_a_label() {
        assert_eq!(
            manual_job_name("backup", "2026-10-23T10:15:30Z"),
            "backup-manual-1023101530"
        );
        let long = "a".repeat(52);
        assert!(manual_job_name(&long, "2026-10-23T10:15:30Z").len() <= 63);
    }
}
