//! The five workload kinds behind one shape.

use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{
    Container, EnvFromSource, EnvVar, PodSpec, PodTemplateSpec, Probe, ResourceRequirements,
    VolumeMount,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use zeabur_adapters::group::{DATABASES_GROUP, OPERATORS_GROUP, explicit_group, resolve_group};
use zeabur_adapters::operators::{database_cluster, is_operator};
use zeabur_model::{
    Classification, ManagedBy, Origin, Phase, Replicas, WorkloadKind, WorkloadRef, WorkloadSummary,
};

use crate::project::provenance;
use crate::snapshot::{Snapshot, name_of};

/// One workload of a [`Snapshot`], whatever its kind.
#[derive(Debug, Clone, Copy)]
pub enum Workload<'a> {
    /// A CronJob.
    CronJob(&'a CronJob),
    /// A DaemonSet.
    DaemonSet(&'a DaemonSet),
    /// A Deployment.
    Deployment(&'a Deployment),
    /// A Job.
    Job(&'a Job),
    /// A StatefulSet.
    StatefulSet(&'a StatefulSet),
}

impl<'a> Workload<'a> {
    /// The workload's kind.
    #[must_use]
    pub const fn kind(self) -> WorkloadKind {
        match self {
            Self::CronJob(_) => WorkloadKind::CronJob,
            Self::DaemonSet(_) => WorkloadKind::DaemonSet,
            Self::Deployment(_) => WorkloadKind::Deployment,
            Self::Job(_) => WorkloadKind::Job,
            Self::StatefulSet(_) => WorkloadKind::StatefulSet,
        }
    }

    /// The workload's metadata.
    #[must_use]
    pub const fn meta(self) -> &'a ObjectMeta {
        match self {
            Self::CronJob(object) => &object.metadata,
            Self::DaemonSet(object) => &object.metadata,
            Self::Deployment(object) => &object.metadata,
            Self::Job(object) => &object.metadata,
            Self::StatefulSet(object) => &object.metadata,
        }
    }

    /// The workload's name.
    #[must_use]
    pub fn name(self) -> &'a str {
        name_of(self.meta())
    }

    /// The workload by kind and name.
    #[must_use]
    pub fn reference(self) -> WorkloadRef {
        WorkloadRef {
            kind: self.kind(),
            name: self.name().to_owned(),
        }
    }

    /// The pod template; one level deeper for a CronJob.
    #[must_use]
    pub fn pod_template(self) -> Option<&'a PodTemplateSpec> {
        match self {
            Self::CronJob(object) => object
                .spec
                .as_ref()
                .and_then(|spec| spec.job_template.spec.as_ref())
                .map(|spec| &spec.template),
            Self::DaemonSet(object) => object.spec.as_ref().map(|spec| &spec.template),
            Self::Deployment(object) => object.spec.as_ref().map(|spec| &spec.template),
            Self::Job(object) => object.spec.as_ref().map(|spec| &spec.template),
            Self::StatefulSet(object) => object.spec.as_ref().map(|spec| &spec.template),
        }
    }

    /// The pod spec of the template.
    #[must_use]
    pub fn pod_spec(self) -> Option<&'a PodSpec> {
        self.pod_template()
            .and_then(|template| template.spec.as_ref())
    }

    /// The field path of the pod spec inside the workload (`spec.template.spec`).
    pub(crate) fn pod_spec_path(self) -> String {
        let mut path = self.kind().pod_template_path().join(".");
        path.push_str(".spec");
        path
    }

    /// The field `path` of this workload.
    pub(crate) fn origin(self, path: String) -> Origin {
        Origin::field(self.kind().as_str(), self.name(), path)
    }
}

/// The workloads of a snapshot, sorted by kind then name.
///
/// A Job whose controller is a CronJob is one of its runs, not a workload of its own: listing it
/// would show every scheduled task twice.
#[must_use]
pub fn workloads(snapshot: &Snapshot) -> Vec<Workload<'_>> {
    let mut out: Vec<Workload<'_>> = snapshot
        .cron_jobs
        .iter()
        .map(Workload::CronJob)
        .chain(snapshot.daemon_sets.iter().map(Workload::DaemonSet))
        .chain(snapshot.deployments.iter().map(Workload::Deployment))
        .chain(
            snapshot
                .jobs
                .iter()
                .filter(|job| !controlled_by(&job.metadata, "CronJob"))
                .map(Workload::Job),
        )
        .chain(snapshot.stateful_sets.iter().map(Workload::StatefulSet))
        .collect();
    out.sort_by(|a, b| (a.kind(), a.name()).cmp(&(b.kind(), b.name())));
    out
}

