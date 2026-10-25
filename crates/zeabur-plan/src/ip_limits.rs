//! IP limits of a service: the addresses that may reach its domains, and the addresses its pods
//! may connect to.
//!
//! - **Incoming** is a Traefik `ipAllowList` middleware [traefik-ipallow] on the Ingress, named
//!   `<ingress>-ip-allow`. The Ingress lists it first and owns it. Traefik answers 403 to any other
//!   client address. Traefik reads the address from the TCP connection unless the middleware sets
//!   `ipStrategy`, so behind a load balancer or proxy the address is the proxy's: set `ipStrategy`
//!   in the middleware with `kubectl`. A plan that changes the addresses writes only `sourceRange`,
//!   so it keeps that setting. Gateway API has no such filter, so an `HTTPRoute` refuses the limit.
//! - **Outgoing** is a `NetworkPolicy` for `Egress` [k8s-netpol], named `<workload>-egress-limit`,
//!   that selects the pods by the labels of the workload's pod template. It allows the pods of the
//!   project, cluster DNS in `kube-system` (port 53, UDP and TCP) and the addresses given as
//!   `ipBlock`. A policy with `Egress` and no other rule blocks everything, so a limit without
//!   addresses leaves only the project and DNS. The policy is not owned by an object, has no
//!   finalizer, and `kubectl delete` removes the limit. The project's `zeabur-project` policy is
//!   not read or written. Only a network plugin that enforces `NetworkPolicy` applies the limit.
//!
//! [traefik-ipallow]: https://doc.traefik.io/traefik/reference/routing-configuration/http/middlewares/ipallowlist/
//! [k8s-netpol]: https://kubernetes.io/docs/concepts/services-networking/network-policies/

use std::collections::BTreeMap;
use std::net::IpAddr;

use serde_json::{Value, json};
use zeabur_model::WorkloadRef;
use zeabur_model::adapters::traefik;

use crate::access::{meta_of, middleware_resource, owner_of};
use crate::change::{Change, EgressLimitChange, RouteKind, RouteRef, Target};
use crate::checks::ResourceRef;
use crate::json::{add, replace, test};
use crate::plan::{ErrorKind, PlanError, Planner, error, ingress_target};

const MANAGED_BY: &str = "app.kubernetes.io/managed-by";

fn policy_resource() -> ResourceRef {
    ResourceRef {
        group: "networking.k8s.io".to_owned(),
        version: "v1".to_owned(),
        resource: "networkpolicies".to_owned(),
    }
}

fn policy_target(name: &str) -> Target {
    Target {
        api_version: "networking.k8s.io/v1".to_owned(),
        kind: "NetworkPolicy".to_owned(),
        plural: "networkpolicies".to_owned(),
        name: name.to_owned(),
    }
}

/// The `NetworkPolicy` that limits where the pods of workload `workload` connect.
#[must_use]
pub fn egress_limit_name(workload: &str) -> String {
    format!("{workload}-egress-limit")
}

/// The egress policy of workload `workload` as read, which deleting the workload with what it
/// made also deletes.
pub(crate) fn owned_policy(planner: &Planner<'_>, workload: &str) -> Option<String> {
    let name = egress_limit_name(workload);
    planner
        .input
        .objects
        .iter()
        .any(|f| f.resource == policy_resource() && f.name == name)
        .then_some(name)
}

/// The Traefik `Middleware` a change reads first, if any.
pub(crate) fn middleware_read(change: &Change) -> Option<(ResourceRef, String)> {
    let Change::IpAllow(allow) = change else {
        return None;
    };
    (allow.route.kind == RouteKind::Ingress).then(|| {
        (
            middleware_resource(),
            traefik::ip_allow_name(&allow.route.name),
        )
    })
}

/// The `NetworkPolicy` a change reads first, if any.
pub(crate) fn policy_read(change: &Change) -> Option<(ResourceRef, String)> {
    let name = if let Change::EgressLimit(limit) = change {
        &limit.workload.name
    } else if let Change::Delete {
        resource,
        name,
        owned: true,
        ..
    } = change
        && crate::owned::workload_of(resource, name).is_some()
    {
        name
    } else {
        return None;
    };
    Some((policy_resource(), egress_limit_name(name)))
}

