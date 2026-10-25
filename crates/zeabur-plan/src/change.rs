//! What a person asks to change, and what a plan says about it.
#![cfg_attr(
    feature = "openapi",
    expect(
        clippy::large_stack_frames,
        reason = "the derived schema of `Change` builds every variant in one function"
    )
)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeabur_model::{PodSecurityLevel, ProjectIcon, TaskConcurrency, WorkloadRef};

use crate::checks::{ProbeRequest, ResourceRef, VolumeRequest};
use crate::rules::{EnvEdit, FieldChange};

/// One change a person asks for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Change {
    /// Set or remove environment variables of one container of a workload.
    #[serde(rename_all = "camelCase")]
    Env {
        /// The workload.
        workload: WorkloadRef,
        /// The container; may be left out when the workload has only one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
        /// The variables to set or remove.
        changes: Vec<EnvEdit>,
    },
    /// Set or remove keys of a ConfigMap or Secret.
    #[serde(rename_all = "camelCase")]
    Data {
        /// `ConfigMap` or `Secret`.
        kind: DataKind,
        /// Its name.
        name: String,
        /// The keys to set or remove.
        changes: Vec<DataEdit>,
    },
    /// Route a new hostname to a workload.
    AddDomain(AddDomainChange),
    /// Replace a hostname of a route with another.
    #[serde(rename_all = "camelCase")]
    RenameDomain {
        /// The route.
        route: RouteRef,
        /// The hostname now.
        from: String,
        /// The hostname wanted.
        to: String,
    },
    /// Stop routing a hostname.
    #[serde(rename_all = "camelCase")]
    RemoveDomain {
        /// The route.
        route: RouteRef,
        /// The hostname.
        hostname: String,
    },
    /// Redirect a hostname to another hostname, or stop (see the `access` module).
    Redirect(RedirectChange),
    /// Ask for a user name and password on every hostname of an Ingress, or stop (see the
    /// `access` module).
    BasicAuth(BasicAuthChange),
    /// Write the HTML of the project's error pages, in the ConfigMap of its page server (see the
    /// `error_pages` module).
    ErrorPageHtml(ErrorPageHtml),
    /// Deploy the page server from Zeabur Catalog's `error-pages` template, with this HTML in
    /// place of the template's pages. The API opens it as a [`Change::Template`] and writes the
    /// HTML into the template's ConfigMap of pages.
    PageServer(ErrorPageHtml),
    /// Deploy a template of Zeabur Catalog in the project. The API renders it and opens it into
    /// the `create` of each object. The values of its generated inputs are Secrets that the same
    /// request creates.
    Template(TemplateChange),
    /// Show the project's error pages on every hostname of an Ingress, or stop.
    ErrorPages(ErrorPagesChange),
    /// Show the maintenance page on a hostname instead of its workload, or stop.
    Maintenance(MaintenanceChange),
    /// Change where the certificate of one hostname of an Ingress comes from (see the
    /// `certificates` module).
    Certificate(CertificateChange),
    /// Give a container every key of a ConfigMap or Secret as variables (`envFrom`), or stop
    /// (see the `shared` module).
    EnvImport(EnvImportChange),
    /// Open a port of a workload to the internet with a `LoadBalancer` Service, or close it (see
    /// the `public_port` module).
    PublicPort(PublicPortChange),
    /// Replace a workload's pods, as `kubectl rollout restart` does.
    #[serde(rename_all = "camelCase")]
    Restart {
        /// The workload.
        workload: WorkloadRef,
    },
    /// Return a workload's pod template to an earlier release, as `kubectl rollout undo
    /// --to-revision` does. ConfigMap and Secret contents stay as they are.
    #[serde(rename_all = "camelCase")]
    Rollback {
        /// The workload: a Deployment, StatefulSet or DaemonSet.
        workload: WorkloadRef,
        /// The release to return to.
        revision: i64,
    },
    /// Ask for more space on a PersistentVolumeClaim; claims only grow.
    #[serde(rename_all = "camelCase")]
    ExpandVolume {
        /// The claim.
        claim: String,
        /// The new size, a Kubernetes quantity such as `20Gi`.
        size: String,
    },
    /// Add labels and annotations to a PersistentVolumeClaim, such as the ones a backup tool
    /// selects claims by.
    LabelVolume(LabelVolumeChange),
    /// Create a PersistentVolumeClaim and mount it in a container of a workload.
    #[serde(rename_all = "camelCase")]
    AddVolume {
        /// The workload.
        workload: WorkloadRef,
        /// The container; may be left out when the workload has only one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
        /// The volume.
        volume: VolumeRequest,
    },
    /// Move a mount, or make it read-only or writable.
    #[serde(rename_all = "camelCase")]
    EditMount {
        /// The workload.
        workload: WorkloadRef,
        /// The container; may be left out when the workload has only one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
        /// Where it is mounted now.
        mount_path: String,
        /// Where to mount it instead.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        new_mount_path: Option<String>,
        /// Mount it read-only (or not).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        read_only: Option<bool>,
    },
    /// Mount another PersistentVolumeClaim where a workload mounts `from`, such as one restored
    /// from a backup. A Deployment stops its old pod before it starts the new one (`Recreate`),
    /// since a `ReadWriteOnce` claim is mounted by one node at a time. `from` stays, with its
    /// data.
    #[serde(rename_all = "camelCase")]
    SwapClaim {
        /// The workload.
        workload: WorkloadRef,
        /// The claim it mounts now.
        from: String,
        /// The claim to mount instead.
        to: String,
    },
    /// Stop mounting a volume in a container, and drop it from the pod template once nothing
    /// mounts it. A PersistentVolumeClaim it names stays, with its data.
    #[serde(rename_all = "camelCase")]
    RemoveVolume {
        /// The workload.
        workload: WorkloadRef,
        /// The container; may be left out when the workload has only one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
        /// Where it is mounted.
        mount_path: String,
    },
    /// Set or remove a liveness, readiness or startup probe of a container.
    #[serde(rename_all = "camelCase")]
    Probe {
        /// The workload.
        workload: WorkloadRef,
        /// The container; may be left out when the workload has only one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
        /// Which probe.
        probe: ProbeKind,
        /// The probe; absent to remove it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        set: Option<ProbeRequest>,
    },
    /// Have Stakater Reloader replace a workload's pods when a ConfigMap or Secret it reads
    /// changes, or stop it: the workload's `reloader.stakater.com/auto` annotation (see the
    /// `reload` module).
    #[serde(rename_all = "camelCase")]
    ConfigReload {
        /// The workload: a Deployment, StatefulSet or DaemonSet.
        workload: WorkloadRef,
        /// On or off.
        enabled: bool,
    },
    /// Run another image in one container of a workload.
    #[serde(rename_all = "camelCase")]
    Image {
        /// The workload.
        workload: WorkloadRef,
        /// The container; may be left out when the workload has only one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
        /// The image reference, such as a Zeabur Builder `Build`'s `status.latestImage`.
        image: String,
    },
    /// Build a Zeabur Builder `Build`'s current commit again (see the `builds` module).
    #[serde(rename_all = "camelCase")]
    Rebuild {
        /// The `Build`.
        build: String,
    },
    /// Set what a Zeabur Builder `Build` builds: its directory, its Dockerfile and its build
    /// variables (see the `builds` module).
    BuildOptions(BuildOptionsChange),
    /// Have Keel move a workload to each new image, or stop it (see the `builds` module).
    #[serde(rename_all = "camelCase")]
    AutoUpdate {
        /// The workload: a Deployment, StatefulSet or DaemonSet.
        workload: WorkloadRef,
        /// On or off.
        enabled: bool,
        /// The `Build` whose new images Keel is told of, when turning it on.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        build: Option<String>,
        /// Keel's native webhook, `…/v1/webhooks/native`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        webhook: Option<String>,
    },
    /// Run this many pods of a Deployment or StatefulSet.
    #[serde(rename_all = "camelCase")]
    Scale {
        /// The workload.
        workload: WorkloadRef,
        /// The replicas.
        replicas: i32,
    },
    /// Set a container's CPU, memory and ephemeral storage requests and limits; those left out
    /// are removed, other resources (such as GPUs) are kept.
    #[serde(rename_all = "camelCase")]
    Resources {
        /// The workload.
        workload: WorkloadRef,
        /// The container; may be left out when the workload has only one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
        /// Requests, by resource (`cpu`, `memory`, `ephemeral-storage`).
        #[serde(default)]
        requests: BTreeMap<String, String>,
        /// Limits, the same way.
        #[serde(default)]
        limits: BTreeMap<String, String>,
    },
    /// Set the Pod Security level a workload's pods meet (see the `pod_security` module):
    /// `restricted` writes the settings the restricted standard asks for into its pod template;
    /// `baseline` removes the ones that keep an image running as root from starting. The pods
    /// are replaced.
    #[serde(rename_all = "camelCase")]
    SecurityProfile {
        /// The workload: a Deployment, StatefulSet, DaemonSet or CronJob.
        workload: WorkloadRef,
        /// `restricted` or `baseline`.
        profile: PodSecurityLevel,
    },
    /// Set the Pod Security level enforced in the project, the Namespace's
    /// `pod-security.kubernetes.io/enforce` label (see the `pod_security` module). `baseline`
    /// lowers only a `restricted` label and writes nothing otherwise; `restricted` is refused
    /// while a workload, as the plan leaves it, does not meet it. Written before the other
    /// changes of the plan.
    #[serde(rename_all = "camelCase")]
    PodSecurity {
        /// `restricted` or `baseline`.
        level: PodSecurityLevel,
    },
    /// Replace one object of the project with an edited copy, written as YAML.
    ///
    /// Fields the server sets (`status`, `resourceVersion`, `managedFields`, …) are kept whatever
    /// the copy says. A ConfigMap's or Secret's edit becomes key changes: a Secret value shown
    /// masked and sent back unchanged is kept.
    #[serde(rename_all = "camelCase")]
    Edit {
        /// The object's resource.
        resource: ResourceRef,
        /// Its name.
        name: String,
        /// The edited object, YAML (JSON is YAML too).
        yaml: String,
    },
    /// Create one object in the project, of any namespaced kind the cluster serves (see the
    /// `create` module). Its namespace is the project's; an object of that name must not exist
    /// yet. Several are created in dependency order, without a transaction.
    #[serde(rename_all = "camelCase")]
    Create {
        /// The object: `apiVersion`, `kind`, `metadata.name`, and its fields. Left out when
        /// `yaml` gives it.
        #[serde(default, skip_serializing_if = "Value::is_null")]
        #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
        object: Value,
        /// The object as one YAML document, as a person wrote it; a problem with it names the
        /// line.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        yaml: Option<String>,
    },
    /// Create the Secret that holds an SSH deploy key Zeabur generated, for Zeabur Builder to
    /// fetch a private repository: a `kubernetes.io/ssh-auth` Secret with the private key and the
    /// Git host's `known_hosts`, labelled `builder.zeabur.com/git-credentials: "true"`. The key is
    /// sealed by the server that made it; the API opens it and plans the Secret as a `create`,
    /// so the private key never travels in the clear.
    #[serde(rename_all = "camelCase")]
    DeployKey {
        /// The Secret's name.
        name: String,
        /// The sealed key, as `POST /projects/{p}/deploy-keys` returned it.
        key: String,
    },
    /// Create a Secret with the keys of an S3 destination from the cluster's list. The API reads
    /// the destination's Secret as the person and plans the Secret as a `create`, so the keys
    /// never travel through the browser. The Secret is labelled with the destination and names
    /// its keys, so a plan that changes the destination's keys can update it.
    CopyS3Keys(CopyS3KeysChange),
    /// Delete one object of the project, as `kubectl delete` does: what it owns goes after it
    /// (background propagation). Refused for an object Helm, Argo CD or Flux manages, and for one
    /// a controller owns, except a Pod, which its owner replaces (see the `delete` module).
    #[serde(rename_all = "camelCase")]
    Delete {
        /// The object's resource.
        resource: ResourceRef,
        /// Its name.
        name: String,
        /// The `resourceVersion` the object must still have, such as the one an apply returned
        /// for an object it just created; when left out, the version the plan reads.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resource_version: Option<String>,
        /// For a workload: also delete what the service made and nothing else uses, its
        /// Kubernetes Services, routes, ConfigMaps, Secrets and claims (see the `owned` module).
        /// What it uses that stays is named in a warning with the reason.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        owned: bool,
    },
    /// Make the namespace a project: add the label `zeabur.com/project: "true"` to the
    /// Namespace, and nothing else. Refused for the namespaces Kubernetes keeps for itself
    /// (`kube-` prefix). Planned on its own or with the project's details only.
    Adopt,
    /// Stop showing the namespace as a project: remove that label, and nothing else. Nothing in
    /// the namespace is deleted.
    Unadopt,
    /// Create the project's Namespace, with the project label and, when given, its display
    /// name. Refused when a namespace of that name exists (adopt it instead), and for a name
    /// Kubernetes keeps for itself (`kube-` prefix). Planned on its own.
    #[serde(rename_all = "camelCase")]
    CreateProject {
        /// The name to show instead of the namespace's.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
    },
    /// Set the project's details, the Namespace's `zeabur.com/display-name`,
    /// `…/description` and `…/icon` annotations: each one left out (or blank) is removed.
    #[serde(rename_all = "camelCase")]
    Details {
        /// The name to show instead of the namespace's.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        /// What the project is.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        /// Its icon.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<ProjectIcon>,
    },
    /// Make a scheduled task from a service: a CronJob that runs a command in a copy of one of
    /// its containers (see the `tasks` module).
    #[serde(rename_all = "camelCase")]
    ScheduleTask {
        /// The service: a Deployment, StatefulSet or DaemonSet.
        from: WorkloadRef,
        /// The container to copy; may be left out when the workload has only one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
        /// The CronJob's name.
        name: String,
        /// When it runs, a cron expression such as `0 3 * * *`.
        schedule: String,
        /// The IANA time zone of the schedule; by default the controller's, usually UTC.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time_zone: Option<String>,
        /// The command, run by `/bin/sh -c`.
        command: String,
        /// What happens when a run is due while the last one still runs.
        #[serde(default)]
        concurrency: TaskConcurrency,
        /// How many succeeded runs to keep.
        #[serde(default = "three")]
        keep_succeeded: i32,
        /// How many failed runs to keep.
        #[serde(default = "one")]
        keep_failed: i32,
    },
    /// Run a CronJob now, as `kubectl create job --from=cronjob/<name>` does, under its
    /// `concurrencyPolicy`: `Forbid` refuses while a run is active, `Replace` deletes the active
    /// runs first.
    #[serde(rename_all = "camelCase")]
    RunTask {
        /// The CronJob.
        cron_job: String,
    },
    /// Change a scheduled task. Each field given replaces the CronJob's; the runs already started
    /// keep going.
    #[serde(rename_all = "camelCase")]
    EditTask {
        /// The CronJob.
        cron_job: String,
        /// When it runs, a cron expression.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schedule: Option<String>,
        /// The IANA time zone of the schedule; empty removes it, for the controller's.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time_zone: Option<String>,
        /// The command, run by `/bin/sh -c`. Only for a task whose only container runs one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        command: Option<String>,
        /// What happens when a run is due while the last one still runs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        concurrency: Option<TaskConcurrency>,
        /// How many succeeded runs to keep.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keep_succeeded: Option<i32>,
        /// How many failed runs to keep.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keep_failed: Option<i32>,
        /// Stop starting runs on schedule (`spec.suspend`), or start again.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        paused: Option<bool>,
    },
    /// Add a debug container to a pod, for a terminal in a container whose image has no shell:
    /// an ephemeral container with a shell image, sharing the target container's processes
    /// (see the `terminal` module). Kubernetes keeps it until the pod is replaced. When one of
    /// Zeabur's still runs for that container, the plan writes nothing and says so. Planned on
    /// its own.
    #[serde(rename_all = "camelCase")]
    DebugContainer {
        /// The pod.
        pod: String,
        /// The container whose processes it shares; by default the pod's default container.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
    },
    /// Start a pod with `kubectl` and `helm` that act as the person, for a terminal (see the
    /// `terminal` module). When the person's tools pod still runs, the plan writes nothing and
    /// says so. Planned on its own.
    ClusterTools,
}

