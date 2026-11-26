//! What a service's overview says first, from its status, its diagnoses and its pods.
//!
//! A service scaled to zero is stopped. A diagnosis, or a failing or warning status, is a problem,
//! named by the reason of the failing pod, else by the first diagnosis that has a word. A rollout
//! whose only trouble is pods that wait for a node stays in progress for
//! [`SCHEDULING_GRACE`]. A task or an idle service says its status word. Otherwise it deploys or
//! runs. Whether its domains are reached, and whether a rollout card shows, the client adds from
//! the trace and the rollout.

use std::collections::BTreeSet;

use zeabur_kube::k8s_openapi::jiff::{SignedDuration, Timestamp};
use zeabur_model::{
    Diagnosis, DiagnosisKind, Headline, Health, Service, ServiceVerdict, StatusCause, Trouble,
    WorkloadKind,
};

/// How long a pod may wait for a node before the wait is a problem.
pub(crate) const SCHEDULING_GRACE: SignedDuration = SignedDuration::from_mins(5);

/// The verdict on `service`, with its diagnoses, at `now`.
pub(crate) fn of(service: &Service, now: Timestamp) -> ServiceVerdict {
    let status = &service.status;
    let task = matches!(
        service.workload.kind,
        WorkloadKind::Job | WorkloadKind::CronJob
    );
    let verdict = |tone, headline| ServiceVerdict { tone, headline };
    if !task && service.workload.replicas.is_some_and(|r| r.desired == 0) {
        return verdict(Health::Idle, Headline::Stopped);
    }
    let causes = causes(&service.diagnoses);
    if !causes.is_empty() || matches!(status.health, Health::Fail | Health::Warn) {
        return verdict(
            problem_tone(service, &causes, now),
            Headline::Problem {
                trouble: trouble(service, &causes),
            },
        );
    }
    if task || status.health == Health::Idle {
        return verdict(status.health, Headline::Status { word: status.word });
    }
    if status.health == Health::Progress {
        return verdict(Health::Progress, Headline::Deploying);
    }
    verdict(Health::Ok, Headline::Running)
}

/// A diagnosis that only follows from another one: no ready replica, because none starts.
fn is_consequence(diagnosis: &Diagnosis) -> bool {
    diagnosis.kind == DiagnosisKind::Unreachable
        && diagnosis.cause.key.ends_with(".noReadyEndpoints")
}

/// The diagnoses that are causes; all of them when each is a consequence.
fn causes(diagnoses: &[Diagnosis]) -> Vec<&Diagnosis> {
    let causes: Vec<&Diagnosis> = diagnoses.iter().filter(|d| !is_consequence(d)).collect();
    if causes.is_empty() {
        diagnoses.iter().collect()
    } else {
        causes
    }
}

const fn kind_trouble(kind: DiagnosisKind) -> Option<Trouble> {
    let word = match kind {
        DiagnosisKind::CrashLoop => StatusCause::Crashing,
        DiagnosisKind::OutOfMemory => StatusCause::OutOfMemory,
        DiagnosisKind::ImagePull => StatusCause::ImagePull,
        DiagnosisKind::ConfigMissing => StatusCause::ConfigMissing,
        DiagnosisKind::Unschedulable => StatusCause::WaitingForRoom,
        DiagnosisKind::RunFailed => StatusCause::RunFailed,
        DiagnosisKind::PodSecurity => StatusCause::Security,
        DiagnosisKind::ProbeFailure => return Some(Trouble::HealthCheck),
        DiagnosisKind::QuotaBlocked => return Some(Trouble::Quota),
        DiagnosisKind::Unreachable | DiagnosisKind::Unexplained => return None,
    };
    Some(Trouble::Reason { word })
}

fn trouble(service: &Service, causes: &[&Diagnosis]) -> Trouble {
    // The kubelet reports a container that may not run as root as a config error.
    if causes.iter().any(|d| d.kind == DiagnosisKind::PodSecurity) {
        return Trouble::Reason {
            word: StatusCause::Security,
        };
    }
    if let Some(word) = service.status.cause {
        return Trouble::Reason { word };
    }
    causes
        .iter()
        .find_map(|d| kind_trouble(d.kind))
        .unwrap_or(Trouble::Status {
            word: service.status.word,
        })
}

