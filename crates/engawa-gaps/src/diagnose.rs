//! Why a service's containers fail, from their status and events.
//!
//! Each diagnosis rests on what the kubelet reports, never on the waiting reason alone: on k3s
//! v1.36 a crash-looping container shows `waiting: CrashLoopBackOff` only briefly between runs
//! and `terminated: Error` most of the time, so a crash loop is read from the restart count and
//! how the previous run ended.
//!
//! **Recent.** Restart counts never go down, so a container that crashed last week and has run
//! since is not diagnosed. The kubelet resets a container's back-off once it "has executed for
//! 10 minutes without any problems" [k8s-restart-policy]; engawa uses the same window
//! ([`RECENT`]): a failure counts while the container is not ready, waits in
//! `CrashLoopBackOff`, or ended within the window.
//!
//! **Exit codes.** 126 and 127 are the shell's "not executable" and "command not found"
//! [bash-exit-status]; 128 + n is death by signal n, so 137 is `SIGKILL` and 143 `SIGTERM`.
//!
//! **Beyond the containers.** A service can fail with every container healthy, or before any
//! runs:
//!
//! - *Quota.* A `ResourceQuota` refuses the pods a controller creates; the apply that scaled the
//!   workload succeeded. The Deployment controller copies its ReplicaSet's `ReplicaFailure`
//!   condition onto the Deployment; the other controllers report `FailedCreate` events about the
//!   workload [k8s-quota]. Only a refusal by a quota ("exceeded quota: {name}") is `quotaBlocked`.
//! - *Scheduling.* A pod whose `PodScheduled` condition is `False` with reason `Unschedulable`
//!   waits for a node with room [k8s-pod-conditions]; the scheduler's message says why.
//! - *Configuration.* `CreateContainerConfigError` is how the kubelet reports a container whose
//!   Secret or ConfigMap, or a key in one, is missing; it does not start until it exists. A volume
//!   from a missing Secret or ConfigMap shows no such reason: the containers wait in
//!   `ContainerCreating`, and the pod's `FailedMount` event names what is missing.
//! - *Traffic* ([`traffic`]): something sends to it by name but no Service selects the pods, an
//!   `HTTPRoute` to it is not accepted, its Service has no ready pod, or a Service's `targetPort`
//!   names no port of its containers.
//!
//! **Never only a raw reason.** A failing service that none of these explain gets `unexplained`,
//! naming the reason the cluster gave, so the status pill's word is never the only text.
//!
//! Every sentence is a [`Message`] key that clients translate, with a `why`: the rule that
//! applies, with this occurrence's numbers; this module writes no prose.
//!
//! [k8s-quota]: https://kubernetes.io/docs/concepts/policy/resource-quotas/
//! [k8s-pod-conditions]: https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#pod-conditions
//!
//! [k8s-restart-policy]: https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#restart-policy
//! [bash-exit-status]: https://www.gnu.org/software/bash/manual/html_node/Exit-Status.html

use std::collections::{BTreeMap, BTreeSet};

use engawa_graph::{Snapshot, Workload, workloads};
use engawa_model::{
    ContainerStatusSummary, ContainerTermination, Diagnosis, DiagnosisKind, Health, Message,
    Origin, PodSummary, Service, WorkloadKind,
};
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::{Pod, PodSpec, Probe};
use k8s_openapi::jiff::{SignedDuration, Timestamp};
use serde_json::Value;

use crate::event::Event;

mod traffic;

/// How long after a failure it still counts, the kubelet's back-off reset window.
pub const RECENT: SignedDuration = SignedDuration::from_mins(10);

/// The longest output a diagnosis carries: a probe's, or a route controller's message.
const OUTPUT_LIMIT: usize = 200;

/// The diagnoses of `service`, sorted by kind then container.
///
/// `snapshot` is the namespace as read: its workloads' conditions and pod templates, its pods'
/// scheduling, and the time of the read ([`Snapshot::now`]), which decides what is recent;
/// without it every failure the status still shows counts. `events` are the project's events
/// and `http_routes` its `HTTPRoutes`, as JSON (any order; those about other objects are
/// ignored).
///
/// Each call reads the whole namespace again. To diagnose more than one service of a namespace,
/// read it once with [`Namespace::new`] and call [`Namespace::diagnose`] for each.
#[must_use]
pub fn diagnose(
    service: &Service,
    snapshot: &Snapshot,
    events: &[Event],
    http_routes: &[Value],
) -> Vec<Diagnosis> {
    Namespace::new(snapshot, events, http_routes).diagnose(service)
}

/// A namespace read once for the diagnoses of all its services.
///
/// It holds the workloads, pods and Jobs by name, the events by the object they are about, and
/// what sends to what. Each diagnosis looks up what it needs instead of going through the
/// namespace again, so diagnosing every service takes time in proportion to the namespace's
/// size, not to its square. Of several objects with one name the first counts, and what is
/// listed keeps the order it was given in, so the diagnoses are those of [`diagnose`].
#[derive(Debug)]
pub struct Namespace<'a> {
    snapshot: &'a Snapshot,
    /// The workloads by kind and name, the first of each in [`workloads`]' order.
    workloads: BTreeMap<WorkloadKind, BTreeMap<&'a str, Workload<'a>>>,
    /// The snapshot's pods by name, with their position in it.
    pods: BTreeMap<&'a str, Vec<(usize, &'a Pod)>>,
    /// The first Job of each name.
    jobs: BTreeMap<&'a str, &'a Job>,
    /// The events by the kind and name of the object they are about, in the order given.
    events: BTreeMap<&'a str, BTreeMap<&'a str, Vec<&'a Event>>>,
    traffic: traffic::Traffic<'a>,
}

