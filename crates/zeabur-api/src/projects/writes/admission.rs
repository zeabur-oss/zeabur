//! Refusals from admission, told apart from refusals for missing rights.
//!
//! kube-apiserver answers `403 Forbidden` both when its authorizers refuse a request and when an
//! admission plugin refuses it after they allowed it: a `ResourceQuota` [k8s-quota], a
//! `LimitRange` [k8s-limitrange], Pod Security admission, a webhook. Only the first is about the
//! person's rights; admin rights do not lift a quota. So on a `403` to a write, Zeabur asks one
//! `SelfSubjectAccessReview` for the same request, with the same credential
//! ([`zeabur_kube::Cluster::review_access`]): allowed means admission refused it.
//!
//! - **`quota-exceeded`.** The quota plugin names the quota it applied ("exceeded quota: {name},
//!   requested: …, used: …, limited: …" [k8s-quota-admission]); the numbers in `why` are read from
//!   that quota's `status.hard` and `status.used`, not from the message.
//! - **`policy-denied`.** Any other admission refusal, with the policy's own words in `why`. A
//!   webhook that denies without a status code of `400` or more is answered `400` with "admission
//!   webhook "{name}" denied the request: …" [k8s-webhook-errors]; that is a policy's refusal too,
//!   not a malformed request.
//!
//! Neither kind is a refusal for missing rights, so neither is offered as `elevation-required`
//! ([`crate::error`]).
//!
//! [k8s-quota]: https://kubernetes.io/docs/concepts/policy/resource-quotas/
//! [k8s-limitrange]: https://kubernetes.io/docs/concepts/policy/limit-range/
//! [k8s-quota-admission]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/admission/plugin/resourcequota/controller.go
//! [k8s-webhook-errors]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/admission/plugin/webhook/errors/statuserror.go

use zeabur_core::message::Message;
use zeabur_core::origin::Origin;
use zeabur_core::problem::{Kind, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::ResourceQuota;
use zeabur_kube::{Cluster, ClusterExt as _};

/// What the quota admission plugin writes before the quota's name.
const QUOTA_EXCEEDED: &str = "exceeded quota: ";

/// What a webhook's denial starts with, before the webhook's name.
const WEBHOOK_DENIED: &str = "admission webhook \"";

/// `problem`, a refused write of `what` in `namespace`, as `quota-exceeded` or `policy-denied`
/// when admission refused it; otherwise unchanged.
pub(crate) async fn refine(
    cluster: &dyn Cluster,
    namespace: &str,
    what: &str,
    problem: Problem,
) -> Problem {
    let Some(message) = problem.kubernetes.as_ref().map(|k| k.message.clone()) else {
        return problem;
    };
    match problem.kind {
        Kind::BadRequest if message.starts_with(WEBHOOK_DENIED) => {
            policy_denied(problem, what, &message)
        }
        Kind::Forbidden => {
            let Some(denied) = &problem.denied else {
                return problem;
            };
            if !matches!(cluster.review_access(denied).await, Ok(true)) {
                return problem;
            }
            match quota_name(&message) {
                Some(quota) => {
                    quota_exceeded(cluster, namespace, what, problem, quota, &message).await
                }
                None => policy_denied(problem, what, &message),
            }
        }
        Kind::BadRequest
        | Kind::Unauthenticated
        | Kind::SignedOut
        | Kind::ElevationRequired
        | Kind::QuotaExceeded
        | Kind::PolicyDenied
        | Kind::NotFound
        | Kind::LogUnavailable
        | Kind::Conflict
        | Kind::Managed
        | Kind::PlanExpired
        | Kind::SystemNamespace
        | Kind::Invalid
        | Kind::TooManyRequests
        | Kind::Unavailable
        | Kind::Internal => problem,
    }
}

/// The quota named in a quota admission refusal.
fn quota_name(message: &str) -> Option<&str> {
    let (_, rest) = message.split_once(QUOTA_EXCEEDED)?;
    let name = rest.split(',').next()?.trim();
    (!name.is_empty()).then_some(name)
}

/// The resources a quota admission refusal says were requested (`requests.cpu`), in order.
fn requested(message: &str) -> Vec<&str> {
    message
        .split_once("requested: ")
        .and_then(|(_, rest)| rest.split(", used:").next())
        .into_iter()
        .flat_map(|list| list.split(','))
        .filter_map(|item| item.split('=').next())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect()
}

/// The resource of `quota` a refusal is about: the first one requested that the quota limits,
/// else the first one it limits that is used up.
fn exceeded<'q>(quota: &'q ResourceQuota, message: &str) -> Option<(&'q str, String, String)> {
    let status = quota.status.as_ref()?;
    let hard = status.hard.as_ref()?;
    let used = status.used.as_ref();
    let used_of = |name: &str| used.and_then(|u| u.get(name)).map(|q| q.0.clone());
    let pick = requested(message)
        .into_iter()
        .find_map(|name| hard.get_key_value(name))
        .or_else(|| {
            hard.iter().find(|(name, limit)| {
                // Millicores read every quantity in the same unit: `4`, `3500m`, `8Gi`.
                let milli = zeabur_core::quantity::millicores;
                used_of(name)
                    .and_then(|used| Some(milli(&used)? >= milli(&limit.0)?))
                    .unwrap_or(false)
            })
        })?;
    let (name, limit) = pick;
    Some((
        name.as_str(),
        limit.0.clone(),
        used_of(name).unwrap_or_default(),
    ))
}

