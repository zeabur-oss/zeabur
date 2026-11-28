//! How much a planned change can hurt, which decides how a person confirms it.
//!
//! Every write is planned and applied the same way; only the confirmation changes with the risk.
//! A change that is easy to take back and keeps the service up is confirmed lightly. A change
//! that loses data, takes the service down or cannot be reversed is confirmed in full, with its
//! consequence first. Confirmations stay rare and specific so they keep their weight
//! [nng-confirmation].
//!
//! Rules, per change (a plan of several changes takes the highest; no change is normal):
//! - low: restarting a workload with at least two ready pods (Kubernetes replaces them one by one
//!   and the rest keep serving); scaling to one pod or more; adopting a namespace, creating a
//!   project, its details and build limits, a rebuild and an upload token.
//! - high, with the reason the confirmation states: a restart with fewer than two ready pods;
//!   scaling to zero; removing a ConfigMap or Secret key; a rollback; any storage change (a claim
//!   cannot shrink, a new or moved mount replaces the pods); a YAML edit, written as typed (a
//!   form's spec-only edit is not); removing a domain; a NetworkPolicy that isolates its pods or an
//!   egress limit; opening a port to the internet; deleting an object or a project.
//! - normal: everything else.
//!
//! [nng-confirmation]: https://www.nngroup.com/articles/confirmation-dialog/

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use zeabur_kube::k8s_openapi::api::networking::v1::NetworkPolicy;
use zeabur_model::WorkloadRef;
use zeabur_plan::Change;

/// How much a change can hurt.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum RiskLevel {
    /// Easy to take back, and the service stays up.
    Low,
    /// Neither low nor high.
    Normal,
    /// It loses data, takes the service down or cannot be reversed.
    High,
}

/// Why a change is high risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum RiskReason {
    /// A restart of a workload with fewer than two ready pods: it stops serving for a moment.
    FewPods,
    /// Scaling to zero stops the workload.
    ScaleToZero,
    /// It removes ConfigMap or Secret keys.
    DeletesKeys,
    /// A rollback.
    Rollback,
    /// A storage change: a claim cannot shrink, and a new or moved mount replaces the pods.
    Storage,
    /// A YAML edit, written as typed.
    YamlEdit,
    /// It removes a domain.
    RemovesDomain,
    /// A NetworkPolicy or egress limit that shuts pods off.
    Isolates,
    /// It opens a port to the internet.
    OpensPort,
    /// It deletes an object or a project, which cannot be taken back.
    Deletes,
}

/// One reason of a high risk, with the workloads it names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RiskCause {
    /// The reason.
    pub reason: RiskReason,
    /// For `fewPods`, the restarted workloads with fewer than two ready pods; for
    /// `scaleToZero`, the stopped workloads; empty otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workloads: Vec<WorkloadRef>,
}

/// The risk of a plan's changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Risk {
    /// The highest of the changes.
    pub level: RiskLevel,
    /// The reasons of the high-risk changes, each once, in the order of the changes.
    pub reasons: Vec<RiskCause>,
}

/// The ready pods of the workloads a plan restarts, as the plan read them.
pub(crate) type ReadyPods = BTreeMap<WorkloadRef, i32>;

/// The risk of one change.
struct Weight {
    level: RiskLevel,
    reason: Option<RiskReason>,
    workload: Option<WorkloadRef>,
}

const fn weight(level: RiskLevel) -> Weight {
    Weight {
        level,
        reason: None,
        workload: None,
    }
}

const fn high(reason: RiskReason) -> Weight {
    Weight {
        level: RiskLevel::High,
        reason: Some(reason),
        workload: None,
    }
}

fn high_for(reason: RiskReason, workload: &WorkloadRef) -> Weight {
    Weight {
        workload: Some(workload.clone()),
        ..high(reason)
    }
}