impl<'a> Namespace<'a> {
    /// Reads `snapshot`, `events` and `http_routes` as [`diagnose`] takes them.
    #[must_use]
    pub fn new(snapshot: &'a Snapshot, events: &'a [Event], http_routes: &'a [Value]) -> Self {
        let sorted = workloads(snapshot);
        let mut by_name: BTreeMap<WorkloadKind, BTreeMap<&str, Workload<'_>>> = BTreeMap::new();
        for &workload in &sorted {
            by_name
                .entry(workload.kind())
                .or_default()
                .entry(workload.name())
                .or_insert(workload);
        }
        let mut pods: BTreeMap<&str, Vec<(usize, &Pod)>> = BTreeMap::new();
        for (position, pod) in snapshot.pods.iter().enumerate() {
            pods.entry(pod.metadata.name.as_deref().unwrap_or_default())
                .or_default()
                .push((position, pod));
        }
        let mut jobs = BTreeMap::new();
        for job in &snapshot.jobs {
            if let Some(name) = job.metadata.name.as_deref() {
                jobs.entry(name).or_insert(job);
            }
        }
        let mut by_object: BTreeMap<&str, BTreeMap<&str, Vec<&Event>>> = BTreeMap::new();
        for event in events {
            if let Some(regarding) = &event.regarding {
                by_object
                    .entry(regarding.kind.as_str())
                    .or_default()
                    .entry(regarding.name.as_str())
                    .or_default()
                    .push(event);
            }
        }
        Self {
            snapshot,
            traffic: traffic::Traffic::new(snapshot, &sorted, http_routes),
            workloads: by_name,
            pods,
            jobs,
            events: by_object,
        }
    }

    /// Says whether the namespace's Services could be listed; [`Namespace::new`] assumes they
    /// could.
    ///
    /// A refused list leaves `snapshot.services` empty because the read was refused, not because
    /// there are none, so with `false` no service is diagnosed as reached by name with no
    /// Service (`noService`). Every other diagnosis stays.
    #[must_use]
    pub const fn services_readable(mut self, readable: bool) -> Self {
        if !readable {
            self.traffic.services_unreadable();
        }
        self
    }

    /// The diagnoses of `service`, a service of this namespace, as [`diagnose`] gives them.
    #[must_use]
    pub fn diagnose(&self, service: &Service) -> Vec<Diagnosis> {
        let now = self.snapshot.now;
        let mut found = Found::default();
        let runs = matches!(
            service.workload.kind,
            WorkloadKind::Job | WorkloadKind::CronJob
        );
        let workload = self
            .workloads
            .get(&service.workload.kind)
            .and_then(|by_name| by_name.get(service.workload.name.as_str()))
            .copied();
        for pod in &service.pods {
            // What a `CreateContainerConfigError` in this pod already names as missing.
            let mut missing = BTreeSet::new();
            let pod_events = self.events_about("Pod", &pod.name);
            for status in &pod.container_statuses {
                let image = service
                    .containers
                    .iter()
                    .find(|c| c.name == status.name)
                    .and_then(|c| c.image.clone());
                image_pull(&mut found, pod, status, image.as_deref(), pod_events);
                missing.extend(config_missing(&mut found, pod, status));
                if runs {
                    run_failed(&mut found, pod, status, self.job_of(&pod.name));
                } else {
                    crash_loop(&mut found, service, pod, status, now);
                }
            }
            let spec = self
                .pod(&pod.name)
                .and_then(|p| p.spec.as_ref())
                .or_else(|| workload.and_then(Workload::pod_spec));
            probe_failures(&mut found, pod, spec, pod_events, now);
            volume_missing(&mut found, pod, pod_events, &missing, now);
        }
        unschedulable(&mut found, service, self.pods_of(service));
        if let Some(workload) = workload {
            let about = self.events_about(workload.kind().as_str(), workload.name());
            quota_blocked(&mut found, workload, about);
            traffic::unreachable(&mut found, service, &self.traffic, workload);
        }
        let mut diagnoses = found.finish(runs);
        if diagnoses.is_empty() && service.status.health == Health::Fail {
            diagnoses.push(unexplained(service));
        }
        diagnoses
    }

    /// The events about object `kind`/`name`, in the order given.
    fn events_about(&self, kind: &str, name: &str) -> &[&'a Event] {
        self.events
            .get(kind)
            .and_then(|by_name| by_name.get(name))
            .map_or(&[], Vec::as_slice)
    }

    /// The first pod called `name`; one without a name has none.
    fn pod(&self, name: &str) -> Option<&'a Pod> {
        self.pods
            .get(name)?
            .iter()
            .map(|&(_, pod)| pod)
            .find(|pod| pod.metadata.name.is_some())
    }

    /// The snapshot's pods that `service` lists, in the snapshot's order.
    fn pods_of(&self, service: &Service) -> Vec<&'a Pod> {
        let names: BTreeSet<&str> = service.pods.iter().map(|p| p.name.as_str()).collect();
        let mut pods: Vec<(usize, &Pod)> = names
            .into_iter()
            .filter_map(|name| self.pods.get(name))
            .flatten()
            .copied()
            .collect();
        pods.sort_by_key(|&(position, _)| position);
        pods.into_iter().map(|(_, pod)| pod).collect()
    }

    /// The Job that owns pod `name`: a Job's run, or a CronJob's.
    fn job_of(&self, name: &str) -> Option<&'a Job> {
        let owner = self
            .pod(name)?
            .metadata
            .owner_references
            .iter()
            .flatten()
            .find(|o| o.kind == "Job")?;
        self.jobs.get(owner.name.as_str()).copied()
    }
}