/// Each address of `given` as an IP address or a CIDR range, lower case, once. With `ranges`, an
/// address alone becomes a range of one address, as `ipBlock` asks.
fn checked(given: &[String], ranges: bool) -> Result<Vec<String>, PlanError> {
    let mut out: Vec<String> = Vec::new();
    for entry in given {
        let entry = entry.trim().to_ascii_lowercase();
        let (address, prefix) = match entry.split_once('/') {
            Some((address, prefix)) => (address, Some(prefix)),
            None => (entry.as_str(), None),
        };
        let invalid = || {
            let mut refused = error(
                ErrorKind::BadRequest,
                "invalid_address",
                format!("{entry:?} is not an IP address or a CIDR range."),
            );
            refused.field = Some("cidrs".to_owned());
            refused
        };
        let ip: IpAddr = address.parse().ok().ok_or_else(invalid)?;
        let longest = if ip.is_ipv4() { 32 } else { 128 };
        let bits = match prefix {
            Some(p) if p.bytes().all(|b| b.is_ascii_digit()) && !p.is_empty() => {
                p.parse::<u8>().ok().ok_or_else(invalid)?
            }
            Some(_) => return Err(invalid()),
            None => longest,
        };
        if bits > longest {
            return Err(invalid());
        }
        let text = if prefix.is_some() || ranges {
            format!("{ip}/{bits}")
        } else {
            ip.to_string()
        };
        if !out.contains(&text) {
            out.push(text);
        }
    }
    Ok(out)
}