impl Change {
    /// Whether the change is planned for a terminal, alone ([`Change::DebugContainer`],
    /// [`Change::ClusterTools`]).
    #[must_use]
    pub const fn is_terminal_change(&self) -> bool {
        matches!(self, Self::DebugContainer { .. } | Self::ClusterTools)
    }

    /// Whether the change writes the project's Namespace (creating it, adopting it, letting it
    /// go, its details) rather than objects in it.
    #[must_use]
    pub const fn is_namespace_change(&self) -> bool {
        matches!(
            self,
            Self::CreateProject { .. } | Self::Adopt | Self::Unadopt | Self::Details { .. }
        )
    }
}

/// The Secret of a [`Change::CopyS3Keys`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct CopyS3KeysChange {
    /// The destination's name.
    pub destination: String,
    /// The Secret to create, as for `create`; `stringData` holds its other keys.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub object: Value,
    /// The key the access key ID goes under.
    pub access_key_id: String,
    /// The key the secret access key goes under.
    pub secret_access_key: String,
}

const fn yes() -> bool {
    true
}

const fn three() -> i32 {
    3
}

const fn one() -> i32 {
    1
}

/// A [`Change::LabelVolume`]. A key the claim has already is set to the new value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct LabelVolumeChange {
    /// The claim.
    pub claim: String,
    /// Labels to set.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// Annotations to set.
    #[serde(default)]
    pub annotations: BTreeMap<String, String>,
}

