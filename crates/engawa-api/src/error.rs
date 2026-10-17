//! Problems as HTTP responses.
//!
//! **Refusals offer elevation.** Where elevation is offered and not in use
//! ([`Elevation::Available`]), every refusal from kube-apiserver (a `403` that carries its
//! answer in `kubernetes`) leaves as `elevation-required`: the same status, `denied` and
//! `kubernetes`, so the interface can offer "Retry as admin" next to any of them. This happens
//! once, in [`offer_elevation_on_refusal`] around every route, and for the terminal, whose
//! refusal travels in a frame after the upgrade, in [`offer_elevation`] itself. Engawa's own
//! `403`s (a handshake from another origin, a plan made by someone else) carry no answer from
//! the cluster and stay `forbidden`: admin rights would not change them. Neither does a refusal
//! while the rights are in use, nor where they are not offered (local mode).

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use engawa_core::problem::{Denied, Kind, MEDIA_TYPE, Problem};
use engawa_kube::ClusterError;

use crate::access::Elevation;
use crate::handlers::AppState;

/// A [`Problem`] as an `application/problem+json` response.
///
/// When kube-apiserver asked clients to wait (`429` or `503` with `retryAfterSeconds`), the
/// response carries the same `Retry-After` (see [`Problem::retry_after`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError(pub Box<Problem>);

impl From<Problem> for ApiError {
    fn from(problem: Problem) -> Self {
        Self(Box::new(problem))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status =
            StatusCode::from_u16(self.0.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let retry_after = self.0.retry_after();
        let refusal = is_cluster_refusal(&self.0).then(|| Refusal(self.0.clone()));
        let mut response = (status, Json(*self.0)).into_response();
        let headers = response.headers_mut();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(MEDIA_TYPE));
        if let Some(seconds) = retry_after {
            headers.insert(header::RETRY_AFTER, HeaderValue::from(seconds));
        }
        if let Some(refusal) = refusal {
            response.extensions_mut().insert(refusal);
        }
        response
    }
}

/// Whether kube-apiserver refused the request: a `forbidden` that carries its answer.
fn is_cluster_refusal(problem: &Problem) -> bool {
    problem.kind == Kind::Forbidden && problem.kubernetes.is_some()
}

/// A refusal from kube-apiserver, kept on its response for [`offer_elevation_on_refusal`].
#[derive(Clone)]
struct Refusal(Box<Problem>);

/// `problem`, as `elevation-required` when kube-apiserver refused it and the person could retry
/// as an administrator; its detail, kube-apiserver's answer and the refused request (`denied`)
/// stay as they are (see [`Problem::refined`]).
pub(crate) fn offer_elevation(problem: Problem, elevation: Elevation) -> Problem {
    if is_cluster_refusal(&problem) && elevation == Elevation::Available {
        let mut offered = problem.refined(Kind::ElevationRequired);
        offered.fix_key = Some(engawa_core::message::Message::new(
            "problem.forbidden.fix.askOrRetry",
        ));
        offered
    } else {
        problem
    }
}

/// [`offer_elevation`] on every answer of the API.
pub(crate) async fn offer_elevation_on_refusal(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let (parts, body) = request.into_parts();
    let elevation = state
        .access
        .for_request(&parts)
        .map(|access| access.elevation);
    let mut response = next.run(Request::from_parts(parts, body)).await;
    match (elevation, response.extensions_mut().remove::<Refusal>()) {
        (Ok(Elevation::Available), Some(Refusal(problem))) => {
            ApiError::from(offer_elevation(*problem, Elevation::Available)).into_response()
        }
        (
            Ok(Elevation::Available | Elevation::Unavailable | Elevation::Active { .. }) | Err(_),
            _,
        ) => response,
    }
}

/// The problem for a read of `what` that failed; on a `403` it also names the read that was
/// refused (`denied`), as a refused write does.
///
/// `read` is the request as Engawa made it, built with [`list_read`] or [`get_read`]:
/// kube-apiserver's answer does not name the verb, and its message is not an interface (see
/// [`Denied`]).
pub(crate) fn read_problem(error: ClusterError, what: &str, read: Denied) -> ApiError {
    let mut problem = error.into_problem(what);
    if problem.kind == Kind::Forbidden {
        problem.deny(read);
    }
    ApiError::from(problem)
}