/// The risk of `changes`; see the module documentation.
pub(crate) fn weigh(changes: &[Change], ready: &ReadyPods) -> Risk {
    let weights: Vec<Weight> = changes.iter().map(|c| weigh_one(c, ready)).collect();
    let level = weights
        .iter()
        .map(|w| w.level)
        .max()
        .unwrap_or(RiskLevel::Normal);
    let mut reasons: Vec<RiskCause> = Vec::new();
    for one in &weights {
        let Some(reason) = one.reason else { continue };
        let found = reasons.iter().position(|c| c.reason == reason);
        let at = found.unwrap_or_else(|| {
            reasons.push(RiskCause {
                reason,
                workloads: Vec::new(),
            });
            reasons.len() - 1
        });
        if let (Some(cause), Some(workload)) = (reasons.get_mut(at), &one.workload) {
            cause.workloads.push(workload.clone());
        }
    }
    Risk { level, reasons }
}

fn weigh_one(change: &Change, ready: &ReadyPods) -> Weight {
    match change {
        Change::Restart { workload } => {
            if ready.get(workload).copied().unwrap_or(0) >= 2 {
                weight(RiskLevel::Low)
            } else {
                high_for(RiskReason::FewPods, workload)
            }
        }
        Change::Scale {
            workload,
            replicas: 0,
        } => high_for(RiskReason::ScaleToZero, workload),
        Change::Scale { .. }
        | Change::Adopt
        | Change::CreateProject { .. }
        | Change::Details { .. }
        | Change::BuildLimits { .. }
        | Change::Rebuild { .. }
        | Change::UploadToken { .. } => weight(RiskLevel::Low),
        Change::Data { changes, .. } if changes.iter().any(|edit| edit.op == "remove") => {
            high(RiskReason::DeletesKeys)
        }
        Change::Create { object, .. } if isolates(object) => high(RiskReason::Isolates),
        Change::EgressLimit(limit) if limit.enabled => high(RiskReason::Isolates),
        Change::PublicPort(port) if !port.remove => high(RiskReason::OpensPort),
        Change::Rollback { .. } => high(RiskReason::Rollback),
        Change::ExpandVolume { .. }
        | Change::AddVolume { .. }
        | Change::EditMount { .. }
        | Change::RemoveVolume { .. }
        | Change::SwapClaim { .. } => high(RiskReason::Storage),
        Change::Edit {
            spec_only: false, ..
        } => high(RiskReason::YamlEdit),
        Change::RemoveDomain { .. } => high(RiskReason::RemovesDomain),
        Change::Delete { .. }
        | Change::RemoveBuild { .. }
        | Change::DeleteProject
        | Change::ForceDeleteProject => high(RiskReason::Deletes),
        Change::Env { .. }
        | Change::Data { .. }
        | Change::AddDomain(_)
        | Change::RenameDomain { .. }
        | Change::Redirect(_)
        | Change::BasicAuth(_)
        | Change::IpAllow(_)
        | Change::EgressLimit(_)
        | Change::ErrorPageHtml(_)
        | Change::PageServer(_)
        | Change::Template(_)
        | Change::ErrorPages(_)
        | Change::Maintenance(_)
        | Change::Certificate(_)
        | Change::EnvImport(_)
        | Change::PublicPort(_)
        | Change::LabelVolume(_)
        | Change::Probe { .. }
        | Change::Port { .. }
        | Change::ConfigReload { .. }
        | Change::Image { .. }
        | Change::ArchiveDigest { .. }
        | Change::BuildOptions(_)
        | Change::AutoUpdate { .. }
        | Change::Group { .. }
        | Change::Resources { .. }
        | Change::SecurityProfile { .. }
        | Change::Edit { .. }
        | Change::Create { .. }
        | Change::CreateBuiltService(_)
        | Change::CreateImageService(_)
        | Change::SetBuild(_)
        | Change::CopyS3Keys(_)
        | Change::Unadopt
        | Change::ScheduleTask { .. }
        | Change::RunTask { .. }
        | Change::EditTask { .. }
        | Change::DebugContainer { .. }
        | Change::ClusterTools => weight(RiskLevel::Normal),
    }
}