/// A [`Change::Redirect`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct RedirectChange {
    /// The route.
    pub route: RouteRef,
    /// The workload the route sends requests to. An `HTTPRoute` that stops redirecting sends
    /// them to it again.
    pub workload: WorkloadRef,
    /// The hostname that redirects.
    pub hostname: String,
    /// The hostname to redirect to; absent stops the redirect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// `301 Moved Permanently` when true, else `302 Found`.
    #[serde(default = "yes")]
    pub permanent: bool,
}

/// A [`Change::EnvImport`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct EnvImportChange {
    /// The workload.
    pub workload: WorkloadRef,
    /// The container; may be left out when the workload has only one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// `ConfigMap` or `Secret`.
    pub kind: DataKind,
    /// Its name.
    pub name: String,
    /// Stop importing it instead.
    #[serde(default)]
    pub remove: bool,
}

/// A [`Change::BuildOptions`]. A field left out stays as it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct BuildOptionsChange {
    /// The `Build`.
    pub build: String,
    /// The build context inside the repository, `spec.source.git.contextDir`. Empty: the
    /// repository's root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_dir: Option<String>,
    /// The Dockerfile, relative to the context, `spec.source.git.dockerfile`. Empty: `Dockerfile`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dockerfile: Option<String>,
    /// The build variables to set or remove, `spec.env`. A value from a Secret names only
    /// `secretName` and `key`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<EnvEdit>,
}

