//! A service's health and the word for it, and each pod's health.
//!
//! This is the only place the rule lives: the project document carries each service's
//! [`ServiceStatus`] and each pod's health, and the project list counts the same values, so the
//! web interface and every count agree.
//!
//! 1. A pod that keeps failing makes the service fail: phase `Failed`; or not ready and not
//!    succeeded, with a waiting or termination reason that means the container keeps failing
//!    (`CrashLoopBackOff`, `ImagePullBackOff`, `OOMKilled`, … as `kubectl get pods` shows them);
//!    or, with no reason given, five restarts or more; or, ready or not, a container that has
//!    restarted three times or more and whose last run lasted less than ten minutes and ended with
//!    a non-zero exit code less than ten minutes ago (see [`CRASH_WINDOW`]). While a Deployment's
//!    update is under way or stuck and some replica is ready, the word is `RolloutStuck`: the
//!    release before still serves, but the failing pod does not recover on its own.
//! 2. Otherwise the workload's [`Phase`] decides: `Failed` and `Degraded` (no replica ready) fail,
//!    `Progressing` progresses (a Deployment past its progress deadline with a ready replica warns,
//!    as `RolloutStuck`), and `Paused`, `Suspended`, `Completed` and `Unknown` are idle.
//! 3. A `Running` workload is idle when it is a CronJob or scaled to zero, fails when no replica is
//!    ready, warns when fewer replicas are ready than wanted, and is ok otherwise.
//!
//! A pod fails by rule 1, is idle once it succeeded, ok when ready, and warns otherwise.

use k8s_openapi::jiff::{SignedDuration, Timestamp};
use zeabur_model::{
    ContainerStatusSummary, Health, Phase, PodSummary, ServiceStatus, StatusWord, WorkloadKind,
    WorkloadSummary,
};

/// Waiting and termination reasons that mean a container keeps failing, as the kubelet reports
/// them in container statuses.
const FAILING_REASONS: [&str; 9] = [
    "CrashLoopBackOff",
    "ImagePullBackOff",
    "ErrImagePull",
    "InvalidImageName",
    "CreateContainerConfigError",
    "CreateContainerError",
    "RunContainerError",
    "OOMKilled",
    "Error",
];

/// How long after a crash a container that runs again still counts as failing.
///
/// A crash-looping container is ready for a while between runs: the kubelet restarts it with an
/// exponential back-off and resets the back-off only "if a container has executed for 10
/// minutes without any problems" [k8s-container-restarts]. A pod that only reads ready in
/// those moments would make its service flip between failing and healthy, so a crash counts
/// for the same ten minutes.
///
/// The restart count never goes down, and the pod's status keeps only the last run, so that run
/// must itself have been short: a container that ran for longer than the window before it
/// crashed had its back-off reset, and one crash after a long run is an incident, not a loop,
/// however many restarts it gathered before.
///
/// [k8s-container-restarts]: https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#container-restarts
const CRASH_WINDOW: SignedDuration = SignedDuration::from_mins(10);

/// Restarts from which a recent crash makes a ready container fail: one crash is an incident,
/// three are a loop.
const CRASH_RESTARTS: i32 = 3;

/// Whether `pod` keeps failing (rule 1). `now` is when the pod was read; without it a ready pod
/// never fails.
#[must_use]
pub fn failing(pod: &PodSummary, now: Option<Timestamp>) -> bool {
    if pod.phase == "Failed" {
        return true;
    }
    if pod.phase == "Succeeded" {
        return false;
    }
    if pod
        .container_statuses
        .iter()
        .any(|status| crashed_recently(status, now))
    {
        return true;
    }
    if pod.ready {
        return false;
    }
    pod.reason.as_deref().map_or(pod.restarts >= 5, |reason| {
        FAILING_REASONS.contains(&reason)
    })
}

/// Whether `status` restarted [`CRASH_RESTARTS`] times or more and its last run, shorter than
/// [`CRASH_WINDOW`], ended with a non-zero exit code less than [`CRASH_WINDOW`] before `now`. A
/// run with no start time never got going, so it was short.
fn crashed_recently(status: &ContainerStatusSummary, now: Option<Timestamp>) -> bool {
    let Some(last) = &status.last_termination else {
        return false;
    };
    let parse = |time: &Option<String>| time.as_deref().and_then(|t| t.parse::<Timestamp>().ok());
    let (started, finished) = (parse(&last.started), parse(&last.finished));
    match (now, finished) {
        (Some(now), Some(finished)) => {
            status.restarts >= CRASH_RESTARTS
                && last.exit_code != 0
                && now.duration_since(finished) < CRASH_WINDOW
                && started.is_none_or(|started| finished.duration_since(started) < CRASH_WINDOW)
        }
        _ => false,
    }
}