/// Whether the object's controller owner reference is of `kind`.
fn controlled_by(meta: &ObjectMeta, kind: &str) -> bool {
    zeabur_adapters::controller_of(meta).is_some_and(|owner| owner.kind == kind)
}

/// One container of a pod spec, whichever list it is in.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ContainerView<'a> {
    /// The list's field name: `initContainers`, `containers` or `ephemeralContainers`.
    pub(crate) list: &'static str,
    pub(crate) name: &'a str,
    pub(crate) image: Option<&'a str>,
    pub(crate) env: &'a [EnvVar],
    pub(crate) env_from: &'a [EnvFromSource],
    pub(crate) volume_mounts: &'a [VolumeMount],
    pub(crate) resources: Option<&'a ResourceRequirements>,
    /// Liveness, readiness and startup, in that order.
    pub(crate) probes: [Option<&'a Probe>; 3],
}

impl ContainerView<'_> {
    fn of<'a>(list: &'static str, container: &'a Container) -> ContainerView<'a> {
        ContainerView {
            list,
            name: &container.name,
            image: container.image.as_deref(),
            env: container.env.as_deref().unwrap_or_default(),
            env_from: container.env_from.as_deref().unwrap_or_default(),
            volume_mounts: container.volume_mounts.as_deref().unwrap_or_default(),
            resources: container.resources.as_ref(),
            probes: [
                container.liveness_probe.as_ref(),
                container.readiness_probe.as_ref(),
                container.startup_probe.as_ref(),
            ],
        }
    }

    /// The field path of the container inside `workload`.
    pub(crate) fn path(&self, workload: Workload<'_>) -> String {
        format!(
            "{}.{}[name={}]",
            workload.pod_spec_path(),
            self.list,
            self.name
        )
    }

    /// Whether this is an init container.
    pub(crate) fn is_init(&self) -> bool {
        self.list == "initContainers"
    }
}

/// Every container of a pod spec: init, regular, then ephemeral. Missing one list would leave
/// its references invisible.
pub(crate) fn containers(spec: &PodSpec) -> impl Iterator<Item = ContainerView<'_>> {
    let init = spec
        .init_containers
        .iter()
        .flatten()
        .map(|container| ContainerView::of("initContainers", container));
    let regular = spec
        .containers
        .iter()
        .map(|container| ContainerView::of("containers", container));
    let ephemeral = spec
        .ephemeral_containers
        .iter()
        .flatten()
        .map(|container| ContainerView {
            list: "ephemeralContainers",
            name: &container.name,
            image: container.image.as_deref(),
            env: container.env.as_deref().unwrap_or_default(),
            env_from: container.env_from.as_deref().unwrap_or_default(),
            volume_mounts: container.volume_mounts.as_deref().unwrap_or_default(),
            // Ephemeral containers may not set resources or probes.
            resources: None,
            probes: [None; 3],
        });
    init.chain(regular).chain(ephemeral)
}

/// The container called `name` in the workload's pod template.
pub(crate) fn find_container<'a>(workload: Workload<'a>, name: &str) -> Option<ContainerView<'a>> {
    workload
        .pod_spec()
        .and_then(|spec| containers(spec).find(|container| container.name == name))
}