/// A [`Change::PublicPort`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PublicPortChange {
    /// The workload.
    pub workload: WorkloadRef,
    /// The workload's Service that has the port.
    pub service: String,
    /// The Service port. It is also the port on the internet.
    pub port: i32,
    /// Close the port instead: delete the `LoadBalancer` Service that opened it.
    #[serde(default)]
    pub remove: bool,
}

/// A [`Change::BasicAuth`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct BasicAuthChange {
    /// The Ingress.
    pub route: RouteRef,
    /// The one user that may sign in; absent stops asking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<BasicAuthUser>,
}

/// The user of a [`Change::BasicAuth`]. Zeabur keeps only a bcrypt hash of the password.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct BasicAuthUser {
    /// The user name; no colon.
    pub username: String,
    /// The password, 8 to 72 bytes.
    pub password: String,
}

impl std::fmt::Debug for BasicAuthUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BasicAuthUser")
            .field("username", &self.username)
            .finish_non_exhaustive()
    }
}

/// A [`Change::ErrorPageHtml`] or [`Change::PageServer`]: the HTML of each page. A page left out
/// keeps its HTML.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ErrorPageHtml {
    /// `502.html`: the workload sent no valid answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bad_gateway: Option<String>,
    /// `503.html`: the workload cannot answer now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_unavailable: Option<String>,
    /// `504.html`: the workload did not answer in time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway_timeout: Option<String>,
    /// `maintenance.html`: what a hostname in maintenance shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintenance: Option<String>,
}

/// A [`Change::Template`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct TemplateChange {
    /// The template's id (`miniflux`).
    pub template: String,
    /// The domain, for a template that routes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// The values of the inputs that are not generated, by name.
    #[serde(default)]
    pub inputs: BTreeMap<String, String>,
}

/// A [`Change::ErrorPages`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ErrorPagesChange {
    /// The Ingress.
    pub route: RouteRef,
    /// Show the pages when true; stop when false.
    pub enabled: bool,
}