/// The health of one pod, read at `now` (see [`failing`]).
#[must_use]
pub fn pod_health(pod: &PodSummary, now: Option<Timestamp>) -> Health {
    if pod.phase == "Succeeded" {
        Health::Idle
    } else if failing(pod, now) {
        Health::Fail
    } else if pod.ready {
        Health::Ok
    } else {
        Health::Warn
    }
}

/// The status of a service made of `workload` and its `pods`, read at `now` (see [`failing`]).
#[must_use]
pub fn status(
    workload: &WorkloadSummary,
    pods: &[PodSummary],
    now: Option<Timestamp>,
) -> ServiceStatus {
    let of = |health, word| ServiceStatus {
        health,
        word,
        reason: None,
    };
    let serving = workload.kind == WorkloadKind::Deployment
        && matches!(workload.phase, Phase::Progressing | Phase::Failed)
        && workload.replicas.is_some_and(|r| r.ready > 0);
    if let Some(pod) = pods.iter().find(|pod| failing(pod, now)) {
        let word = if serving {
            StatusWord::RolloutStuck
        } else {
            StatusWord::Failing
        };
        return ServiceStatus {
            reason: pod.reason.clone().or_else(|| {
                pod.container_statuses
                    .iter()
                    .filter(|status| crashed_recently(status, now))
                    .find_map(|status| status.last_termination.as_ref()?.reason.clone())
            }),
            ..of(Health::Fail, word)
        };
    }
    match workload.phase {
        Phase::Failed if serving => of(Health::Warn, StatusWord::RolloutStuck),
        Phase::Failed => of(Health::Fail, StatusWord::Failed),
        Phase::Degraded => of(Health::Fail, StatusWord::Degraded),
        Phase::Progressing => of(Health::Progress, StatusWord::RollingOut),
        Phase::Paused | Phase::Suspended => of(Health::Idle, StatusWord::Paused),
        Phase::Completed => of(Health::Idle, StatusWord::Completed),
        Phase::Unknown => of(Health::Idle, StatusWord::Unknown),
        Phase::Running => match (workload.kind, workload.replicas) {
            (WorkloadKind::CronJob, _) => of(Health::Idle, StatusWord::Scheduled),
            (_, Some(replicas)) if replicas.desired == 0 => {
                of(Health::Idle, StatusWord::ScaledToZero)
            }
            (_, Some(replicas)) if replicas.ready <= 0 => of(Health::Fail, StatusWord::Degraded),
            (_, Some(replicas)) if replicas.ready < replicas.desired => {
                of(Health::Warn, StatusWord::Degraded)
            }
            (
                WorkloadKind::DaemonSet
                | WorkloadKind::Deployment
                | WorkloadKind::Job
                | WorkloadKind::StatefulSet,
                _,
            ) => of(Health::Ok, StatusWord::Ready),
        },
    }
}

#[cfg(test)]
mod tests {
    use zeabur_model::{Classification, ContainerTermination, Origin, Provenance, Replicas};

    use super::*;

    struct Service {
        workload: WorkloadSummary,
        pods: Vec<PodSummary>,
    }

    fn health(service: &Service) -> Health {
        status(&service.workload, &service.pods, None).health
    }

    fn service(kind: WorkloadKind, phase: Phase, replicas: Option<(i32, i32)>) -> Service {
        Service {
            workload: WorkloadSummary {
                kind,
                name: "y".to_owned(),
                namespace: "p".to_owned(),
                classification: Classification {
                    group: "default".to_owned(),
                    display_name: None,
                    managed_by: None,
                },
                phase,
                replicas: replicas.map(|(ready, desired)| Replicas { ready, desired }),
                image: None,
                created: None,
                provenance: Provenance {
                    tool: None,
                    managed_by: None,
                    warn_before_edit: false,
                },
                origin: Origin::object(kind.as_str(), "y"),
            },
            pods: Vec::new(),
        }
    }

