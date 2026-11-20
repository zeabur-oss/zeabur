//! A project's Namespace: whether it is adopted, its details, and whether the person may change
//! them.
//!
//! Adopting, letting go and the details are changes like any other (`Adopt`, `Unadopt` and
//! `Details`, planned and applied by the write endpoints). What this endpoint adds is a hint,
//! before anything is planned, of whether the person may patch the Namespace, so an interface
//! can show the details read-only and say which right to ask for instead of offering a change
//! that kube-apiserver would refuse.
//!
//! # Who may patch a Namespace
//!
//! kube-apiserver authorises a request for `/api/v1/namespaces/{name}` as one in namespace
//! `name` ([requestinfo]): RBAC rules from the ClusterRoleBindings and from the RoleBindings of
//! that namespace apply. One `SelfSubjectAccessReview` for `patch` on this Namespace answers it
//! [k8s-ssar]; when the review fails, the answer is `unknown` and the apply's dry run decides.
//!
//! # Read-only
//!
//! A project is read-only when the namespace's `SelfSubjectRulesReview` [k8s-ssar] has no write
//! verb on any resource. An incomplete review (another authorizer, such as a webhook, may allow
//! more) gives unknown.
//!
//! RBAC cannot limit a patch to some labels: a rule names verbs, resources and object names,
//! never fields [k8s-rbac-names]. So the right to adopt a namespace is also the right to change
//! its other labels, such as the Pod Security Admission levels `pod-security.kubernetes.io/*`
//! [k8s-psa-labels]; the interface says so when it asks for the right. Lowering the project's
//! level for a service that runs as root is such a change, so it needs the same right.
//!
//! With the member roles, Developers may patch their project's Namespace, and the chart's
//! admission policy (`deploy/chart/zeabur/templates/namespace-policy.yaml`) keeps such a patch to
//! the details unless the person may also [`MANAGE_VERB`] it. Adopting and letting go change a
//! label, so there they also need that verb.
//!
//! [requestinfo]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/endpoints/request/requestinfo.go
//! [k8s-ssar]: https://kubernetes.io/docs/reference/access-authn-authz/authorization/#checking-api-access
//! [k8s-rbac-names]: https://kubernetes.io/docs/reference/access-authn-authz/rbac/#referring-to-resources
//! [k8s-psa-labels]: https://kubernetes.io/docs/concepts/security/pod-security-admission/#pod-security-admission-labels-for-namespaces

mod deleting;

use axum::extract::State;
pub(crate) use deleting::held_objects;
pub use deleting::{HeldObject, ProjectDeleting, RemainingContent, RemainingFinalizer};
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::names::{LABEL_PROJECT, MANAGE_VERB, is_system_namespace};
use zeabur_core::problem::{Denied, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::{AccessRules, ClusterError, ResourceRef};
use zeabur_model::{Origin, PodSecurityLevel, ProjectBuildLimits, ProjectDetails};

use crate::account::{AccountRole, role};
use crate::error::{ApiError, get_read, read_problem};
use crate::extract::{Json, Path};
use crate::handlers::{AppState, Person};

/// Whether the person may change something, as one `SelfSubjectAccessReview` answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    /// kube-apiserver allows it.
    Allowed,
    /// kube-apiserver refuses it.
    Denied,
    /// The review failed: only trying tells.
    Unknown,
}

impl Permission {
    /// Allowed when both are, denied when either is.
    pub(crate) const fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Allowed, Self::Allowed) => Self::Allowed,
            (Self::Denied, _) | (_, Self::Denied) => Self::Denied,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
        }
    }
}

impl From<Result<bool, ClusterError>> for Permission {
    fn from(answer: Result<bool, ClusterError>) -> Self {
        match answer {
            Ok(true) => Self::Allowed,
            Ok(false) => Self::Denied,
            Err(_) => Self::Unknown,
        }
    }
}