/// A [`Change::Maintenance`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceChange {
    /// The route.
    pub route: RouteRef,
    /// The hostname.
    pub hostname: String,
    /// Show the maintenance page when true; send requests to the workload again when false.
    pub enabled: bool,
    /// When stopping: the workload whose first Service gets the hostname when the rules changed
    /// after maintenance started, so the backends kept then no longer fit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workload: Option<WorkloadRef>,
}

/// The probes of a container [k8s-probes].
///
/// [k8s-probes]: https://kubernetes.io/docs/concepts/configuration/liveness-readiness-startup-probes/
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum ProbeKind {
    /// Restarts the container when it fails.
    Liveness,
    /// Takes the pod out of its Services while it fails.
    Readiness,
    /// Holds the other two until it succeeds once.
    Startup,
}

impl ProbeKind {
    /// The container field (`livenessProbe`, …).
    #[must_use]
    pub const fn field(self) -> &'static str {
        match self {
            Self::Liveness => "livenessProbe",
            Self::Readiness => "readinessProbe",
            Self::Startup => "startupProbe",
        }
    }
}

/// A ConfigMap or a Secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum DataKind {
    /// `v1` `ConfigMap`.
    ConfigMap,
    /// `v1` `Secret`.
    Secret,
}

/// One requested change to a ConfigMap or Secret key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DataEdit {
    /// `set` or `remove`.
    pub op: String,
    /// The key.
    pub key: String,
    /// The value, for `set`; for a Secret, the plain value (Zeabur encodes it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// A [`Change::AddDomain`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct AddDomainChange {
    /// The workload.
    pub workload: WorkloadRef,
    /// The hostname.
    pub hostname: String,
    /// Where its certificate comes from; absent: as the route already does, else the chart's
    /// `ClusterIssuer`. With a source the hostname goes on an Ingress (see the `certificates`
    /// module).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate: Option<CertificateSource>,
}

/// A [`Change::Certificate`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct CertificateChange {
    /// The route; only an Ingress names its certificates.
    pub route: RouteRef,
    /// The hostname.
    pub hostname: String,
    /// Where its certificate comes from.
    pub certificate: CertificateSource,
}

/// Where the certificate of a hostname comes from.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "from", rename_all = "camelCase")]
pub enum CertificateSource {
    /// cert-manager issues and renews it through this `ClusterIssuer`.
    Issuer {
        /// The `ClusterIssuer`.
        name: String,
    },
    /// The person's own certificate, kept in a `kubernetes.io/tls` Secret. Nothing renews it.
    Own {
        /// The certificate, then its intermediates, PEM.
        certificate: String,
        /// Its private key, PEM. It is never shown again.
        key: String,
    },
}

impl std::fmt::Debug for CertificateSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Issuer { name } => f.debug_struct("Issuer").field("name", name).finish(),
            Self::Own { .. } => f.debug_struct("Own").finish_non_exhaustive(),
        }
    }
}

/// A route object: an Ingress or a Gateway API `HTTPRoute`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RouteRef {
    /// `Ingress` or `HTTPRoute`.
    pub kind: RouteKind,
    /// Its name.
    pub name: String,
}

/// The kinds of route object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum RouteKind {
    /// `networking.k8s.io/v1` `Ingress`.
    Ingress,
    /// `gateway.networking.k8s.io/v1` `HTTPRoute`.
    #[serde(rename = "HTTPRoute")]
    HttpRoute,
}

/// What a person asks to change, as one plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PlanRequest {
    /// The changes, applied in order.
    pub changes: Vec<Change>,
}

/// The object a planned write addresses.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", schema(as = PlanTarget))]
pub struct Target {
    /// `apiVersion`.
    pub api_version: String,
    /// The kind.
    pub kind: String,
    /// The resource (plural).
    pub plural: String,
    /// The name.
    pub name: String,
}

impl Target {
    /// The object `name` of the k8s-openapi type `K`.
    #[must_use]
    pub fn of<K: k8s_openapi::Resource>(name: &str) -> Self {
        Self {
            api_version: K::API_VERSION.to_owned(),
            kind: K::KIND.to_owned(),
            plural: K::URL_PATH_SEGMENT.to_owned(),
            name: name.to_owned(),
        }
    }

    /// The project's Namespace.
    #[must_use]
    pub fn namespace(name: &str) -> Self {
        Self::of::<k8s_openapi::api::core::v1::Namespace>(name)
    }

    /// Whether the object lives in the project's namespace: every object a plan writes does,
    /// except the Namespace itself.
    #[must_use]
    pub fn namespaced(&self) -> bool {
        !(self.api_version == "v1" && self.plural == "namespaces")
    }
}