    fn pod(phase: &str, ready: bool, restarts: i32, reason: Option<&str>) -> PodSummary {
        PodSummary {
            id: "projects/p/pods/y-1".to_owned(),
            name: "y-1".to_owned(),
            phase: phase.to_owned(),
            ready,
            restarts,
            health: Health::Warn,
            containers: vec!["y".to_owned()],
            reason: reason.map(str::to_owned),
            created: None,
            node: None,
            container_statuses: Vec::new(),
            origin: Origin::object("Pod", "y-1"),
        }
    }

    #[test]
    fn the_rule_gives_each_case_its_health() {
        use Health::{Fail, Idle, Ok, Progress, Warn};
        use WorkloadKind::{CronJob, Deployment};
        let running = |r| service(Deployment, Phase::Running, r);
        assert_eq!(health(&running(Some((2, 2)))), Ok);
        assert_eq!(health(&running(Some((1, 2)))), Warn);
        assert_eq!(health(&running(Some((0, 0)))), Idle);
        assert_eq!(health(&service(CronJob, Phase::Running, None)), Idle);
        assert_eq!(
            health(&service(Deployment, Phase::Progressing, None)),
            Progress
        );
        assert_eq!(health(&running(Some((0, 2)))), Fail);
        assert_eq!(health(&service(Deployment, Phase::Degraded, None)), Fail);
        assert_eq!(health(&service(Deployment, Phase::Failed, None)), Fail);
        assert_eq!(
            health(&service(Deployment, Phase::Failed, Some((0, 1)))),
            Fail
        );
        for phase in [
            Phase::Paused,
            Phase::Suspended,
            Phase::Completed,
            Phase::Unknown,
        ] {
            assert_eq!(health(&service(Deployment, phase, None)), Idle);
        }

        let mut crashing = running(Some((2, 2)));
        crashing.pods = vec![pod("Running", false, 3, Some("CrashLoopBackOff"))];
        assert_eq!(health(&crashing), Fail);
        // Not ready without a failing reason: only five restarts make it a failure.
        crashing.pods = vec![pod("Running", false, 4, None)];
        assert_eq!(health(&crashing), Ok);
        crashing.pods = vec![pod("Running", false, 5, None)];
        assert_eq!(health(&crashing), Fail);
        crashing.pods = vec![pod("Pending", false, 0, Some("ContainerCreating"))];
        assert_eq!(health(&crashing), Ok);
        crashing.pods = vec![pod("Succeeded", false, 9, Some("Error"))];
        assert_eq!(health(&crashing), Ok);
        crashing.pods = vec![pod("Failed", false, 0, None)];
        assert_eq!(health(&crashing), Fail);
    }

    #[test]
    fn words_and_reasons_follow_the_health() {
        let mut crashing = service(WorkloadKind::Deployment, Phase::Running, Some((1, 2)));
        assert_eq!(
            status(&crashing.workload, &crashing.pods, None).word,
            StatusWord::Degraded
        );
        crashing.pods = vec![pod("Running", false, 3, Some("CrashLoopBackOff"))];
        let failing = status(&crashing.workload, &crashing.pods, None);
        assert_eq!(failing.word, StatusWord::Failing);
        assert_eq!(failing.reason.as_deref(), Some("CrashLoopBackOff"));
        let cron = service(WorkloadKind::CronJob, Phase::Running, None);
        assert_eq!(
            status(&cron.workload, &cron.pods, None).word,
            StatusWord::Scheduled
        );
        let zero = service(WorkloadKind::Deployment, Phase::Running, Some((0, 0)));
        assert_eq!(
            status(&zero.workload, &zero.pods, None).word,
            StatusWord::ScaledToZero
        );
    }

    #[test]
    fn a_stuck_update_warns_while_the_release_before_serves_but_a_crash_loop_fails() {
        let stuck = service(WorkloadKind::Deployment, Phase::Failed, Some((1, 1)));
        let read = status(&stuck.workload, &stuck.pods, None);
        assert_eq!(read.health, Health::Warn);
        assert_eq!(read.word, StatusWord::RolloutStuck);

        let mut crashing = service(WorkloadKind::Deployment, Phase::Progressing, Some((1, 1)));
        crashing.pods = vec![pod("Running", false, 3, Some("CrashLoopBackOff"))];
        let read = status(&crashing.workload, &crashing.pods, None);
        assert_eq!(read.health, Health::Fail);
        assert_eq!(read.word, StatusWord::RolloutStuck);
        assert_eq!(read.reason.as_deref(), Some("CrashLoopBackOff"));

        let mut down = service(WorkloadKind::Deployment, Phase::Progressing, Some((0, 1)));
        down.pods = vec![pod("Running", false, 3, Some("CrashLoopBackOff"))];
        assert_eq!(
            status(&down.workload, &down.pods, None).word,
            StatusWord::Failing
        );
    }