/// What one pod's container shows, gathered across pods by kind, container and sentence.
#[derive(Default)]
struct Found {
    by_key: BTreeMap<(DiagnosisKind, String, String), Gathered>,
    /// Runs of a Job or CronJob that succeeded, by container, for "a later run passed".
    succeeded: BTreeMap<String, Option<Timestamp>>,
}

struct Gathered {
    pods: BTreeSet<String>,
    cause: Message,
    why: Message,
    fix: Message,
    evidence: BTreeSet<Origin>,
    last: Option<Timestamp>,
}

impl Found {
    fn add(
        &mut self,
        kind: DiagnosisKind,
        container: &str,
        pod: &str,
        (cause, why, fix): (Message, Message, Message),
        evidence: Vec<Origin>,
        when: Option<Timestamp>,
    ) {
        let entry = self
            .by_key
            .entry((kind, container.to_owned(), cause.key.clone()))
            .or_insert_with(|| Gathered {
                pods: BTreeSet::new(),
                cause: cause.clone(),
                why: why.clone(),
                fix: fix.clone(),
                evidence: BTreeSet::new(),
                last: when,
            });
        if !pod.is_empty() {
            entry.pods.insert(pod.to_owned());
        }
        entry.evidence.extend(evidence);
        // The newest occurrence names the values (restarts, exit code, output).
        if when >= entry.last {
            entry.cause = cause;
            entry.why = why;
            entry.fix = fix;
            entry.last = when;
        }
    }

    fn finish(self, runs: bool) -> Vec<Diagnosis> {
        let succeeded = self.succeeded;
        self.by_key
            .into_iter()
            .filter(|((kind, container, _), gathered)| {
                // A Job or CronJob whose newest run of this container passed is not failing.
                !(runs
                    && *kind == DiagnosisKind::RunFailed
                    && succeeded
                        .get(container)
                        .is_some_and(|passed| *passed > gathered.last))
            })
            .map(|((kind, container, _), gathered)| Diagnosis {
                kind,
                container,
                pods: gathered.pods.into_iter().collect(),
                cause: gathered.cause,
                why: gathered.why,
                fix: gathered.fix,
                evidence: gathered.evidence.into_iter().collect(),
                last_seen: gathered.last.map(|t| t.to_string()),
            })
            .collect()
    }
}

fn parse(time: Option<&str>) -> Option<Timestamp> {
    time.and_then(|t| t.parse().ok())
}

fn recent(when: Option<Timestamp>, now: Option<Timestamp>) -> bool {
    match (when, now) {
        (Some(when), Some(now)) => now.duration_since(when) <= RECENT,
        _ => true,
    }
}

fn status_field(pod: &PodSummary, status: &ContainerStatusSummary, rest: &str) -> Origin {
    let list = if status.init {
        "initContainerStatuses"
    } else {
        "containerStatuses"
    };
    Origin::field(
        "Pod",
        &pod.name,
        format!("status.{list}[name={}].{rest}", status.name),
    )
}

// ── Image pull ──────────────────────────────────────────────────────────────────────────────────

/// Why an image cannot be pulled, read from the kubelet's and the runtime's words. Registries
/// word their errors differently, so anything unrecognised is `unknown` rather than a guess.
fn pull_class(reason: &str, text: &str) -> &'static str {
    let text = text.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| text.contains(n));
    if reason == "InvalidImageName" {
        "invalidName"
    } else if has(&["toomanyrequests", "429 too many", "rate limit"]) {
        "rateLimited"
    } else if has(&["does not exist or may require"]) {
        // Docker Hub answers the same for a missing and a private repository.
        "notFoundOrPrivate"
    } else if has(&["ghcr.io/"]) && has(&["anonymous token", "denied"]) {
        // GitHub's container registry refuses an anonymous token for a package that does not
        // exist and for a private one alike (k3s 1.36's containerd reports "failed to fetch
        // anonymous token: … 403 Forbidden" for `ghcr.io/<owner>/<missing>`); a private package
        // needs credentials [ghcr-auth]. With no credentials sent, that refusal says "missing or
        // private", not "your credentials were refused".
        //
        // [ghcr-auth]: https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry#authenticating-to-the-container-registry
        "notFoundOrPrivate"
    } else if has(&[
        "unauthorized",
        "authentication required",
        "401 unauthorized",
        "403 forbidden",
        "access denied",
        "denied:",
    ]) {
        "unauthorized"
    } else if has(&[
        "not found",
        "manifest unknown",
        "404 not found",
        "name unknown",
    ]) {
        "notFound"
    } else if has(&[
        "no such host",
        "i/o timeout",
        "connection refused",
        "dial tcp",
        "tls handshake timeout",
    ]) {
        "unreachable"
    } else {
        "unknown"
    }
}