/// Where a workload is listed and under which name.
///
/// A workload a database operator controls belongs to that database: it records the managing
/// resource, is shown under the cluster's name when it holds the data, and goes to the
/// `databases` group. An operator's own Deployment goes to `operators`. The explicit group
/// label wins over both; otherwise the labels decide
/// ([`resolve_group`]).
#[must_use]
pub fn classify(workload: Workload<'_>) -> Classification {
    let meta = workload.meta();
    let labels = meta.labels.as_ref();
    let explicit = explicit_group(labels);
    if let Some(cluster) = database_cluster(meta) {
        return Classification {
            group: explicit.unwrap_or(DATABASES_GROUP).to_owned(),
            display_name: cluster.holds_data.then(|| cluster.name.to_owned()),
            managed_by: Some(ManagedBy {
                kind: cluster.kind.to_owned(),
                name: cluster.name.to_owned(),
                engine: Some(cluster.engine.as_str().to_owned()),
            }),
        };
    }
    let operator = explicit.is_none()
        && matches!(workload, Workload::Deployment(_))
        && is_operator(
            labels,
            workload
                .pod_template()
                .and_then(|template| template.metadata.as_ref())
                .and_then(|meta| meta.labels.as_ref()),
        );
    Classification {
        group: if operator {
            OPERATORS_GROUP
        } else {
            resolve_group(labels)
        }
        .to_owned(),
        display_name: None,
        managed_by: None,
    }
}

/// The phase of a Deployment, from its rollout conditions rather than replica counts alone, so a
/// stuck rollout is reported as one.
///
/// A rollout is under way until the controller has observed the current generation and its
/// `Progressing` condition says the new ReplicaSet is available. A `Progressing` condition that
/// is `False` with reason `ProgressDeadlineExceeded` is a failed rollout. With no ready pod and
/// no rollout under way, pods that were once available have stopped serving: degraded.
#[must_use]
pub fn deployment_phase(deployment: &Deployment) -> Phase {
    let spec = deployment.spec.as_ref();
    if spec.and_then(|spec| spec.paused) == Some(true) {
        return Phase::Paused;
    }
    let status = deployment.status.as_ref();
    let generation = deployment.metadata.generation.unwrap_or_default();
    let observed = status
        .and_then(|status| status.observed_generation)
        .unwrap_or_default();
    let unobserved = observed < generation;
    let mut rolling = unobserved;
    for condition in status
        .and_then(|status| status.conditions.as_ref())
        .into_iter()
        .flatten()
        .filter(|condition| condition.type_ == "Progressing")
    {
        let reason = condition.reason.as_deref().unwrap_or_default();
        if condition.status == "False" && reason == "ProgressDeadlineExceeded" {
            return Phase::Failed;
        }
        if condition.status == "True" && reason != "NewReplicaSetAvailable" {
            rolling = true;
        }
    }
    let desired = desired_replicas(spec.and_then(|spec| spec.replicas));
    let ready = status
        .and_then(|status| status.ready_replicas)
        .unwrap_or_default();
    let updated = status
        .and_then(|status| status.updated_replicas)
        .unwrap_or_default();
    if desired == 0 {
        Phase::Paused
    } else if !unobserved && ready >= desired && updated >= desired {
        Phase::Running
    } else if ready == 0 && !rolling {
        Phase::Degraded
    } else {
        Phase::Progressing
    }
}

/// `spec.replicas`, which defaults to one.
fn desired_replicas(replicas: Option<i32>) -> i32 {
    replicas.unwrap_or(1)
}