/// Whether `object` is a NetworkPolicy that shuts its pods off: it limits what they send
/// (`Egress`), or it covers ingress and lets nothing in. A policy that only admits more is not:
/// Kubernetes adds the rules of every policy that selects a pod [k8s-network-policies].
///
/// [k8s-network-policies]: https://kubernetes.io/docs/concepts/services-networking/network-policies/
fn isolates(object: &serde_json::Value) -> bool {
    if object.get("kind").and_then(serde_json::Value::as_str) != Some("NetworkPolicy") {
        return false;
    }
    let Ok(policy) = serde_json::from_value::<NetworkPolicy>(object.clone()) else {
        return false;
    };
    let spec = policy.spec.unwrap_or_default();
    let types = spec.policy_types.as_deref();
    // Without policyTypes a policy covers ingress, and egress when it has egress rules.
    let egress = types.map_or_else(
        || spec.egress.is_some(),
        |t| t.iter().any(|t| t == "Egress"),
    );
    let ingress = types.is_none_or(|t| t.iter().any(|t| t == "Ingress"));
    egress || (ingress && spec.ingress.unwrap_or_default().is_empty())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zeabur_model::WorkloadKind;

    use super::*;

    fn api() -> WorkloadRef {
        WorkloadRef {
            kind: WorkloadKind::Deployment,
            name: "api".to_owned(),
        }
    }

    fn change(value: serde_json::Value) -> Change {
        serde_json::from_value(value).unwrap()
    }

    fn workload() -> serde_json::Value {
        json!({"kind": "Deployment", "name": "api"})
    }

    fn reasons(risk: &Risk) -> Vec<RiskReason> {
        risk.reasons.iter().map(|c| c.reason).collect()
    }

    #[test]
    fn a_restart_is_low_with_two_ready_pods_and_high_with_fewer() {
        let restart = [change(json!({"op": "restart", "workload": workload()}))];
        let two = ReadyPods::from([(api(), 2)]);
        assert_eq!(weigh(&restart, &two).level, RiskLevel::Low);
        let one = weigh(&restart, &ReadyPods::from([(api(), 1)]));
        assert_eq!(one.level, RiskLevel::High);
        assert_eq!(
            one.reasons,
            [RiskCause {
                reason: RiskReason::FewPods,
                workloads: vec![api()],
            }]
        );
        // Pods the plan did not read count as none.
        assert_eq!(weigh(&restart, &ReadyPods::new()).level, RiskLevel::High);
    }

    #[test]
    fn scaling_to_zero_is_high_and_names_the_workload() {
        let stop = weigh(
            &[change(
                json!({"op": "scale", "workload": workload(), "replicas": 0}),
            )],
            &ReadyPods::new(),
        );
        assert_eq!(stop.level, RiskLevel::High);
        assert_eq!(stop.reasons[0].workloads, [api()]);
        let up = weigh(
            &[change(
                json!({"op": "scale", "workload": workload(), "replicas": 3}),
            )],
            &ReadyPods::new(),
        );
        assert_eq!(up.level, RiskLevel::Low);
    }

    #[test]
    fn a_plan_takes_its_highest_change_and_each_reason_once_in_change_order() {
        let changes = [
            change(json!({"op": "env", "workload": workload(), "changes": []})),
            change(json!({"op": "rollback", "workload": workload(), "revision": 1})),
            change(json!({"op": "removeDomain",
                "route": {"kind": "Ingress", "name": "web"}, "hostname": "a.test"})),
            change(json!({"op": "rollback", "workload": workload(), "revision": 2})),
        ];
        let risk = weigh(&changes, &ReadyPods::new());
        assert_eq!(risk.level, RiskLevel::High);
        assert_eq!(
            reasons(&risk),
            [RiskReason::Rollback, RiskReason::RemovesDomain]
        );
        assert_eq!(weigh(&[], &ReadyPods::new()).level, RiskLevel::Normal);
        assert_eq!(
            weigh(&changes[..1], &ReadyPods::new()).level,
            RiskLevel::Normal
        );
    }

    #[test]
    fn removing_a_key_is_high_and_setting_one_is_normal() {
        let data = |op: &str| {
            change(json!({"op": "data", "kind": "ConfigMap", "name": "app",
                "changes": [{"op": op, "key": "A", "value": "1"}]}))
        };
        assert_eq!(
            reasons(&weigh(&[data("remove")], &ReadyPods::new())),
            [RiskReason::DeletesKeys]
        );
        assert_eq!(
            weigh(&[data("set")], &ReadyPods::new()).level,
            RiskLevel::Normal
        );
    }

    #[test]
    fn a_yaml_edit_is_high_and_a_forms_spec_edit_is_not() {
        let edit = |spec_only: bool| {
            change(json!({"op": "edit",
                "resource": {"group": "apps", "version": "v1", "resource": "deployments"},
                "name": "api", "yaml": "{}", "specOnly": spec_only}))
        };
        assert_eq!(
            reasons(&weigh(&[edit(false)], &ReadyPods::new())),
            [RiskReason::YamlEdit]
        );
        assert_eq!(
            weigh(&[edit(true)], &ReadyPods::new()).level,
            RiskLevel::Normal
        );
    }

    #[test]
    fn a_policy_isolates_when_it_limits_egress_or_lets_nothing_in() {
        let policy = |spec: serde_json::Value| {
            json!({"apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
                "metadata": {"name": "p"}, "spec": spec})
        };
        let selector = json!({"matchLabels": {"app": "api"}});
        assert!(isolates(&policy(json!({"podSelector": selector}))));
        assert!(isolates(&policy(
            json!({"podSelector": selector, "egress": [{}]})
        )));
        assert!(isolates(&policy(json!({"podSelector": selector,
            "policyTypes": ["Egress"], "egress": [{"to": []}]}))));
        // An allow rule only admits more.
        assert!(!isolates(&policy(json!({"podSelector": selector,
            "ingress": [{"from": [{"podSelector": {}}]}]}))));
        assert!(!isolates(
            &json!({"kind": "ConfigMap", "metadata": {"name": "p"}})
        ));
        let create =
            change(json!({"op": "create", "object": policy(json!({"podSelector": selector}))}));
        assert_eq!(
            reasons(&weigh(&[create], &ReadyPods::new())),
            [RiskReason::Isolates]
        );
    }

    #[test]
    fn opening_a_port_and_limiting_egress_are_high_and_their_inverses_normal() {
        let port = |remove: bool| {
            change(
                json!({"op": "publicPort", "workload": workload(), "service": "api",
                "port": 5432, "remove": remove}),
            )
        };
        assert_eq!(
            reasons(&weigh(&[port(false)], &ReadyPods::new())),
            [RiskReason::OpensPort]
        );
        assert_eq!(
            weigh(&[port(true)], &ReadyPods::new()).level,
            RiskLevel::Normal
        );
        let egress = |enabled: bool| {
            change(
                json!({"op": "egressLimit", "workload": workload(), "enabled": enabled,
                "cidrs": []}),
            )
        };
        assert_eq!(
            reasons(&weigh(&[egress(true)], &ReadyPods::new())),
            [RiskReason::Isolates]
        );
        assert_eq!(
            weigh(&[egress(false)], &ReadyPods::new()).level,
            RiskLevel::Normal
        );
    }

    #[test]
    fn deletions_storage_and_project_changes_weigh_as_the_rules_say() {
        let delete = change(json!({"op": "delete",
            "resource": {"group": "", "version": "v1", "resource": "configmaps"}, "name": "a"}));
        assert_eq!(
            reasons(&weigh(&[delete], &ReadyPods::new())),
            [RiskReason::Deletes]
        );
        let expand = change(json!({"op": "expandVolume", "claim": "data", "size": "2Gi"}));
        assert_eq!(
            reasons(&weigh(&[expand], &ReadyPods::new())),
            [RiskReason::Storage]
        );
        assert_eq!(
            weigh(&[change(json!({"op": "adopt"}))], &ReadyPods::new()).level,
            RiskLevel::Low
        );
        assert_eq!(
            weigh(&[change(json!({"op": "unadopt"}))], &ReadyPods::new()).level,
            RiskLevel::Normal
        );
        assert_eq!(
            reasons(&weigh(
                &[change(json!({"op": "deleteProject"}))],
                &ReadyPods::new()
            )),
            [RiskReason::Deletes]
        );
    }
}