/// The `spec` of the egress policy for pods with `labels`.
fn egress_spec(labels: &BTreeMap<String, String>, cidrs: &[String]) -> Value {
    let mut egress = vec![
        json!({"to": [{"podSelector": {}}]}),
        json!({
            "to": [{
                "namespaceSelector": {"matchLabels": {"kubernetes.io/metadata.name": "kube-system"}},
                "podSelector": {"matchLabels": {"k8s-app": "kube-dns"}},
            }],
            "ports": [{"protocol": "UDP", "port": 53}, {"protocol": "TCP", "port": 53}],
        }),
    ];
    if !cidrs.is_empty() {
        let to: Vec<Value> = cidrs
            .iter()
            .map(|c| json!({"ipBlock": {"cidr": c}}))
            .collect();
        egress.push(json!({ "to": to }));
    }
    json!({
        "podSelector": {"matchLabels": labels},
        "policyTypes": ["Egress"],
        "egress": egress,
    })
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl Planner<'_> {
    pub(crate) fn ip_allow(
        &mut self,
        route: &RouteRef,
        enabled: bool,
        cidrs: &[String],
    ) -> Result<(), PlanError> {
        if route.kind == RouteKind::HttpRoute {
            return Err(error(
                ErrorKind::Invalid,
                "ip_allow_needs_ingress",
                "Gateway API has no filter for client addresses; limit a domain on an Ingress.",
            ));
        }
        let ranges = if enabled {
            checked(cidrs, false)?
        } else {
            Vec::new()
        };
        if enabled && ranges.is_empty() {
            let mut refused = error(
                ErrorKind::BadRequest,
                "no_addresses",
                "Give at least one address; a limit with none would block every client.",
            );
            refused.field = Some("cidrs".to_owned());
            return Err(refused);
        }
        let object = self.access_route(route, "")?;
        let ingress = route.name.as_str();
        let name = traefik::ip_allow_name(ingress);
        let meta = meta_of(&object);
        let listed = traefik::middlewares(&meta).contains(&name);
        let middleware = self.fetched_middleware(&name);
        if !enabled {
            let index = self.working(ingress_target(ingress), object);
            if !listed && middleware.is_none() {
                return Err(error(
                    ErrorKind::NotFound,
                    "no_ip_allow",
                    format!("Ingress {ingress} limits no client addresses."),
                ));
            }
            if listed {
                self.unlist(index, &name)?;
            }
            if middleware.is_some() {
                self.delete(&middleware_resource(), &name, None)?;
            }
            return Ok(());
        }
        if let Some(found) = middleware {
            let spec = if found.pointer("/spec/ipAllowList/sourceRange").is_some() {
                let before = found.pointer("/spec/ipAllowList/sourceRange").cloned();
                vec![
                    test("/spec/ipAllowList/sourceRange", before.unwrap_or_default()),
                    replace("/spec/ipAllowList/sourceRange", json!(ranges)),
                ]
            } else if found.pointer("/spec/ipAllowList").is_some() {
                vec![add("/spec/ipAllowList/sourceRange", json!(ranges))]
            } else {
                vec![add(
                    "/spec",
                    json!({"ipAllowList": {"sourceRange": ranges}}),
                )]
            };
            let at = self.working(crate::access::middleware_target(&name), found);
            self.patch(at, spec)?;
        } else {
            self.created_middleware(
                &name,
                owner_of(&meta).as_ref(),
                &json!({"ipAllowList": {"sourceRange": ranges}}),
            );
        }
        if !listed {
            let index = self.working(ingress_target(ingress), object);
            self.list(index, &name, true)?;
        }
        Ok(())
    }

    pub(crate) fn egress_limit(&mut self, change: &EgressLimitChange) -> Result<(), PlanError> {
        let reference: &WorkloadRef = &change.workload;
        let found = self.workload(reference)?;
        Self::refuse_managed(found.meta(), reference.kind.as_str(), &reference.name)?;
        let name = egress_limit_name(&reference.name);
        let existing = self
            .input
            .objects
            .iter()
            .find(|f| f.resource == policy_resource() && f.name == name)
            .map(|f| f.object.clone());
        if let Some(object) = &existing {
            Self::refuse_managed(&meta_of(object), "NetworkPolicy", &name)?;
        }
        self.impact.insert(reference.clone());
        if !change.enabled {
            if existing.is_none() {
                return Err(error(
                    ErrorKind::NotFound,
                    "no_egress_limit",
                    format!(
                        "{} {} has no limit on where it connects.",
                        reference.kind.as_str(),
                        reference.name
                    ),
                ));
            }
            return self.delete(&policy_resource(), &name, None);
        }
        let cidrs = checked(&change.cidrs, true)?;
        let labels: BTreeMap<String, String> = found
            .pod_template()
            .and_then(|t| t.metadata.as_ref())
            .and_then(|m| m.labels.clone())
            .unwrap_or_default();
        if labels.is_empty() {
            return Err(error(
                ErrorKind::Invalid,
                "no_pod_labels",
                format!(
                    "The pods of {} {} have no labels, so a policy could not select them alone.",
                    reference.kind.as_str(),
                    reference.name
                ),
            ));
        }
        let spec = egress_spec(&labels, &cidrs);
        if let Some(object) = existing {
            let before = object.get("spec").cloned().unwrap_or_default();
            let index = self.working(policy_target(&name), object);
            return self.patch(index, vec![test("/spec", before), replace("/spec", spec)]);
        }
        let body = json!({
            "apiVersion": "networking.k8s.io/v1",
            "kind": "NetworkPolicy",
            "metadata": {
                "name": name,
                "namespace": self.input.snapshot.namespace,
                "labels": {MANAGED_BY: "zeabur"},
            },
            "spec": spec,
        });
        self.created(policy_target(&name), body);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn given(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn an_address_is_kept_alone_for_traefik_and_made_a_range_for_a_policy() {
        let list = given(&["203.0.113.7", " 2001:DB8::/32 ", "203.0.113.7"]);
        assert_eq!(
            checked(&list, false).unwrap(),
            ["203.0.113.7", "2001:db8::/32"]
        );
        assert_eq!(
            checked(&list, true).unwrap(),
            ["203.0.113.7/32", "2001:db8::/32"]
        );
    }

    #[test]
    fn what_is_no_address_is_refused() {
        for bad in [
            "example.com",
            "10.0.0.0/33",
            "::1/129",
            "10.0.0.0/",
            "10.0.0.0/-1",
            "",
        ] {
            assert!(checked(&given(&[bad]), false).is_err(), "{bad}");
        }
    }
}
