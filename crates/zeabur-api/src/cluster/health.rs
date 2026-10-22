//! The API server's readiness, from `/readyz?verbose` [k8s-health], and how recently
//! kube-scheduler and kube-controller-manager renewed their leader leases.
//!
//! # What it covers
//!
//! `/readyz` is answered by the one kube-apiserver replica the request reached, from its own
//! checks: its connection to etcd, its informers and post-start hooks. It says nothing about the
//! other replicas, and nothing about kube-scheduler or kube-controller-manager, which managed
//! clusters run out of sight. So the answer is the API server's readiness, not the control
//! plane's. On k3s the scheduler and controller manager run in the same process as the API
//! server, so a process that answers has them running too; RKE2 runs them as static pods of their
//! own.
//!
//! The leader leases (`kube-system/kube-scheduler`, `kube-system/kube-controller-manager`) are
//! the one sign of the scheduler and controller manager a person can read with plain RBAC
//! (`get` on `leases.coordination.k8s.io` in `kube-system`). A lease not renewed within its
//! duration means no replica is leading. A single k3s server does not elect leaders, and managed
//! clusters may not keep these leases where a person can read them; then they are left out,
//! without a refusal to show.
//!
//! # Who can read it
//!
//! `/readyz` is in the default `system:public-info-viewer` ClusterRole, bound to every
//! authenticated person [k8s-default-roles], so the chart grants nothing for it. A cluster that
//! removed that binding answers `403`, told as the rule to ask for: `get` on the non-resource URL
//! `/readyz`.
//!
//! [k8s-health]: https://kubernetes.io/docs/reference/using-api/health-checks/
//! [k8s-default-roles]: https://kubernetes.io/docs/reference/access-authn-authz/rbac/#discovery-roles

use zeabur_core::problem::Kind;
use zeabur_kube::k8s_openapi::api::coordination::v1::Lease;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{ClusterError, ClusterExt as _, ResourceRef};
use zeabur_model::Origin;

use crate::error::ApiError;
use crate::views::{ClusterHealth, FailingCheck, LeaderLease};

/// The components whose leader lease is read, in the order shown.
const LEADERS: [&str; 2] = ["kube-scheduler", "kube-controller-manager"];

/// The checks of a verbose `/readyz` body: how many ran, and those that failed, in order.
pub(crate) fn parse(body: &str) -> (usize, Vec<FailingCheck>) {
    let mut checks = 0;
    let mut failing = Vec::new();
    for line in body.lines().map(str::trim) {
        if line.starts_with("[+]") {
            checks += 1;
        } else if let Some(rest) = line.strip_prefix("[-]") {
            checks += 1;
            let (name, reason) = match rest.split_once(' ') {
                Some((name, rest)) => {
                    let reason = rest.strip_prefix("failed:").unwrap_or(rest).trim();
                    (name, (!reason.is_empty()).then(|| reason.to_owned()))
                }
                None => (rest, None),
            };
            failing.push(FailingCheck {
                name: name.to_owned(),
                reason,
            });
        }
    }
    (checks, failing)
}

/// One lease as shown: who holds it, when it was renewed, and whether that is within its
/// duration at `now`.
fn leader(component: &str, lease: &Lease, now: Timestamp) -> LeaderLease {
    let spec = lease.spec.as_ref();
    let renewed = spec.and_then(|s| s.renew_time.as_ref()).map(|t| t.0);
    let duration = spec.and_then(|s| s.lease_duration_seconds);
    let current = match (renewed, duration) {
        (Some(renewed), Some(seconds)) => {
            now.as_second().saturating_sub(renewed.as_second()) <= i64::from(seconds)
        }
        _ => false,
    };
    LeaderLease {
        origin: Origin::complete(
            "coordination.k8s.io/v1",
            "Lease",
            Some("kube-system"),
            component,
        ),
        component: component.to_owned(),
        holder: spec
            .and_then(|s| s.holder_identity.clone())
            .filter(|h| !h.is_empty()),
        renewed: renewed.map(|t| t.to_string()),
        duration_seconds: duration,
        current,
    }
}