fn image_pull(
    found: &mut Found,
    pod: &PodSummary,
    status: &ContainerStatusSummary,
    image: Option<&str>,
    events: &[&Event],
) {
    let Some(waiting) = &status.waiting else {
        return;
    };
    let reason = waiting.reason.as_deref().unwrap_or_default();
    if !matches!(
        reason,
        "ErrImagePull" | "ImagePullBackOff" | "InvalidImageName" | "ErrImageNeverPull"
    ) {
        return;
    }
    let pulls: Vec<&&Event> = events
        .iter()
        .filter(|e| {
            e.kind == "Warning"
                && e.reason == "Failed"
                && e.container().is_none_or(|c| c == status.name)
                && e.note.to_ascii_lowercase().contains("pull")
        })
        .collect();
    let text = pulls
        .iter()
        .map(|e| e.note.as_str())
        .chain(waiting.message.as_deref())
        .collect::<Vec<_>>()
        .join("\n");
    let class = pull_class(reason, &text);
    let mut cause =
        Message::new(&format!("diagnosis.imagePull.cause.{class}")).with("container", &status.name);
    if let Some(image) = image {
        cause = cause.with("image", image);
    }
    let fix =
        Message::new(&format!("diagnosis.imagePull.fix.{class}")).with("container", &status.name);
    let why = Message::new("diagnosis.imagePull.why").with("container", &status.name);
    let mut evidence = vec![status_field(pod, status, "state.waiting")];
    evidence.extend(pulls.iter().map(|e| e.origin.clone()));
    let when = pulls.iter().filter_map(|e| e.last_seen).max();
    found.add(
        DiagnosisKind::ImagePull,
        &status.name,
        &pod.name,
        (cause, why, fix),
        evidence,
        when,
    );
}

// ── Crash loop and out of memory ────────────────────────────────────────────────────────────────

/// The sentence for a run that ended with `code`, by what the code means.
const fn exit_variant(code: i32) -> &'static str {
    match code {
        0 => "exitsZero",
        126 => "notExecutable",
        127 => "commandNotFound",
        137 => "killed",
        143 => "terminated",
        _ => "exitCode",
    }
}

fn exit_fix(variant: &str) -> &'static str {
    match variant {
        "exitsZero" => "diagnosis.crashLoop.fix.exitsZero",
        "notExecutable" | "commandNotFound" => "diagnosis.crashLoop.fix.command",
        "killed" => "diagnosis.crashLoop.fix.killed",
        "terminated" => "diagnosis.crashLoop.fix.terminated",
        _ => "diagnosis.crashLoop.fix.logs",
    }
}

fn is_oom(termination: &ContainerTermination) -> bool {
    termination.reason.as_deref() == Some("OOMKilled")
}

fn memory_limit<'a>(service: &'a Service, container: &str) -> Option<&'a str> {
    service
        .containers
        .iter()
        .find(|c| c.name == container)
        .and_then(|c| c.resources.as_ref())
        .and_then(|r| r.limits.memory.as_deref())
}

fn out_of_memory(
    found: &mut Found,
    limit: Option<&str>,
    pod: &PodSummary,
    status: &ContainerStatusSummary,
    field: &str,
    termination: &ContainerTermination,
) {
    // Without a limit the node itself ran out of memory; the sentences differ.
    let variant = if limit.is_some() { "limit" } else { "noLimit" };
    let sentence = |part: &str| {
        let message = Message::new(&format!("diagnosis.outOfMemory.{part}.{variant}"))
            .with("container", &status.name);
        match limit {
            Some(limit) => message.with("limit", limit),
            None => message,
        }
    };
    let (cause, why, fix) = (sentence("cause"), sentence("why"), sentence("fix"));
    found.add(
        DiagnosisKind::OutOfMemory,
        &status.name,
        &pod.name,
        (
            cause,
            why,
            fix.with("restarts", status.restarts.to_string()),
        ),
        vec![
            status_field(pod, status, &format!("{field}.reason"))
                .with_value(termination.reason.clone().unwrap_or_default()),
        ],
        parse(termination.finished.as_deref()),
    );
}

/// A long-running container that keeps exiting: restarted at least once, and its previous run
/// ended recently, or it is not ready, or it waits in `CrashLoopBackOff`.
fn crash_loop(
    found: &mut Found,
    service: &Service,
    pod: &PodSummary,
    status: &ContainerStatusSummary,
    now: Option<Timestamp>,
) {
    if let Some(terminated) = &status.terminated
        && is_oom(terminated)
    {
        out_of_memory(
            found,
            memory_limit(service, &status.name),
            pod,
            status,
            "state.terminated",
            terminated,
        );
        return;
    }
    let Some(last) = &status.last_termination else {
        return;
    };
    let backing_off = status
        .waiting
        .as_ref()
        .is_some_and(|w| w.reason.as_deref() == Some("CrashLoopBackOff"));
    let when = parse(last.finished.as_deref());
    if status.restarts < 1 || !(backing_off || !status.ready || recent(when, now)) {
        return;
    }
    if is_oom(last) {
        out_of_memory(
            found,
            memory_limit(service, &status.name),
            pod,
            status,
            "lastState.terminated",
            last,
        );
        return;
    }
    let variant = exit_variant(last.exit_code);
    let mut cause = Message::new(&format!("diagnosis.crashLoop.cause.{variant}"))
        .with("container", &status.name)
        .with("restarts", status.restarts.to_string())
        .with("exitCode", last.exit_code.to_string());
    if let Some(reason) = &last.reason {
        cause = cause.with("reason", reason);
    }
    let fix = Message::new(exit_fix(variant)).with("container", &status.name);
    // A clean exit is restarted too, without the waits growing a reason; every other exit is
    // restarted with the kubelet's back-off [k8s-container-restarts].
    let why = if variant == "exitsZero" {
        Message::new("diagnosis.crashLoop.why.exitsZero")
    } else {
        Message::new("diagnosis.crashLoop.why.backOff")
    }
    .with("container", &status.name)
    .with("restarts", status.restarts.to_string());
    found.add(
        DiagnosisKind::CrashLoop,
        &status.name,
        &pod.name,
        (cause, why, fix),
        vec![
            status_field(pod, status, "lastState.terminated.exitCode")
                .with_value(last.exit_code.to_string()),
        ],
        when,
    );
}

