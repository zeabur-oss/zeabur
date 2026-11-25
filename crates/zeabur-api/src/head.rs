//! `HEAD` on a page for administrators: whether the person may open it, without reading it.
//!
//! A `HEAD` request is a `GET` without the content, and the server should send the same header
//! fields and status [rfc9110-head]. So each such page lists the reads its `GET` makes, in a
//! function beside the handler, and both use that list. `HEAD` asks kube-apiserver, as the person,
//! whether any of the reads is allowed, with one `SelfSubjectAccessReview` per read, all at once
//! [k8s-ssar]. It answers `200` when one is, `403` when none is, and nothing else: no body, and
//! no object is read. `GET` makes the same review before it reads and answers `403` when none is
//! allowed; when some are, it answers `200` and names the refused reads as `unreadable`, as
//! before.
//!
//! `SelfSubjectRulesReview` is not used: it needs a namespace to answer for, and Kubernetes
//! documents its answer as possibly incomplete, since webhook authorizers cannot list their rules
//! [k8s-ssrr].
//!
//! A review that fails (kube-apiserver cannot be reached) is not a refusal: when none of the
//! reviews allows and one failed, `HEAD` answers `503`, so the interface asks again rather than
//! taking the person for refused.
//!
//! [rfc9110-head]: https://www.rfc-editor.org/rfc/rfc9110#section-9.3.2
//! [k8s-ssar]: https://kubernetes.io/docs/reference/access-authn-authz/authorization/#checking-api-access
//! [k8s-ssrr]: https://kubernetes.io/docs/reference/access-authn-authz/authorization/#checking-api-access

use axum::http::StatusCode;
use zeabur_core::problem::{Denied, Kind, Problem};
use zeabur_kube::Cluster;

use crate::error::{ApiError, review_refusal};

/// What the reviews of a page's reads came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Opening {
    /// At least one read is allowed.
    Open,
    /// Every read is refused.
    Closed,
    /// None is allowed, and at least one review failed, so the answer is not known.
    Unknown,
}

/// Asks `cluster` whether the person may make each of `reads`, all at once.
pub(crate) async fn review(cluster: &dyn Cluster, reads: &[Denied]) -> Opening {
    let answers =
        futures_util::future::join_all(reads.iter().map(|read| cluster.review_access(read))).await;
    if answers.iter().any(|a| matches!(a, Ok(true))) {
        Opening::Open
    } else if answers.iter().any(Result::is_err) {
        Opening::Unknown
    } else {
        Opening::Closed
    }
}

/// The status of a `HEAD` request for the page whose `GET` makes `reads`.
pub(crate) async fn status(cluster: &dyn Cluster, reads: &[Denied]) -> StatusCode {
    match review(cluster, reads).await {
        Opening::Open => StatusCode::OK,
        Opening::Closed => StatusCode::FORBIDDEN,
        Opening::Unknown => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// The start of a `GET`: an error when none of `reads` is allowed, naming the first as the one to
/// ask for, so `GET` and `HEAD` agree.
pub(crate) async fn require(
    cluster: &dyn Cluster,
    reads: &[Denied],
    what: &str,
) -> Result<(), ApiError> {
    match review(cluster, reads).await {
        Opening::Open => Ok(()),
        Opening::Closed => Err(reads.first().map_or_else(
            || ApiError::from(Problem::new(Kind::Forbidden, what)),
            |read| review_refusal(what, read.clone()),
        )),
        Opening::Unknown => Err(ApiError::from(Problem::new(
            Kind::Unavailable,
            format!("Zeabur could not check whether you may open {what}."),
        ))),
    }
}