/// How a planned write changes its object: a new object is created whole, an existing one is
/// patched or deleted. Creations and patches go as the field manager `zeabur`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", schema(as = PlannedWrite))]
pub enum Write {
    /// A new object, created with `POST`; it fails if one of that name exists by then.
    #[serde(rename_all = "camelCase")]
    Create {
        /// The object, `apiVersion`, `kind` and `metadata` included.
        #[cfg_attr(feature = "openapi", schema(value_type = Object))]
        body: Value,
    },
    /// An RFC 6902 JSON Patch whose first operation tests `metadata.generation` (or, for an
    /// object without one, `metadata.resourceVersion`), and whose
    /// `test` operations check every value it replaces or removes.
    #[serde(rename_all = "camelCase")]
    Patch {
        /// The operations.
        #[cfg_attr(feature = "openapi", schema(value_type = Vec<Object>))]
        operations: Vec<Value>,
    },
    /// A `DELETE` with background propagation, refused (`409`) unless the object still has this
    /// `uid` and `resourceVersion`.
    #[serde(rename_all = "camelCase")]
    Delete {
        /// The object's `metadata.uid`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        uid: Option<String>,
        /// The object's `metadata.resourceVersion`.
        resource_version: String,
    },
    /// An RFC 6902 JSON Patch of a Pod's `ephemeralcontainers` subresource that adds one
    /// container. Its first operation tests `metadata.uid`, so a pod replaced since the plan
    /// fails it.
    #[serde(rename_all = "camelCase")]
    EphemeralContainers {
        /// The operations.
        #[cfg_attr(feature = "openapi", schema(value_type = Vec<Object>))]
        operations: Vec<Value>,
    },
}

/// The managers that own a field a write touches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = PlanFieldOwners))]
pub struct FieldOwners {
    /// The field.
    pub path: String,
    /// Its managers, sorted.
    pub managers: Vec<String>,
}

/// One object a plan writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PlannedObject {
    /// The object.
    pub target: Target,
    /// The object as it is, without `managedFields` and with Secret values redacted; absent
    /// when the write creates it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    pub before: Option<Value>,
    /// The object as it will be, the same way; absent when the write deletes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    pub after: Option<Value>,
    /// `before` as YAML, written as the object's YAML view writes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_yaml: Option<String>,
    /// `after` as YAML, the same way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_yaml: Option<String>,
    /// The fields that change.
    pub changes: Vec<FieldChange>,
    /// Who owns the fields the write touches now.
    pub owners: Vec<FieldOwners>,
    /// The `resourceVersion` the plan was computed against; absent for a creation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_version: Option<String>,
    /// The write.
    pub write: Write,
    /// For an object a `create` makes, the index of that change in the request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<usize>,
    /// The object as it will be, unredacted: for re-projection only, never sent; `Null` when the
    /// write deletes it.
    #[serde(skip)]
    #[cfg_attr(feature = "openapi", schema(ignore))]
    pub raw_after: Value,
}