/// The leases that exist and may be read; any other answer leaves the lease out.
async fn leaders(cluster: &dyn zeabur_kube::Cluster, now: Timestamp) -> Vec<LeaderLease> {
    let resource = ResourceRef::of::<Lease>();
    let reads = LEADERS.map(|name| cluster.find_object(&resource, Some("kube-system"), name));
    let answers = futures_util::future::join_all(reads).await;
    LEADERS
        .iter()
        .zip(answers)
        .filter_map(|(component, answer)| {
            let object = answer.ok()??;
            let lease: Lease = serde_json::to_value(object)
                .and_then(serde_json::from_value)
                .ok()?;
            Some(leader(component, &lease, now))
        })
        .collect()
}

/// A refusal told as the rule to ask for; other problems as they are.
fn refused(error: ClusterError) -> ApiError {
    let what = "the API server's readiness";
    let mut problem = error.into_problem(what);
    if problem.kind == Kind::Forbidden {
        format!("You do not have permission to read {what}.").clone_into(&mut problem.detail);
        let rule = "`get` on the non-resource URL `/readyz`";
        problem.fix_for_occasion(
            format!(
                "Ask a cluster administrator for {rule}, for example with a ClusterRole granting \
                 it and a ClusterRoleBinding to you."
            ),
            zeabur_core::message::Message::new("problem.forbidden.fix.askForRule")
                .with("rule", rule),
        );
    }
    ApiError::from(problem)
}

/// The API server's readiness and the leader leases, read as the person.
pub(crate) async fn read(
    cluster: &dyn zeabur_kube::Cluster,
    now: Timestamp,
) -> Result<ClusterHealth, ApiError> {
    let (body, leaders) = futures_util::join!(cluster.readyz(), leaders(cluster, now));
    let (checks, failing) = parse(&body.map_err(refused)?);
    Ok(ClusterHealth {
        id: "cluster/health".to_owned(),
        ready: failing.is_empty(),
        checks,
        failing,
        leaders,
    })
}

#[cfg(test)]
mod tests {
    use zeabur_kube::k8s_openapi::api::coordination::v1::LeaseSpec;
    use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::MicroTime;

    use super::*;

    const ALL_OK: &str = "[+]ping ok\n[+]log ok\n[+]etcd ok\n[+]etcd-readiness ok\n\
        [+]informer-sync ok\n[+]poststarthook/start-apiserver-admission-initializer ok\n\
        [+]shutdown ok\nreadyz check passed\n";

    const ETCD_FAILING: &str = "[+]ping ok\n[+]log ok\n[-]etcd failed: reason withheld\n\
        [+]etcd-readiness ok\n[+]informer-sync ok\n\
        [-]poststarthook/crd-informer-synced failed: not finished\n[+]shutdown ok\n\
        readyz check failed\n";

    #[test]
    fn every_check_passing_names_none() {
        let (checks, failing) = parse(ALL_OK);
        assert_eq!(checks, 7);
        assert_eq!(failing, []);
    }

    #[test]
    fn failing_checks_are_named_with_their_reason() {
        let (checks, failing) = parse(ETCD_FAILING);
        assert_eq!(checks, 7);
        assert_eq!(
            failing,
            [
                FailingCheck {
                    name: "etcd".to_owned(),
                    reason: Some("reason withheld".to_owned()),
                },
                FailingCheck {
                    name: "poststarthook/crd-informer-synced".to_owned(),
                    reason: Some("not finished".to_owned()),
                },
            ]
        );
    }

    #[test]
    fn an_excluded_check_counts_as_passing() {
        let (checks, failing) = parse("[+]etcd excluded: ok\n[+]ping ok\nreadyz check passed\n");
        assert_eq!(checks, 2);
        assert_eq!(failing, []);
    }

    fn lease(renewed: &str, seconds: i32) -> Lease {
        Lease {
            spec: Some(LeaseSpec {
                holder_identity: Some("server-0_1f2e".to_owned()),
                lease_duration_seconds: Some(seconds),
                renew_time: Some(MicroTime(renewed.parse().unwrap())),
                ..LeaseSpec::default()
            }),
            ..Lease::default()
        }
    }

    #[test]
    fn a_lease_is_current_within_its_duration() {
        let now: Timestamp = "2026-10-22T10:00:30Z".parse().unwrap();
        let fresh = leader("kube-scheduler", &lease("2026-10-22T10:00:20Z", 15), now);
        assert!(fresh.current);
        assert_eq!(fresh.holder.as_deref(), Some("server-0_1f2e"));
        assert_eq!(fresh.renewed.as_deref(), Some("2026-10-22T10:00:20Z"));
        let stale = leader("kube-scheduler", &lease("2026-10-22T09:58:00Z", 15), now);
        assert!(!stale.current);
    }
}