/// Whether `job` stopped retrying: its `Failed` condition with reason `BackoffLimitExceeded`
/// [k8s-job-backoff].
///
/// [k8s-job-backoff]: https://kubernetes.io/docs/concepts/workloads/controllers/job/#pod-backoff-failure-policy
fn gave_up(job: &Job) -> bool {
    job.status
        .iter()
        .flat_map(|s| s.conditions.iter().flatten())
        .any(|c| {
            c.type_ == "Failed"
                && c.status == "True"
                && c.reason.as_deref() == Some("BackoffLimitExceeded")
        })
}

/// A Job's or CronJob's run that ended with a non-zero code. Runs are not restarted in place
/// (or only with `OnFailure`), so the current termination is read, then the previous one. When
/// the Job has given up, `why` says so with its `backoffLimit`, the retries it allows
/// (kube-apiserver fills in 6 when a Job sets none).
fn run_failed(
    found: &mut Found,
    pod: &PodSummary,
    status: &ContainerStatusSummary,
    job: Option<&Job>,
) {
    let (field, termination) = match (&status.terminated, &status.last_termination) {
        (Some(t), _) => ("state.terminated", t),
        (None, Some(t)) => ("lastState.terminated", t),
        (None, None) => return,
    };
    let when = parse(termination.finished.as_deref());
    if termination.exit_code == 0 {
        let passed = found.succeeded.entry(status.name.clone()).or_insert(when);
        *passed = (*passed).max(when);
        return;
    }
    if is_oom(termination) {
        out_of_memory(found, None, pod, status, field, termination);
        return;
    }
    let variant = exit_variant(termination.exit_code);
    let mut cause = Message::new(&format!("diagnosis.runFailed.cause.{variant}"))
        .with("container", &status.name)
        .with("exitCode", termination.exit_code.to_string());
    if let Some(reason) = &termination.reason {
        cause = cause.with("reason", reason);
    }
    let fix = Message::new(exit_fix(variant)).with("container", &status.name);
    let gave_up = job.is_some_and(gave_up);
    let mut why = Message::new(if gave_up {
        "diagnosis.runFailed.why.gaveUp"
    } else {
        "diagnosis.runFailed.why.exitCode"
    })
    .with("container", &status.name)
    .with("exitCode", termination.exit_code.to_string());
    let mut evidence = vec![
        status_field(pod, status, &format!("{field}.exitCode"))
            .with_value(termination.exit_code.to_string()),
    ];
    if let Some(job) = job {
        let name = job.metadata.name.as_deref().unwrap_or_default();
        if let Some(limit) = job.spec.as_ref().and_then(|s| s.backoff_limit) {
            why = why.with("backoffLimit", limit.to_string());
            cause = cause.with("backoffLimit", limit.to_string());
            evidence.push(
                Origin::field("Job", name, "spec.backoffLimit".to_owned())
                    .with_value(limit.to_string()),
            );
        }
        if gave_up {
            evidence.push(
                Origin::field("Job", name, "status.conditions[type=Failed]".to_owned())
                    .with_value("BackoffLimitExceeded"),
            );
        }
    }
    found.add(
        DiagnosisKind::RunFailed,
        &status.name,
        &pod.name,
        (cause, why, fix),
        evidence,
        when,
    );
}

// ── Probes ──────────────────────────────────────────────────────────────────────────────────────

/// Which probe an `Unhealthy` note is about and what it printed. The kubelet writes
/// "Liveness probe failed: …" followed by the probe's output (or "errored" when the probe could
/// not run).
fn probe_note(note: &str) -> Option<(&'static str, &str)> {
    for (prefix, probe) in [
        ("Liveness probe", "liveness"),
        ("Readiness probe", "readiness"),
        ("Startup probe", "startup"),
    ] {
        if let Some(rest) = note.strip_prefix(prefix) {
            let output = rest.split_once(':').map_or("", |(_, output)| output).trim();
            return Some((probe, output));
        }
    }
    None
}

fn truncate(text: &str) -> String {
    match text.char_indices().nth(OUTPUT_LIMIT) {
        Some((end, _)) => format!("{}…", text.get(..end).unwrap_or(text)),
        None => text.to_owned(),
    }
}

/// How often a probe runs when `periodSeconds` is unset [k8s-probe].
///
/// [k8s-probe]: https://kubernetes.io/docs/reference/kubernetes-api/workload-resources/pod-v1/#lifecycle-1
const PROBE_PERIOD: i32 = 10;

/// How many failures in a row fail a probe when `failureThreshold` is unset [k8s-probe].
///
/// [k8s-probe]: https://kubernetes.io/docs/reference/kubernetes-api/workload-resources/pod-v1/#lifecycle-1
const PROBE_THRESHOLD: i32 = 3;

/// The `probe` (`liveness`, `readiness`, `startup`) of `container` in `spec`.
fn probe_of<'s>(spec: &'s PodSpec, container: &str, probe: &str) -> Option<&'s Probe> {
    let container = spec
        .containers
        .iter()
        .chain(spec.init_containers.iter().flatten())
        .find(|c| c.name == container)?;
    match probe {
        "liveness" => container.liveness_probe.as_ref(),
        "readiness" => container.readiness_probe.as_ref(),
        "startup" => container.startup_probe.as_ref(),
        _ => None,
    }
}