/// Something a person should know before applying.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "code", rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", schema(as = PlanWarning))]
pub enum Warning {
    /// These workloads read a changed ConfigMap or Secret into environment variables, which a
    /// running container reads only when it starts: restart them for the change to take effect.
    #[serde(rename_all = "camelCase")]
    RestartNeeded {
        /// The workloads.
        workloads: Vec<WorkloadRef>,
    },
    /// These workloads read a changed ConfigMap or Secret and ask Stakater Reloader to replace
    /// their pods when it changes, so Reloader restarts them after the apply.
    #[serde(rename_all = "camelCase")]
    ReloaderRestarts {
        /// The workloads.
        workloads: Vec<WorkloadRef>,
        /// Whether Reloader was seen running in the cluster; `false` when that could not be
        /// read with the person's rights.
        confirmed: bool,
    },
    /// A changed hostname was also listed for HTTPS; its certificate must be issued again for
    /// the new name.
    #[serde(rename_all = "camelCase")]
    TlsHostChanged {
        /// The route.
        route: String,
        /// The new hostname.
        hostname: String,
    },
    /// The new hostname is listed for HTTPS on an Ingress cert-manager watches: once applied,
    /// cert-manager asks this issuer for its certificate.
    #[serde(rename_all = "camelCase")]
    HttpsCertificate {
        /// The hostname.
        hostname: String,
        /// The `ClusterIssuer` or `Issuer` the Ingress names.
        issuer: String,
    },
    /// The hostname is served with the person's own certificate, which nothing renews.
    #[serde(rename_all = "camelCase")]
    OwnCertificate {
        /// The hostname.
        hostname: String,
        /// When the certificate expires, RFC 3339.
        not_after: String,
    },
    /// The issuer is set on the whole Ingress, so these other hostnames also get their
    /// certificates from it.
    #[serde(rename_all = "camelCase")]
    IssuerShared {
        /// The Ingress.
        route: String,
        /// The issuer.
        issuer: String,
        /// The other hostnames listed for HTTPS.
        hostnames: Vec<String>,
    },
    /// The new hostname is served over HTTP only.
    #[serde(rename_all = "camelCase")]
    HttpOnly {
        /// The hostname.
        hostname: String,
        /// Why.
        reason: HttpOnlyReason,
        /// The Ingress whose certificates come from elsewhere, for `certificatesElsewhere`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        route: Option<String>,
    },
    /// The new `HTTPRoute` attaches to this Gateway, the first of several.
    #[serde(rename_all = "camelCase")]
    GatewayChosen {
        /// Its namespace.
        namespace: String,
        /// Its name.
        name: String,
        /// How many Gateways there are.
        of: usize,
    },
    /// Gateways could not be listed, so the new route is an Ingress.
    GatewaysUnreadable,
    /// The Secret's values could not be read, so its diff shows only which keys change.
    #[serde(rename_all = "camelCase")]
    SecretValuesUnknown {
        /// The Secret.
        name: String,
    },
    /// A rollback returns the pod template only; the ConfigMaps and Secrets the workload reads
    /// keep their current contents.
    #[serde(rename_all = "camelCase")]
    ConfigNotRolledBack {
        /// The workload.
        workload: WorkloadRef,
    },
    /// The claim's `StorageClass` could not be read, so whether it can grow is known only when
    /// kube-apiserver answers.
    #[serde(rename_all = "camelCase")]
    StorageClassUnknown {
        /// The claim.
        claim: String,
    },
    /// A new `ReadWriteOnce` claim is mounted by a workload with several pods, which can share
    /// it only while they run on one node.
    #[serde(rename_all = "camelCase")]
    ReadWriteOnceShared {
        /// The workload.
        workload: WorkloadRef,
    },
    /// A new `ReadWriteOncePod` claim is mounted by a workload with several pods, of which only
    /// one at a time can use it.
    #[serde(rename_all = "camelCase")]
    ReadWriteOncePodShared {
        /// The workload.
        workload: WorkloadRef,
    },
    /// The deleted object owns these objects, as far as the person could read them; the garbage
    /// collector deletes them after it [k8s-gc].
    ///
    /// [k8s-gc]: https://kubernetes.io/docs/concepts/architecture/garbage-collection/
    #[serde(rename_all = "camelCase")]
    DeletesDependents {
        /// The deleted object.
        target: ObjectName,
        /// What goes with it, owners before what they own.
        dependents: Vec<ObjectName>,
    },
    /// The deleted Pod belongs to a controller, which starts a new one in its place.
    #[serde(rename_all = "camelCase")]
    PodReplaced {
        /// The Pod.
        pod: String,
        /// Its controller.
        owner: ObjectName,
    },
    /// Deleting the claim may lose its data: what happens to its PersistentVolume is the volume's
    /// reclaim policy, `Delete` (the volume and its data go) or `Retain` (they stay until an
    /// administrator removes them) [k8s-reclaim].
    ///
    /// [k8s-reclaim]: https://kubernetes.io/docs/concepts/storage/persistent-volumes/#reclaiming
    #[serde(rename_all = "camelCase")]
    ClaimDataMayBeLost {
        /// The claim.
        claim: String,
        /// Its PersistentVolume; absent while the claim is not bound.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        volume: Option<String>,
        /// The volume's `persistentVolumeReclaimPolicy`; absent when it could not be read.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reclaim_policy: Option<String>,
    },
    /// Deleting the service also deletes these objects: it made them and nothing else uses
    /// them.
    #[serde(rename_all = "camelCase")]
    DeletesOwned {
        /// The deleted workload.
        target: ObjectName,
        /// The objects, in the order they are deleted.
        objects: Vec<ObjectName>,
    },
    /// The deleted service uses or made this object, and it stays.
    #[serde(rename_all = "camelCase")]
    Stays {
        /// The object.
        object: ObjectName,
        /// Why.
        reason: KeptReason,
        /// The other workloads that use it, for `shared`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        workloads: Vec<WorkloadRef>,
        /// Its controller, for `controlled`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        owner: Option<ObjectName>,
        /// The tool that manages it, for `managed`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool: Option<zeabur_kernel::tool::Tool>,
    },
    /// These workloads still use the deleted ConfigMap, Secret or claim: their pods that start
    /// after it is gone fail to start, and a claim in use stays until no pod uses it
    /// [k8s-in-use].
    ///
    /// [k8s-in-use]: https://kubernetes.io/docs/concepts/storage/persistent-volumes/#storage-object-in-use-protection
    #[serde(rename_all = "camelCase")]
    StillUsed {
        /// The deleted object.
        target: ObjectName,
        /// The workloads.
        workloads: Vec<WorkloadRef>,
    },
    /// Deleting the NetworkPolicy opens traffic it held back: these services are no longer
    /// isolated, and these connections the project shows become allowed.
    #[serde(rename_all = "camelCase")]
    NetworkPolicyRemoved {
        /// The policy.
        policy: String,
        /// Services nothing limits reaching once it is gone.
        ingress_opened: Vec<WorkloadRef>,
        /// Services nothing limits sending once it is gone.
        egress_opened: Vec<WorkloadRef>,
        /// Connections it blocked or limited that become allowed.
        connections_opened: Vec<ConnectionRef>,
    },
    /// The plan adds this debug container to the pod. Kubernetes cannot remove an ephemeral
    /// container: it stays in the pod's spec, stopped once its shell exits, until the pod is
    /// replaced.
    #[serde(rename_all = "camelCase")]
    DebugContainerStays {
        /// The pod.
        pod: String,
        /// The debug container.
        container: String,
        /// The container whose processes it shares.
        target: String,
        /// Its image.
        image: String,
    },
    /// A debug container of Zeabur's still runs (or is starting) for this container; the
    /// terminal uses it, and nothing is written.
    #[serde(rename_all = "camelCase")]
    DebugContainerReused {
        /// The pod.
        pod: String,
        /// The debug container.
        container: String,
        /// The container whose processes it shares.
        target: String,
    },
    /// The person's credential lacks rights the plan needs: kube-apiserver will refuse it unless
    /// an administrator grants them (or the person acts as one).
    #[serde(rename_all = "camelCase")]
    RightsMissing {
        /// Each right, as kube-apiserver would name it in a refusal.
        rights: Vec<zeabur_kernel::denied::Denied>,
    },
    /// The plan creates a tools pod: what runs where, as whom, until when.
    #[serde(rename_all = "camelCase")]
    ToolsPod {
        /// Its namespace.
        namespace: String,
        /// Its name.
        pod: String,
        /// Its image.
        image: String,
        /// Who `kubectl` and `helm` act as: the person's user name.
        person: String,
        /// Whether they act with the administrator's credential, which the person holds now.
        admin: bool,
        /// When the credential written into the pod expires, RFC 3339; `kubectl` is refused
        /// from then until a terminal opens again with a fresh one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        credential_expires_at: Option<String>,
        /// The pod stops this long after it starts, whatever happens (`activeDeadlineSeconds`).
        deadline_seconds: i64,
        /// It is deleted this long after its last terminal closes.
        grace_seconds: i64,
    },
    /// The person's tools pod still runs; the terminal uses it, and nothing is written.
    #[serde(rename_all = "camelCase")]
    ToolsPodReused {
        /// Its namespace.
        namespace: String,
        /// Its name.
        pod: String,
    },
    /// The hostname moved to a route of its own: for its own certificate, or for a redirect or
    /// maintenance that would apply to every hostname of an `HTTPRoute`.
    #[serde(rename_all = "camelCase")]
    DomainMoved {
        /// The hostname.
        hostname: String,
        /// The route it leaves.
        from: ObjectName,
        /// Its new route.
        to: ObjectName,
    },
    /// The hostname was the route's last, so the route is deleted with it.
    #[serde(rename_all = "camelCase")]
    RouteDeleted {
        /// The route.
        route: ObjectName,
        /// The hostname.
        hostname: String,
    },
    /// The rules changed after maintenance started, so the hostname goes to this Service port
    /// instead of the backends kept then.
    #[serde(rename_all = "camelCase")]
    MaintenanceRestored {
        /// The hostname.
        hostname: String,
        /// The Service.
        service: String,
        /// Its port.
        port: i32,
    },
    /// Browsers keep a permanent redirect: after it stops, visitors who saw it still go to the
    /// other hostname until their browser forgets it.
    #[serde(rename_all = "camelCase")]
    PermanentRedirect {
        /// The hostname that redirects.
        hostname: String,
    },
    /// Anyone on the internet can connect to the port: no password of Zeabur's stands in front
    /// of it. For a data store, its own password is all that protects the data.
    #[serde(rename_all = "camelCase")]
    PublicPort {
        /// The workload.
        workload: WorkloadRef,
        /// The port.
        port: i32,
        /// What kind of data store the workload is, when it is one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data_store: Option<zeabur_model::DataStore>,
    },
    /// A NetworkPolicy isolates the workload's pods and admits nothing from the internet, so the
    /// new public port does not answer until a policy admits it (added by the API, which reads the
    /// project's policies).
    #[serde(rename_all = "camelCase")]
    PublicPortBlocked {
        /// The workload.
        workload: WorkloadRef,
        /// The port.
        port: i32,
        /// The policies that isolate its pods, by name.
        policies: Vec<String>,
    },
    /// The Ingress serves these hostnames over plain HTTP, so the password crosses the network
    /// readable on them.
    #[serde(rename_all = "camelCase")]
    PasswordOverHttp {
        /// The Ingress.
        route: String,
        /// Its hostnames without HTTPS.
        hostnames: Vec<String>,
    },
    /// The new task runs the service's image as it is now; a later change to the service's
    /// image does not reach the task.
    #[serde(rename_all = "camelCase")]
    TaskImageCopied {
        /// The service.
        from: WorkloadRef,
        /// The image.
        image: String,
    },
    /// Once applied, every workload of the project meets the restricted level, but the project
    /// enforces only this one: a `podSecurity` change to `restricted` would raise it.
    #[serde(rename_all = "camelCase")]
    ProjectCanBeRestricted {
        /// The level the project enforces now.
        level: PodSecurityLevel,
    },
}