/// Red unless the service still serves. A rollout whose only trouble is pods that wait for a
/// node is in progress until the oldest of them has waited [`SCHEDULING_GRACE`].
fn problem_tone(service: &Service, causes: &[&Diagnosis], now: Timestamp) -> Health {
    let health = service.status.health;
    if health != Health::Progress
        || !causes
            .iter()
            .all(|d| d.kind == DiagnosisKind::Unschedulable)
    {
        return if health == Health::Warn {
            Health::Warn
        } else {
            Health::Fail
        };
    }
    let waiting: BTreeSet<&str> = causes
        .iter()
        .flat_map(|d| d.pods.iter().map(String::as_str))
        .collect();
    let oldest = service
        .pods
        .iter()
        .filter(|pod| waiting.contains(pod.name.as_str()))
        .filter_map(|pod| pod.created_at)
        .min();
    match oldest {
        Some(created) if now.duration_since(created) < SCHEDULING_GRACE => Health::Progress,
        _ => Health::Fail,
    }
}

#[cfg(test)]
mod tests {
    use zeabur_model::{
        Classification, Fault, Message, Origin, Phase, PodSummary, Provenance, Replicas,
        ServiceRole, ServiceStatus, StatusWord, WorkloadSummary,
    };

    use super::*;

    fn service(kind: WorkloadKind, health: Health, desired: i32) -> Service {
        Service {
            id: "projects/p/services/deployment/web".to_owned(),
            workload: WorkloadSummary {
                kind,
                name: "web".to_owned(),
                namespace: "p".to_owned(),
                classification: Classification {
                    group: "default".to_owned(),
                    display_name: None,
                    managed_by: None,
                },
                phase: Phase::Running,
                replicas: Some(Replicas { ready: 0, desired }),
                image: None,
                created_at: None,
                schedule: None,
                provenance: Provenance {
                    tool: None,
                    managed_by: None,
                    warn_before_edit: false,
                },
                origin: Origin::object(kind.as_str(), "web"),
            },
            status: ServiceStatus {
                health,
                word: StatusWord::Ready,
                reason: None,
                cause: None,
            },
            pods: Vec::new(),
            containers: Vec::new(),
            endpoint_names: Vec::new(),
            route_names: Vec::new(),
            data_store: None,
            config_reload: None,
            image_updates: None,
            pod_security: None,
            run_as: None,
            selector: None,
            diagnoses: Vec::new(),
            role: ServiceRole::Worker,
            verdict: None,
        }
    }

    fn diagnosis(kind: DiagnosisKind, cause: &str, pods: &[&str]) -> Diagnosis {
        let cause = Message::new(cause);
        Diagnosis {
            kind,
            container: String::new(),
            pods: pods.iter().map(|p| (*p).to_owned()).collect(),
            group: zeabur_model::DiagnosisGroup::of(kind, &cause),
            cause,
            why: Message::new("why"),
            fix: Message::new("fix"),
            evidence: Vec::new(),
            last_seen_at: None,
            fault: Fault::default(),
            incident: None,
            refused_port: None,
        }
    }

    fn waiting_pod(name: &str, created: &str) -> PodSummary {
        PodSummary {
            id: format!("projects/p/pods/{name}"),
            name: name.to_owned(),
            phase: "Pending".to_owned(),
            ready: false,
            restarts: 0,
            health: Health::Progress,
            containers: Vec::new(),
            reason: Some("Unschedulable".to_owned()),
            cause: None,
            created_at: Some(created.parse().unwrap()),
            node: None,
            terminating: false,
            container_statuses: Vec::new(),
            origin: Origin::object("Pod", name),
        }
    }

    fn now() -> Timestamp {
        "2026-10-23T08:10:00Z".parse().unwrap()
    }

    const fn problem(word: StatusCause) -> Headline {
        Headline::Problem {
            trouble: Trouble::Reason { word },
        }
    }