/// `why` with the probe's period and failure threshold, and whether each is the default
/// ([`PROBE_PERIOD`], [`PROBE_THRESHOLD`]); the kubelet acts after `failureThreshold`
/// failures in a row, one every `periodSeconds` [k8s-configure-probes].
///
/// [k8s-configure-probes]: https://kubernetes.io/docs/tasks/configure-pod-container/configure-liveness-readiness-startup-probes/#configure-probes
fn with_timing(why: Message, probe: &Probe) -> Message {
    let period = probe.period_seconds;
    let threshold = probe.failure_threshold;
    why.with("period", period.unwrap_or(PROBE_PERIOD).to_string())
        .with("periodDefault", period.is_none().to_string())
        .with(
            "threshold",
            threshold.unwrap_or(PROBE_THRESHOLD).to_string(),
        )
        .with("thresholdDefault", threshold.is_none().to_string())
}

fn probe_failures(
    found: &mut Found,
    pod: &PodSummary,
    spec: Option<&PodSpec>,
    events: &[&Event],
    now: Option<Timestamp>,
) {
    for event in events {
        if event.reason != "Unhealthy" || !recent(event.last_seen, now) {
            continue;
        }
        let Some((probe, output)) = probe_note(&event.note) else {
            continue;
        };
        let container = event
            .container()
            .map(str::to_owned)
            .or_else(|| (pod.containers.len() == 1).then(|| pod.containers.concat()))
            .unwrap_or_default();
        let cause = Message::new(&format!("diagnosis.probeFailure.cause.{probe}"))
            .with("container", &container)
            .with("count", event.count.to_string())
            .with("output", truncate(output));
        let fix = Message::new(&format!("diagnosis.probeFailure.fix.{probe}"))
            .with("container", &container);
        let why = Message::new(&format!("diagnosis.probeFailure.why.{probe}"))
            .with("container", &container);
        // Without the spec the sentence says only what the probe does, not when.
        let why = match spec.and_then(|spec| probe_of(spec, &container, probe)) {
            Some(read) => with_timing(why, read),
            None => why,
        };
        found.add(
            DiagnosisKind::ProbeFailure,
            &container,
            &pod.name,
            (cause, why, fix),
            vec![event.origin.clone()],
            event.last_seen,
        );
    }
}

// ── Configuration ───────────────────────────────────────────────────────────────────────────────

/// What a `CreateContainerConfigError` message names as missing: the kind (`Secret`,
/// `ConfigMap`), its name, and the key when the object exists without it. The kubelet writes
/// `secret "db" not found` and `couldn't find key PASSWORD in Secret shop/db`.
fn missing_config(message: &str) -> Option<(&'static str, &str, Option<&str>)> {
    if let Some(rest) = message.strip_prefix("couldn't find key ") {
        let (key, rest) = rest.split_once(" in ")?;
        let (kind, object) = rest.split_once(' ')?;
        let kind = match kind {
            "Secret" => "Secret",
            "ConfigMap" => "ConfigMap",
            _ => return None,
        };
        let name = object.rsplit_once('/').map_or(object, |(_, name)| name);
        return Some((kind, name.trim(), Some(key)));
    }
    let (kind, rest) = [
        ("Secret", "secret \""),
        ("Secret", "secrets \""),
        ("ConfigMap", "configmap \""),
        ("ConfigMap", "configmaps \""),
    ]
    .into_iter()
    .find_map(|(kind, prefix)| Some((kind, message.strip_prefix(prefix)?)))?;
    let (name, _) = rest.split_once('"')?;
    Some((kind, name, None))
}

/// A container waiting in `CreateContainerConfigError`; returns the object it names as
/// missing, kind and name.
fn config_missing(
    found: &mut Found,
    pod: &PodSummary,
    status: &ContainerStatusSummary,
) -> Option<(&'static str, String)> {
    let waiting = status.waiting.as_ref()?;
    if waiting.reason.as_deref() != Some("CreateContainerConfigError") {
        return None;
    }
    let missing = waiting.message.as_deref().and_then(missing_config);
    let variant = match missing {
        Some((_, _, Some(_))) => "key",
        Some((_, _, None)) => "object",
        None => "unknown",
    };
    let sentence = |part: &str| {
        let mut message = Message::new(&format!("diagnosis.configMissing.{part}.{variant}"))
            .with("container", &status.name);
        if let Some((kind, name, key)) = missing {
            message = message.with("object", kind).with("name", name);
            if let Some(key) = key {
                message = message.with("key", key);
            }
        }
        message
    };
    let why = Message::new("diagnosis.configMissing.why").with("container", &status.name);
    let mut evidence = vec![status_field(pod, status, "state.waiting")];
    if let Some((kind, name, _)) = missing {
        evidence.push(Origin::object(kind, name));
    }
    found.add(
        DiagnosisKind::ConfigMissing,
        &status.name,
        &pod.name,
        (sentence("cause"), why, sentence("fix")),
        evidence,
        None,
    );
    missing.map(|(kind, name, _)| (kind, name.to_owned()))
}

/// What a `FailedMount` note names: the volume, and the Secret or ConfigMap that does not
/// exist. The kubelet writes `MountVolume.SetUp failed for volume "v" : secret "db" not found`
/// [k8s-operation-generator]; the not-found part is the Secret or ConfigMap manager's, whose
/// resource is `secret` or `configmap` [k8s-configmap-manager], as [`missing_config`] reads it.
///
/// [k8s-operation-generator]: https://github.com/kubernetes/kubernetes/blob/master/pkg/volume/util/operationexecutor/operation_generator.go
/// [k8s-configmap-manager]: https://github.com/kubernetes/kubernetes/blob/master/pkg/kubelet/configmap/configmap_manager.go
fn missing_volume(note: &str) -> Option<(&str, &'static str, &str)> {
    let rest = note.strip_prefix("MountVolume.SetUp failed for volume \"")?;
    let (volume, rest) = rest.split_once('"')?;
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    match missing_config(rest)? {
        (kind, name, None) => Some((volume, kind, name)),
        (_, _, Some(_)) => None,
    }
}