    #[test]
    fn a_pod_is_idle_once_done_ok_when_ready_and_warns_otherwise() {
        assert_eq!(
            pod_health(&pod("Succeeded", false, 9, Some("Error")), None),
            Health::Idle
        );
        assert_eq!(
            pod_health(&pod("Running", false, 3, Some("CrashLoopBackOff")), None),
            Health::Fail
        );
        assert_eq!(pod_health(&pod("Running", true, 0, None), None), Health::Ok);
        assert_eq!(
            pod_health(&pod("Pending", false, 0, Some("ContainerCreating")), None),
            Health::Warn
        );
    }

    /// A ready pod whose container `y` restarted `restarts` times and last exited with
    /// `exit_code`, `ago` before the time of reading, after a run of 30 seconds.
    fn ready_after_crash(
        restarts: i32,
        exit_code: i32,
        ago: SignedDuration,
    ) -> (PodSummary, Timestamp) {
        ready_after_run(restarts, exit_code, ago, SignedDuration::from_secs(30))
    }

    /// As [`ready_after_crash`], after a run that lasted `ran`.
    fn ready_after_run(
        restarts: i32,
        exit_code: i32,
        ago: SignedDuration,
        ran: SignedDuration,
    ) -> (PodSummary, Timestamp) {
        let now: Timestamp = "2026-10-17T12:00:00Z".parse().unwrap();
        let mut ready = pod("Running", true, restarts, None);
        ready.container_statuses = vec![ContainerStatusSummary {
            name: "y".to_owned(),
            init: false,
            ready: true,
            restarts,
            waiting: None,
            terminated: None,
            last_termination: Some(ContainerTermination {
                reason: Some("Error".to_owned()),
                exit_code,
                started: Some(now.checked_sub(ago + ran).unwrap().to_string()),
                finished: Some(now.checked_sub(ago).unwrap().to_string()),
                message: None,
            }),
        }];
        (ready, now)
    }

    #[test]
    fn a_ready_pod_fails_while_its_last_crash_is_recent() {
        let fails = |restarts, exit_code, ago| {
            let (pod, now) = ready_after_crash(restarts, exit_code, ago);
            pod_health(&pod, Some(now)) == Health::Fail
        };
        let minutes = |m: i64, s: i64| SignedDuration::from_mins(m) + SignedDuration::from_secs(s);
        assert!(fails(3, 1, minutes(9, 59)));
        assert!(!fails(3, 1, minutes(10, 1)));
        assert!(!fails(2, 1, minutes(1, 0)));
        assert!(fails(5, 137, minutes(1, 0)));
        // A clean exit is not a crash.
        assert!(!fails(3, 0, minutes(1, 0)));
        // Restarts gathered long ago do not make one crash after a long run a loop.
        let after_run = |ran| {
            let (pod, now) = ready_after_run(3, 137, minutes(1, 0), ran);
            pod_health(&pod, Some(now)) == Health::Fail
        };
        assert!(after_run(minutes(9, 59)));
        assert!(!after_run(minutes(10, 1)));
        assert!(!after_run(SignedDuration::from_hours(24 * 7)));
        // Without the time of reading the kubelet's reasons alone decide.
        let (pod, _) = ready_after_crash(5, 1, minutes(1, 0));
        assert_eq!(pod_health(&pod, None), Health::Ok);

        let mut crashing = service(WorkloadKind::Deployment, Phase::Running, Some((1, 1)));
        let (pod, now) = ready_after_crash(5, 1, minutes(2, 0));
        crashing.pods = vec![pod];
        let failing = status(&crashing.workload, &crashing.pods, Some(now));
        assert_eq!(failing.health, Health::Fail);
        assert_eq!(failing.word, StatusWord::Failing);
        assert_eq!(failing.reason.as_deref(), Some("Error"));
    }
}