/// An object of the project, by kind and name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ObjectName {
    /// The kind.
    pub kind: String,
    /// The name.
    pub name: String,
}

impl ObjectName {
    pub(crate) fn of(kind: &str, name: &str) -> Self {
        Self {
            kind: kind.to_owned(),
            name: name.to_owned(),
        }
    }
}

/// A connection between two services of the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ConnectionRef {
    /// The caller.
    pub from: WorkloadRef,
    /// The callee.
    pub to: WorkloadRef,
}

/// What a plan will do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// The objects it writes, in the order it writes them.
    pub objects: Vec<PlannedObject>,
    /// The workloads it affects, sorted.
    pub impact: Vec<WorkloadRef>,
    /// What to know before applying.
    pub warnings: Vec<Warning>,
}

/// Why an object a deleted service uses or made stays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum KeptReason {
    /// Other workloads use it too.
    Shared,
    /// It is a route that also leads to a Kubernetes Service that stays.
    RoutesElsewhere,
    /// Another controller owns it, such as a database operator its Secrets.
    Controlled,
    /// Helm, Argo CD or Flux manages it.
    Managed,
    /// The service uses it but did not make it: neither the service's label nor a name Zeabur
    /// gives it.
    NotMade,
}

/// Why a new hostname gets no HTTPS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum HttpOnlyReason {
    /// The chart names no `ClusterIssuer` (`domains.clusterIssuer`) and the Ingress no issuer.
    NoIssuer,
    /// The Ingress lists certificates cert-manager does not issue; the new hostname has to be
    /// added where they are made.
    CertificatesElsewhere,
    /// The hostname is a wildcard, and the issuer is not known to answer DNS-01 challenges,
    /// the only ACME challenge for a wildcard.
    WildcardNeedsDns01,
}