/// A volume from a Secret or ConfigMap that does not exist: the pod's containers wait in
/// `ContainerCreating` and only a recent `FailedMount` event says why. An object a
/// `CreateContainerConfigError` in the same pod already names (`already`) is not said twice.
fn volume_missing(
    found: &mut Found,
    pod: &PodSummary,
    events: &[&Event],
    already: &BTreeSet<(&'static str, String)>,
    now: Option<Timestamp>,
) {
    for event in events {
        if event.reason != "FailedMount" || !recent(event.last_seen, now) {
            continue;
        }
        let Some((volume, kind, name)) = missing_volume(&event.note) else {
            continue;
        };
        if already.contains(&(kind, name.to_owned())) {
            continue;
        }
        // The event is about the pod; it names a container only when the pod has one.
        let container = if pod.containers.len() == 1 {
            pod.containers.concat()
        } else {
            String::new()
        };
        let sentence = |key: &str| {
            let message = Message::new(key)
                .with("volume", volume)
                .with("object", kind)
                .with("name", name);
            if container.is_empty() {
                message.with("pod", &pod.name)
            } else {
                message.with("container", &container)
            }
        };
        found.add(
            DiagnosisKind::ConfigMissing,
            &container,
            &pod.name,
            (
                sentence("diagnosis.configMissing.cause.volume"),
                sentence("diagnosis.configMissing.why.volume"),
                sentence("diagnosis.configMissing.fix.volume"),
            ),
            vec![event.origin.clone(), Origin::object(kind, name)],
            event.last_seen,
        );
    }
}

// ── Scheduling and quota ────────────────────────────────────────────────────────────────────────

/// What the scheduler says is short, from its message ("0/3 nodes are available: 3 Insufficient
/// cpu."): `cpu`, `memory`, `placement` (taints, affinity, selectors), or `other`.
fn shortage(message: &str) -> &'static str {
    if message.contains("Insufficient cpu") {
        "cpu"
    } else if message.contains("Insufficient memory") {
        "memory"
    } else if ["taint", "affinity", "selector"]
        .iter()
        .any(|word| message.contains(word))
    {
        "placement"
    } else {
        "other"
    }
}

/// The pods of `service` that wait for a node; `pods` are the snapshot's pods it lists.
fn unschedulable(found: &mut Found, service: &Service, pods: Vec<&Pod>) {
    let name = &service.workload.name;
    for pod in pods {
        let pod_name = pod.metadata.name.as_deref().unwrap_or_default();
        let Some(condition) = pod
            .status
            .iter()
            .flat_map(|s| s.conditions.iter().flatten())
            .find(|c| {
                c.type_ == "PodScheduled"
                    && c.status == "False"
                    && c.reason.as_deref() == Some("Unschedulable")
            })
        else {
            continue;
        };
        let message = condition.message.as_deref().unwrap_or_default();
        let variant = shortage(message);
        let cause =
            Message::new(&format!("diagnosis.unschedulable.cause.{variant}")).with("service", name);
        let why = Message::new("diagnosis.unschedulable.why").with("message", message);
        let fix =
            Message::new(&format!("diagnosis.unschedulable.fix.{variant}")).with("service", name);
        found.add(
            DiagnosisKind::Unschedulable,
            "",
            pod_name,
            (cause, why, fix),
            vec![Origin::field(
                "Pod",
                pod_name,
                "status.conditions[type=PodScheduled]".to_owned(),
            )],
            condition.last_transition_time.as_ref().map(|t| t.0),
        );
    }
}

/// What the quota admission plugin writes before the quota's name [k8s-quota-admission].
///
/// [k8s-quota-admission]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/admission/plugin/resourcequota/controller.go
const QUOTA_EXCEEDED: &str = "exceeded quota: ";

/// The quota a refusal to create pods names.
fn quota_of(message: &str) -> Option<&str> {
    let (_, rest) = message.split_once(QUOTA_EXCEEDED)?;
    let name = rest.split(',').next()?.trim();
    (!name.is_empty()).then_some(name)
}

/// A quota that refuses the workload's pods; `events` are those about the workload.
fn quota_blocked(found: &mut Found, workload: Workload<'_>, events: &[&Event]) {
    let (kind, name) = (workload.kind().as_str(), workload.name());
    let mut add = |message: &str, evidence: Origin, when: Option<Timestamp>| {
        let Some(quota) = quota_of(message) else {
            return;
        };
        let cause = Message::new("diagnosis.quotaBlocked.cause")
            .with("service", name)
            .with("quota", quota);
        let why = Message::new("diagnosis.quotaBlocked.why").with("quota", quota);
        let fix = Message::new("diagnosis.quotaBlocked.fix")
            .with("service", name)
            .with("quota", quota);
        found.add(
            DiagnosisKind::QuotaBlocked,
            "",
            "",
            (cause, why, fix),
            vec![evidence, Origin::object("ResourceQuota", quota)],
            when,
        );
    };
    if let Workload::Deployment(deployment) = workload {
        for condition in deployment
            .status
            .iter()
            .flat_map(|s| s.conditions.iter().flatten())
            .filter(|c| c.type_ == "ReplicaFailure" && c.status == "True")
        {
            add(
                condition.message.as_deref().unwrap_or_default(),
                Origin::field(
                    kind,
                    name,
                    "status.conditions[type=ReplicaFailure]".to_owned(),
                ),
                condition.last_update_time.as_ref().map(|t| t.0),
            );
        }
    }
    for event in events.iter().filter(|e| e.reason == "FailedCreate") {
        add(&event.note, event.origin.clone(), event.last_seen);
    }
}