/// The workload's phase.
#[must_use]
pub fn phase(workload: Workload<'_>) -> Phase {
    match workload {
        Workload::Deployment(deployment) => deployment_phase(deployment),
        Workload::StatefulSet(_) | Workload::DaemonSet(_) => {
            let Some(Replicas { ready, desired }) = replicas(workload) else {
                return Phase::Unknown;
            };
            match (desired, matches!(workload, Workload::StatefulSet(_))) {
                (0, true) => Phase::Paused,
                (0, false) => Phase::Unknown,
                _ if ready >= desired => Phase::Running,
                _ if ready == 0 => Phase::Degraded,
                _ => Phase::Progressing,
            }
        }
        Workload::Job(job) => job_phase(job),
        Workload::CronJob(cron_job) => {
            if cron_job.spec.as_ref().and_then(|spec| spec.suspend) == Some(true) {
                Phase::Suspended
            } else {
                Phase::Running
            }
        }
    }
}

/// The phase of a Job: its `Complete` and `Failed` conditions first, then its pod counts.
fn job_phase(job: &Job) -> Phase {
    let condition = |kind: &str| {
        job.status
            .as_ref()
            .and_then(|status| status.conditions.as_ref())
            .into_iter()
            .flatten()
            .any(|condition| condition.type_ == kind && condition.status == "True")
    };
    let status = job.status.as_ref();
    let count = |field: fn(&k8s_openapi::api::batch::v1::JobStatus) -> Option<i32>| {
        status.and_then(field).unwrap_or_default()
    };
    let desired = job
        .spec
        .as_ref()
        .and_then(|spec| spec.completions)
        .unwrap_or(1);
    if job.spec.as_ref().and_then(|spec| spec.suspend) == Some(true) {
        Phase::Suspended
    } else if condition("Complete") || count(|status| status.succeeded) >= desired {
        Phase::Completed
    } else if condition("Failed") {
        Phase::Failed
    } else if count(|status| status.active) > 0 {
        Phase::Progressing
    } else if count(|status| status.failed) > 0 {
        Phase::Failed
    } else {
        Phase::Unknown
    }
}

/// Ready and desired pods; `None` for a CronJob, which has no meaningful ratio.
#[must_use]
pub fn replicas(workload: Workload<'_>) -> Option<Replicas> {
    match workload {
        Workload::Deployment(object) => Some(Replicas {
            ready: object
                .status
                .as_ref()
                .and_then(|status| status.ready_replicas)
                .unwrap_or_default(),
            desired: desired_replicas(object.spec.as_ref().and_then(|spec| spec.replicas)),
        }),
        Workload::StatefulSet(object) => Some(Replicas {
            ready: object
                .status
                .as_ref()
                .and_then(|status| status.ready_replicas)
                .unwrap_or_default(),
            desired: desired_replicas(object.spec.as_ref().and_then(|spec| spec.replicas)),
        }),
        Workload::DaemonSet(object) => Some(object.status.as_ref().map_or(
            Replicas {
                ready: 0,
                desired: 0,
            },
            |status| Replicas {
                ready: status.number_ready,
                desired: status.desired_number_scheduled,
            },
        )),
        Workload::Job(object) => Some(Replicas {
            ready: object
                .status
                .as_ref()
                .and_then(|status| status.succeeded)
                .unwrap_or_default(),
            desired: object
                .spec
                .as_ref()
                .and_then(|spec| spec.completions)
                .unwrap_or(1),
        }),
        Workload::CronJob(_) => None,
    }
}

/// The workload as lists show it.
#[must_use]
pub fn summarise(snapshot: &Snapshot, workload: Workload<'_>) -> WorkloadSummary {
    WorkloadSummary {
        kind: workload.kind(),
        name: workload.name().to_owned(),
        namespace: workload
            .meta()
            .namespace
            .clone()
            .unwrap_or_else(|| snapshot.namespace.clone()),
        classification: classify(workload),
        phase: phase(workload),
        replicas: replicas(workload),
        image: workload
            .pod_spec()
            .and_then(|spec| spec.containers.first())
            .and_then(|container| container.image.clone()),
        created: workload
            .meta()
            .creation_timestamp
            .as_ref()
            .map(|time| time.0.to_string()),
        provenance: provenance(workload.meta()),
        origin: Origin::object(workload.kind().as_str(), workload.name()),
    }
}