/// A project's Namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectNamespace {
    /// The Namespace.
    pub origin: Origin,
    /// Its name.
    pub name: String,
    /// It carries Zeabur's project label.
    pub adopted: bool,
    /// Kubernetes keeps it for itself (`kube-` prefix); it cannot be adopted.
    pub system: bool,
    /// The name, description and icon given to it.
    #[serde(flatten)]
    pub details: ProjectDetails,
    /// The memory and CPU limits of one build, when the project sets them.
    #[serde(flatten)]
    pub build_limits: ProjectBuildLimits,
    /// The most memory a build limit may ask for, the largest allocatable memory of a node, in
    /// bytes; absent when the nodes could not be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_build_memory_bytes: Option<u64>,
    /// The most CPU a build limit may ask for, the largest allocatable CPU of a node, in
    /// millicores; absent when the nodes could not be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_build_cpu_millicores: Option<u64>,
    /// Whether the person may patch the Namespace: change its details.
    pub can_change: Permission,
    /// Whether the person may adopt it or let it go: patch it, and where the admission policy
    /// guards its labels, also `zeabur-manage` it. Changing its Pod Security level or its build
    /// limits needs the same.
    pub can_adopt: Permission,
    /// The Pod Security level enforced in it, its `pod-security.kubernetes.io/enforce` label;
    /// absent when the label is not set, and the cluster's default applies, or names no level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pod_security: Option<PodSecurityLevel>,
    /// Your role in the project, from your rights there; absent when they could not be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<AccountRole>,
    /// You may write nothing in the project: the namespace's rules review has no write verb.
    /// Absent when the review failed or is incomplete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    /// Whether you may read the pods' logs (`get` on `pods/log`), which a Viewer may not.
    pub can_read_logs: Permission,
    /// Whether you may read secret values (`get` on `secrets`), which a Viewer may not. A
    /// read-only project may still allow it.
    pub can_read_secrets: Permission,
    /// The chart's admission policy is installed: patching the Namespace without `zeabur-manage`
    /// changes only its details.
    pub labels_guarded: bool,
    /// The project is being deleted: what is left, and what holds it back. Absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleting: Option<ProjectDeleting>,
}

/// A project's Namespace: adopted or not, its details, and whether you may change them.
#[utoipa::path(get, path = "/api/v1/projects/{project}/namespace", tag = "projects",
    operation_id = "getProjectNamespace",
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = ProjectNamespace),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<ProjectNamespace>, ApiError> {
    let resource = ResourceRef::new("", "v1", "namespaces", "Namespace");
    let what = format!("namespace {project}");
    let (namespace, rules) = futures_util::future::join(
        cluster.get_object(&resource, None, &project),
        cluster.access_rules(&project),
    )
    .await;
    let namespace = namespace
        .map_err(|e| read_problem(e, &what, get_read("", "namespaces", None, &project)))?
        .ok_or_else(|| {
            ApiError::from(Problem::new(
                zeabur_core::problem::Kind::NotFound,
                format!("There is no {what}."),
            ))
        })?;
    let deleting = deleting::read(cluster.as_ref(), &project, &namespace).await;
    let capacity = super::writes::node_capacity(cluster.as_ref()).await;
    let metadata = namespace.metadata;
    let guarded = state.settings.identity.project_roles;
    let read_only = rules.as_ref().ok().and_then(AccessRules::read_only);
    let review = |verb: &str| Denied {
        verb: verb.to_owned(),
        group: String::new(),
        resource: "namespaces".to_owned(),
        name: Some(project.clone()),
        namespace: Some(project.clone()),
    };
    let patch = review("patch");
    let get = |resource: &str| Denied {
        verb: "get".to_owned(),
        group: String::new(),
        resource: resource.to_owned(),
        name: None,
        namespace: Some(project.clone()),
    };
    let (logs, secrets) = (get("pods/log"), get("secrets"));
    let (patch, logs, secrets, manage, role) = futures_util::join!(
        cluster.review_access(&patch),
        cluster.review_access(&logs),
        cluster.review_access(&secrets),
        async {
            if guarded {
                Permission::from(cluster.review_access(&review(MANAGE_VERB)).await)
            } else {
                Permission::Allowed
            }
        },
        role(cluster.as_ref(), &project, read_only),
    );
    let can_change = Permission::from(patch);
    let can_adopt = can_change.and(manage);
    Ok(Json(ProjectNamespace {
        origin: Origin::complete("v1", "Namespace", None, &project),
        adopted: metadata
            .labels
            .as_ref()
            .is_some_and(|labels| labels.contains_key(LABEL_PROJECT)),
        system: is_system_namespace(&project),
        details: ProjectDetails::from_annotations(metadata.annotations.as_ref()),
        build_limits: ProjectBuildLimits::from_annotations(metadata.annotations.as_ref()),
        max_build_memory_bytes: capacity.map(|c| c.memory_bytes),
        max_build_cpu_millicores: capacity.map(|c| c.cpu_millicores),
        can_change,
        can_adopt,
        pod_security: metadata
            .labels
            .as_ref()
            .and_then(|labels| labels.get(PodSecurityLevel::ENFORCE))
            .and_then(|level| PodSecurityLevel::parse(level)),
        role,
        read_only,
        can_read_logs: logs.into(),
        can_read_secrets: secrets.into(),
        labels_guarded: guarded,
        deleting,
        name: project,
    }))
}