async fn quota_exceeded(
    cluster: &dyn Cluster,
    namespace: &str,
    what: &str,
    problem: Problem,
    quota: &str,
    message: &str,
) -> Problem {
    let mut why = Message::new("problem.why.quota-exceeded").with("quota", quota);
    let mut evidence = Vec::new();
    if let Ok(quotas) = cluster.list::<ResourceQuota>(Some(namespace)).await
        && let Some(found) = quotas
            .iter()
            .find(|q| q.metadata.name.as_deref() == Some(quota))
        && let Some((resource, hard, used)) = exceeded(found, message)
    {
        let field = |part: &str| {
            Origin::complete("v1", "ResourceQuota", Some(namespace), quota)
                .with_field(format!("status.{part}.{resource}"))
        };
        evidence.push(field("hard").with_value(&hard));
        evidence.push(field("used").with_value(&used));
        why = why
            .with("resource", resource)
            .with("hard", hard)
            .with("used", used);
    }
    if evidence.is_empty() {
        evidence.push(Origin::complete(
            "v1",
            "ResourceQuota",
            Some(namespace),
            quota,
        ));
    }
    let name = problem.summary.params.get("name").cloned();
    let mut refined = problem.refined(Kind::QuotaExceeded);
    refined.detail = format!("The change to {what} would take the project over its limit {quota}.");
    refined.why = Some(why);
    let mut key = Message::new("problem.quotaExceeded.fix.scale").with("quota", quota);
    if let Some(name) = name {
        key = key.with("object", name);
    }
    refined.fix_for_occasion(
        format!(
            "Ask for less, free some of what the project uses, or ask a cluster administrator to \
             raise the limit {quota}."
        ),
        key,
    );
    refined.evidence.extend(evidence);
    refined
}

/// The policy's own words: a webhook's reason after its name, else the whole message.
fn policy_words(message: &str) -> &str {
    message
        .split_once("denied the request: ")
        .map_or(message, |(_, words)| words)
        .trim()
}

/// The webhook a denial names.
fn webhook(message: &str) -> Option<&str> {
    let (_, rest) = message.split_once(WEBHOOK_DENIED)?;
    rest.split_once('"').map(|(name, _)| name)
}

fn policy_denied(problem: Problem, what: &str, message: &str) -> Problem {
    let words = policy_words(message);
    let mut why = Message::new("problem.why.policy-denied").with("message", words);
    if let Some(name) = webhook(message) {
        why = why.with("policy", name);
    }
    let mut refined = problem.refined(Kind::PolicyDenied);
    refined.detail = format!("A policy in the cluster refused the change to {what}: {words}");
    refined.why = Some(why);
    refined.fix_for_occasion(
        "Change what the policy names, or ask whoever runs the policy.",
        Message::new("problem.policyDenied.fix.change"),
    );
    refined
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUOTA: &str = "deployments.apps \"web\" is forbidden: exceeded quota: compute, \
                         requested: requests.cpu=1,requests.memory=1Gi, used: requests.cpu=3500m, \
                         limited: requests.cpu=4";

    #[test]
    fn a_quota_refusal_names_its_quota_and_what_was_requested() {
        assert_eq!(quota_name(QUOTA), Some("compute"));
        assert_eq!(requested(QUOTA), ["requests.cpu", "requests.memory"]);
        assert_eq!(
            quota_name("pods \"web\" is forbidden: User cannot create"),
            None
        );
    }

    #[test]
    fn a_webhooks_denial_is_quoted_in_its_own_words() {
        let message = "admission webhook \"deny.example.com\" denied the request: images must \
                       come from registry.example.com";
        assert_eq!(webhook(message), Some("deny.example.com"));
        assert_eq!(
            policy_words(message),
            "images must come from registry.example.com"
        );
        let limit =
            "pods \"web-1\" is forbidden: maximum cpu usage per Container is 1, but limit is 2";
        assert_eq!(webhook(limit), None);
        assert_eq!(policy_words(limit), limit);
    }
}
