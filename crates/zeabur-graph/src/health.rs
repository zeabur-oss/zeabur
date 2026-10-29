//! A service's health and the word for it, and each pod's health.
//!
//! This is the only place the rule lives: the project document carries each service's
//! [`ServiceStatus`] and each pod's health, and the project list counts the same values, so the
//! web interface and every count agree.
//!
//! 1. A pod that keeps failing makes the service fail: phase `Failed`; or not ready and not
//!    succeeded, with a waiting or termination reason that means the container keeps failing
//!    (`CrashLoopBackOff`, `ImagePullBackOff`, `OOMKilled`, … as `kubectl get pods` shows them);
//!    or, ready or not, a container the kubelet backs off restarting (see [`crash_looping`]). While
//!    a Deployment's update is under way or stuck and some replica is ready, the word is
//!    `RolloutStuck`: the release before still serves, but the failing pod does not recover on its
//!    own.
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

/// The kubelet resets a container's restart back-off after 600 s without a restart:
/// <https://github.com/kubernetes/kubernetes/blob/master/pkg/kubelet/kubelet.go> (`crashLoopBackOff.HasExpiredFunc`).
const BACKOFF_RESET: SignedDuration = SignedDuration::from_secs(600);

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
        .any(|status| crash_looping(status, now))
    {
        return true;
    }
    if pod.ready {
        return false;
    }
    pod.reason
        .as_deref()
        .is_some_and(|reason| FAILING_REASONS.contains(&reason))
}

/// Whether the kubelet backs off restarting `status`.
///
/// It waits in `CrashLoopBackOff`; or it restarted at least twice (so its previous run was itself a
/// restart), that run lasted less than ten minutes, and the current run is not running yet or
/// started less than ten minutes before `now`. The exit code and readiness do not count,
/// as they do not in the kubelet. Missing times count as short runs.
#[must_use]
pub fn crash_looping(status: &ContainerStatusSummary, now: Option<Timestamp>) -> bool {
    if status
        .waiting
        .as_ref()
        .is_some_and(|w| w.reason.as_deref() == Some("CrashLoopBackOff"))
    {
        return true;
    }
    let Some(last) = &status.last_termination else {
        return false;
    };
    let parse = |time: &Option<String>| time.as_deref().and_then(|t| t.parse::<Timestamp>().ok());
    let last_run_short = match (parse(&last.started), parse(&last.finished)) {
        (Some(started), Some(finished)) => finished.duration_since(started) < BACKOFF_RESET,
        _ => true,
    };
    let current_run_short = match (parse(&status.running_since), now) {
        (Some(since), Some(now)) => now.duration_since(since) < BACKOFF_RESET,
        (Some(_), None) => false,
        (None, _) => true,
    };
    status.restarts >= 2 && last_run_short && current_run_short
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
                    .filter(|status| crash_looping(status, now))
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
    use zeabur_model::{
        Classification, ContainerTermination, ContainerWaiting, Origin, Provenance, Replicas,
    };

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
                schedule: None,
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
        // Restarts alone, with no failing reason, do not make it a failure.
        crashing.pods = vec![pod("Running", false, 9, None)];
        assert_eq!(health(&crashing), Ok);
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

    /// A ready pod whose container `y` restarted `restarts` times; its previous run lasted
    /// `previous` and ended 30 seconds before the current run started, `running` before `now`.
    fn ready_after_run(
        restarts: i32,
        previous: SignedDuration,
        running: SignedDuration,
    ) -> (PodSummary, Timestamp) {
        let now: Timestamp = "2026-10-17T12:00:00Z".parse().unwrap();
        let since = now.checked_sub(running).unwrap();
        let finished = since.checked_sub(SignedDuration::from_secs(30)).unwrap();
        let mut ready = pod("Running", true, restarts, None);
        ready.container_statuses = vec![ContainerStatusSummary {
            name: "y".to_owned(),
            init: false,
            ready: true,
            restarts,
            waiting: None,
            running_since: Some(since.to_string()),
            terminated: None,
            last_termination: Some(ContainerTermination {
                reason: Some("Error".to_owned()),
                exit_code: 1,
                started: Some(finished.checked_sub(previous).unwrap().to_string()),
                finished: Some(finished.to_string()),
                message: None,
            }),
        }];
        (ready, now)
    }

    #[test]
    fn a_ready_pod_fails_while_the_kubelet_still_backs_off_its_container() {
        let secs = SignedDuration::from_secs;
        let fails = |restarts, previous, running| {
            let (pod, now) = ready_after_run(restarts, previous, running);
            pod_health(&pod, Some(now)) == Health::Fail
        };
        assert!(fails(4, secs(5), secs(35)));
        assert!(fails(2, secs(5), secs(599)));
        // The back-off resets after 600 seconds without a restart.
        assert!(!fails(4, secs(5), secs(601)));
        // The first restart is immediate, so it is no back-off.
        assert!(!fails(1, secs(5), secs(35)));
        // A previous run of 600 seconds or more reset the back-off, however many restarts before.
        assert!(!fails(7, secs(601), secs(35)));
        assert!(!fails(7, SignedDuration::from_hours(24 * 7), secs(35)));
        assert!(fails(7, secs(599), secs(35)));
        // Without the time of reading the kubelet's reasons alone decide.
        let (pod, _) = ready_after_run(5, secs(5), secs(35));
        assert_eq!(pod_health(&pod, None), Health::Ok);

        let mut crashing = service(WorkloadKind::Deployment, Phase::Running, Some((1, 1)));
        let (pod, now) = ready_after_run(5, secs(5), secs(35));
        crashing.pods = vec![pod];
        let failing = status(&crashing.workload, &crashing.pods, Some(now));
        assert_eq!(failing.health, Health::Fail);
        assert_eq!(failing.word, StatusWord::Failing);
        assert_eq!(failing.reason.as_deref(), Some("Error"));
    }

    #[test]
    fn a_container_waiting_in_back_off_is_a_crash_loop_whatever_it_exited_with() {
        let (mut pod, now) = ready_after_run(1, SignedDuration::from_secs(5), SignedDuration::ZERO);
        let status = &mut pod.container_statuses[0];
        status.running_since = None;
        status.last_termination.as_mut().unwrap().exit_code = 0;
        assert!(!crash_looping(status, Some(now)));
        status.waiting = Some(ContainerWaiting {
            reason: Some("CrashLoopBackOff".to_owned()),
            message: None,
        });
        assert!(crash_looping(status, Some(now)));
    }
}