    #[test]
    fn a_service_scaled_to_zero_is_stopped() {
        let v = of(&service(WorkloadKind::Deployment, Health::Idle, 0), now());
        assert_eq!(v.headline, Headline::Stopped);
        assert_eq!(v.tone, Health::Idle);
    }

    #[test]
    fn a_healthy_service_runs_or_deploys() {
        let running = of(&service(WorkloadKind::Deployment, Health::Ok, 1), now());
        assert_eq!(running.headline, Headline::Running);
        let rolling = of(
            &service(WorkloadKind::Deployment, Health::Progress, 1),
            now(),
        );
        assert_eq!(rolling.headline, Headline::Deploying);
    }

    #[test]
    fn a_problem_is_named_by_the_status_cause_before_the_diagnoses() {
        let mut s = service(WorkloadKind::Deployment, Health::Fail, 1);
        s.diagnoses = vec![diagnosis(DiagnosisKind::ProbeFailure, "c", &[])];
        let health_check = of(&s, now());
        let trouble = Trouble::HealthCheck;
        assert_eq!(health_check.headline, Headline::Problem { trouble });
        assert_eq!(health_check.tone, Health::Fail);
        s.status.cause = Some(StatusCause::OutOfMemory);
        assert_eq!(of(&s, now()).headline, problem(StatusCause::OutOfMemory));
        s.diagnoses
            .push(diagnosis(DiagnosisKind::PodSecurity, "c", &[]));
        assert_eq!(of(&s, now()).headline, problem(StatusCause::Security));
    }

    #[test]
    fn no_ready_endpoints_only_counts_without_another_cause() {
        let mut s = service(WorkloadKind::Deployment, Health::Fail, 1);
        s.diagnoses = vec![
            diagnosis(DiagnosisKind::Unreachable, "x.noReadyEndpoints", &[]),
            diagnosis(DiagnosisKind::CrashLoop, "c", &[]),
        ];
        assert_eq!(of(&s, now()).headline, problem(StatusCause::Crashing));
    }

    #[test]
    fn a_rollout_waiting_for_a_node_stays_in_progress_for_the_grace() {
        let mut s = service(WorkloadKind::Deployment, Health::Progress, 1);
        s.diagnoses = vec![diagnosis(DiagnosisKind::Unschedulable, "c", &["web-1"])];
        s.pods = vec![waiting_pod("web-1", "2026-10-23T08:08:00Z")];
        assert_eq!(of(&s, now()).tone, Health::Progress);
        s.pods = vec![waiting_pod("web-1", "2026-10-23T08:00:00Z")];
        assert_eq!(of(&s, now()).tone, Health::Fail);
    }

    #[test]
    fn a_warning_service_keeps_its_tone() {
        let mut s = service(WorkloadKind::Deployment, Health::Warn, 2);
        s.diagnoses = vec![diagnosis(DiagnosisKind::CrashLoop, "c", &[])];
        assert_eq!(of(&s, now()).tone, Health::Warn);
    }

    #[test]
    fn a_task_says_its_status_word() {
        let mut s = service(WorkloadKind::CronJob, Health::Idle, 0);
        s.status.word = StatusWord::Scheduled;
        let word = StatusWord::Scheduled;
        assert_eq!(of(&s, now()).headline, Headline::Status { word });
    }

    #[test]
    fn a_role_comes_from_routes_then_data_store_then_kind_then_endpoints() {
        let mut s = service(WorkloadKind::Deployment, Health::Ok, 1);
        assert_eq!(s.role_now(), ServiceRole::Worker);
        s.endpoint_names = vec!["web".to_owned()];
        assert_eq!(s.role_now(), ServiceRole::Internal);
        s.data_store = Some(zeabur_model::DataStore::Postgres);
        assert_eq!(s.role_now(), ServiceRole::Data);
        s.route_names = vec!["web".to_owned()];
        assert_eq!(s.role_now(), ServiceRole::Web);
        let mut job = service(WorkloadKind::CronJob, Health::Idle, 0);
        job.endpoint_names = vec!["web".to_owned()];
        assert_eq!(job.role_now(), ServiceRole::Job);
    }
}
