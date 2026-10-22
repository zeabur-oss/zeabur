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
//! that namespace apply. One `SelfSubjectRulesReview` for the namespace returns exactly those
//! rules [k8s-ssrr], so `patch` on `namespaces` (all, or by `resourceNames` this one) answers
//! it. The review is a hint, as it says itself: when another authorizer (a webhook) may allow
//! more, it is incomplete and the answer is `unknown`; the apply's dry run decides.
//!
//! RBAC cannot limit a patch to some labels: a rule names verbs, resources and object names,
//! never fields [k8s-rbac-names]. So the right to adopt a namespace is also the right to change
//! its other labels, such as the Pod Security Admission levels `pod-security.kubernetes.io/*`
//! [k8s-psa-labels]; the interface says so when it asks for the right.
//!
//! With the member roles, Developers may patch their project's Namespace, and the chart's
//! admission policy (`deploy/chart/zeabur/templates/namespace-policy.yaml`) keeps such a patch to
//! the details unless the person may also [`MANAGE_VERB`] it. Adopting and letting go change a
//! label, so there they also need that verb.
//!
//! [requestinfo]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/endpoints/request/requestinfo.go
//! [k8s-ssrr]: https://kubernetes.io/docs/reference/access-authn-authz/authorization/#checking-api-access
//! [k8s-rbac-names]: https://kubernetes.io/docs/reference/access-authn-authz/rbac/#referring-to-resources
//! [k8s-psa-labels]: https://kubernetes.io/docs/concepts/security/pod-security-admission/#pod-security-admission-labels-for-namespaces

use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::names::{LABEL_PROJECT, MANAGE_VERB, is_system_namespace};
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::{Access, ResourceRef};
use zeabur_model::{Origin, ProjectDetails};

use crate::error::{ApiError, get_read, read_problem};
use crate::handlers::{AppState, Person};

/// Whether the person may change something, as far as their RBAC rules tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    /// A rule allows it.
    Allowed,
    /// No rule allows it, and the rules are complete.
    Denied,
    /// No rule allows it, but another authorizer may: only trying tells.
    Unknown,
}

impl Permission {
    /// Allowed when both are, denied when either is.
    const fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Allowed, Self::Allowed) => Self::Allowed,
            (Self::Denied, _) | (_, Self::Denied) => Self::Denied,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
        }
    }
}

impl From<Access> for Permission {
    fn from(access: Access) -> Self {
        match access {
            Access::Allowed => Self::Allowed,
            Access::Denied => Self::Denied,
            Access::Unknown => Self::Unknown,
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
    /// Whether the person may patch the Namespace: change its details.
    pub can_change: Permission,
    /// Whether the person may adopt it or let it go: patch it, and where the admission policy
    /// guards its labels, also `zeabur-manage` it.
    pub can_adopt: Permission,
    /// The chart's admission policy is installed: patching the Namespace without `zeabur-manage`
    /// changes only its details.
    pub labels_guarded: bool,
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
    let metadata = namespace.metadata;
    let guarded = state.settings.identity.project_roles;
    let (can_change, can_adopt) =
        rules.map_or((Permission::Unknown, Permission::Unknown), |rules| {
            let patch = Permission::from(rules.allows("patch", &resource, &project));
            let manage = if guarded {
                rules.allows(MANAGE_VERB, &resource, &project).into()
            } else {
                Permission::Allowed
            };
            (patch, patch.and(manage))
        });
    Ok(Json(ProjectNamespace {
        origin: Origin::complete("v1", "Namespace", None, &project),
        adopted: metadata
            .labels
            .as_ref()
            .is_some_and(|labels| labels.contains_key(LABEL_PROJECT)),
        system: is_system_namespace(&project),
        details: ProjectDetails::from_annotations(metadata.annotations.as_ref()),
        can_change,
        can_adopt,
        labels_guarded: guarded,
        name: project,
    }))
}