/// A `list` of `resource` in `group` (empty for the core group), in `namespace` or, when that is
/// `None`, across the cluster.
pub(crate) fn list_read(group: &str, resource: &str, namespace: Option<&str>) -> Denied {
    Denied {
        verb: "list".to_owned(),
        group: group.to_owned(),
        resource: resource.to_owned(),
        name: None,
        namespace: namespace.map(str::to_owned),
    }
}

/// A `get` of the object `name`; a subresource is part of `resource` (`pods/log`), as RBAC rules
/// and `kubectl auth can-i` write it [k8s-subresource].
///
/// [k8s-subresource]: https://kubernetes.io/docs/reference/access-authn-authz/rbac/#referring-to-resources
pub(crate) fn get_read(group: &str, resource: &str, namespace: Option<&str>, name: &str) -> Denied {
    Denied {
        verb: "get".to_owned(),
        name: Some(name.to_owned()),
        ..list_read(group, resource, namespace)
    }
}

/// The read refused when a project's objects are refused as a whole: `engawa-kube` answers with
/// kube-apiserver's first refusal, and that is always the first kind it reads, Deployments, since
/// the whole is refused only when every kind is.
pub(crate) fn project_read(project: &str) -> Denied {
    list_read("apps", "deployments", Some(project))
}

/// The cluster-wide `list` an unreadable kind names, from its key (`deployments.apps`, `nodes`).
pub(crate) fn list_read_of_key(key: &str) -> Denied {
    let (resource, group) = key.split_once('.').unwrap_or((key, ""));
    list_read(group, resource, None)
}

#[cfg(test)]
mod tests {
    use engawa_core::problem::{Denied, Kind, KubernetesStatus};
    use engawa_kube::ClusterError;

    use super::{get_read, list_read, list_read_of_key, read_problem};

    fn forbidden() -> ClusterError {
        ClusterError::Status(KubernetesStatus::new(403, "Forbidden", "no"))
    }

    fn denied(read: Denied) -> Option<Denied> {
        read_problem(forbidden(), "it", read).0.denied
    }

    #[test]
    fn a_403_on_a_list_in_a_namespace_names_the_list() {
        assert_eq!(
            denied(list_read("", "configmaps", Some("shop"))),
            Some(Denied {
                verb: "list".to_owned(),
                group: String::new(),
                resource: "configmaps".to_owned(),
                name: None,
                namespace: Some("shop".to_owned()),
            })
        );
    }

    #[test]
    fn a_403_on_one_object_names_it() {
        assert_eq!(
            denied(get_read("", "pods/log", Some("shop"), "web-1")),
            Some(Denied {
                verb: "get".to_owned(),
                group: String::new(),
                resource: "pods/log".to_owned(),
                name: Some("web-1".to_owned()),
                namespace: Some("shop".to_owned()),
            })
        );
    }

    #[test]
    fn a_403_on_a_cluster_scoped_list_names_no_namespace() {
        let expected = Some(Denied {
            verb: "list".to_owned(),
            group: "apiextensions.k8s.io".to_owned(),
            resource: "customresourcedefinitions".to_owned(),
            name: None,
            namespace: None,
        });
        assert_eq!(
            denied(list_read(
                "apiextensions.k8s.io",
                "customresourcedefinitions",
                None
            )),
            expected
        );
        assert_eq!(
            denied(list_read_of_key(
                "customresourcedefinitions.apiextensions.k8s.io"
            )),
            expected
        );
        assert_eq!(
            denied(list_read_of_key("nodes")).map(|d| (d.group, d.resource)),
            Some((String::new(), "nodes".to_owned()))
        );
    }

    #[test]
    fn other_failures_name_no_read() {
        for error in [
            ClusterError::Status(KubernetesStatus::new(404, "NotFound", "gone")),
            ClusterError::Transport("reset".to_owned()),
        ] {
            let problem = read_problem(error, "it", list_read("", "pods", None)).0;
            assert_ne!(problem.kind, Kind::Forbidden);
            assert_eq!(problem.denied, None);
        }
    }
}