// ── The fallback ────────────────────────────────────────────────────────────────────────────────

/// A failing service nothing else explains, naming the reason the cluster gave.
fn unexplained(service: &Service) -> Diagnosis {
    let name = &service.workload.name;
    let reason = service.status.reason.as_deref();
    let with_reason = |message: Message| match reason {
        Some(reason) => message.with("reason", reason),
        None => message,
    };
    let failing: Vec<&PodSummary> = service
        .pods
        .iter()
        .filter(|pod| pod.health == Health::Fail)
        .collect();
    Diagnosis {
        kind: DiagnosisKind::Unexplained,
        container: String::new(),
        pods: failing.iter().map(|pod| pod.name.clone()).collect(),
        cause: with_reason(Message::new("diagnosis.unexplained.cause").with("service", name)),
        why: with_reason(Message::new("diagnosis.unexplained.why")),
        fix: Message::new("diagnosis.unexplained.fix").with("service", name),
        evidence: failing
            .iter()
            .map(|pod| Origin::field("Pod", &pod.name, "status".to_owned()))
            .collect(),
        last_seen: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pull_errors_are_classified_by_their_words() {
        let cases = [
            ("InvalidImageName", "", "invalidName"),
            (
                "ErrImagePull",
                "toomanyrequests: You have reached your pull rate limit",
                "rateLimited",
            ),
            (
                "ErrImagePull",
                "pull access denied for x, repository does not exist or may require 'docker login'",
                "notFoundOrPrivate",
            ),
            (
                "ErrImagePull",
                "failed to authorize: 401 Unauthorized",
                "unauthorized",
            ),
            (
                // As k3s v1.36.5 reports a missing ghcr.io image.
                "ImagePullBackOff",
                "Back-off pulling image \"ghcr.io/example/blog:2.4.1\": ErrImagePull: failed to pull and unpack image \"ghcr.io/example/blog:2.4.1\": failed to resolve reference \"ghcr.io/example/blog:2.4.1\": failed to authorize: failed to fetch anonymous token: unexpected status from GET request to https://ghcr.io/token?scope=repository%3Aexample%2Fblog%3Apull&service=ghcr.io: 403 Forbidden",
                "notFoundOrPrivate",
            ),
            (
                "ImagePullBackOff",
                "docker.io/library/nginx:nope: not found",
                "notFound",
            ),
            (
                "ErrImagePull",
                "dial tcp: lookup registry.example.test: no such host",
                "unreachable",
            ),
            ("ErrImagePull", "something new", "unknown"),
        ];
        for (reason, text, class) in cases {
            assert_eq!(pull_class(reason, text), class, "{text}");
        }
    }

    #[test]
    fn a_missing_config_is_read_from_the_kubelets_words() {
        assert_eq!(
            missing_config("secret \"db\" not found"),
            Some(("Secret", "db", None))
        );
        assert_eq!(
            missing_config("configmaps \"app\" not found"),
            Some(("ConfigMap", "app", None))
        );
        assert_eq!(
            missing_config("couldn't find key PASSWORD in Secret shop/db"),
            Some(("Secret", "db", Some("PASSWORD")))
        );
        assert_eq!(missing_config("something new"), None);
    }

    #[test]
    fn a_volume_from_a_missing_config_is_read_from_the_mount_failure() {
        // As k3s v1.36.5 reports a volume from a Secret that does not exist.
        assert_eq!(
            missing_volume(
                "MountVolume.SetUp failed for volume \"v\" : secret \"no-secret\" not found"
            ),
            Some(("v", "Secret", "no-secret"))
        );
        assert_eq!(
            missing_volume(
                "MountVolume.SetUp failed for volume \"settings\" : configmap \"app\" not found"
            ),
            Some(("settings", "ConfigMap", "app"))
        );
        assert_eq!(
            missing_volume(
                "MountVolume.SetUp failed for volume \"data\" : mount failed: exit status 32"
            ),
            None
        );
    }

    #[test]
    fn the_scheduler_message_says_what_is_short_and_a_quota_refusal_names_its_quota() {
        assert_eq!(
            shortage("0/3 nodes are available: 3 Insufficient memory."),
            "memory"
        );
        assert_eq!(
            shortage("0/1 nodes are available: 1 node(s) had untolerated taint {a: b}."),
            "placement"
        );
        assert_eq!(shortage("0/1 nodes are available."), "other");
        assert_eq!(
            quota_of("pods \"a\" is forbidden: exceeded quota: compute, requested: cpu=1"),
            Some("compute")
        );
        assert_eq!(
            quota_of("pods \"a\" is forbidden: violates PodSecurity"),
            None
        );
    }

    #[test]
    fn probe_notes_name_the_probe_and_its_output() {
        assert_eq!(
            probe_note("Readiness probe failed: HTTP probe failed with statuscode: 503"),
            Some(("readiness", "HTTP probe failed with statuscode: 503"))
        );
        assert_eq!(
            probe_note("Liveness probe errored: rpc error"),
            Some(("liveness", "rpc error"))
        );
        assert_eq!(probe_note("Back-off restarting failed container"), None);
    }

    #[test]
    fn long_output_is_cut_on_a_character_boundary() {
        let long = "é".repeat(OUTPUT_LIMIT + 5);
        let cut = truncate(&long);
        assert_eq!(cut.chars().count(), OUTPUT_LIMIT + 1);
        assert!(cut.ends_with('…'));
    }
}
