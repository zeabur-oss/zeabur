//! Falco: what it flagged at run time (a shell in a container, a sensitive file read), from the
//! policy reports falcosidekick writes [sidekick-policyreport].
//!
//! Falcosidekick appends each alert to the `PolicyReport` `falco-policy-report` of the namespace
//! named by its `k8s.ns.name` (or `ka.target.namespace`) field, and an alert without one to the
//! `ClusterPolicyReport` `falco-cluster-policy-report`. A report keeps its newest
//! `policyreport.maxevents` results (1000 by default), so these are recent alerts, not all of
//! them. Kubernetes' RBAC on `policyreports` decides who reads them; Zeabur reads as the person.
//!
//! A result's `timestamp` is the alert's time from falcosidekick 2.35.0 on; earlier versions
//! write only its seconds within the minute, so a time before 2001 is replaced by the `evt.time`
//! field when the rule outputs it.
//!
//! [sidekick-policyreport]: https://github.com/falcosecurity/falcosidekick/blob/2.35.0/outputs/policyreport.go

use k8s_openapi::jiff::Timestamp;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_kube::DynamicObject;
use zeabur_model::Origin;

use crate::{Module, Read};

/// The module.
pub static MODULE: Module = Module {
    id: "falco",
    capability: "runtime-security",
    reads: &[POLICY_REPORTS, CLUSTER_POLICY_REPORTS],
    http: &[],
};

/// `wgpolicyk8s.io/v1alpha2` `PolicyReport`, in the project's namespace.
pub const POLICY_REPORTS: Read = Read {
    group: "wgpolicyk8s.io",
    version: "v1alpha2",
    resource: "policyreports",
    kind: "PolicyReport",
};

/// `wgpolicyk8s.io/v1alpha2` `ClusterPolicyReport`: alerts without a namespace.
pub const CLUSTER_POLICY_REPORTS: Read = Read {
    group: "wgpolicyk8s.io",
    version: "v1alpha2",
    resource: "clusterpolicyreports",
    kind: "ClusterPolicyReport",
};

/// The `source` falcosidekick gives its results; other producers share the API.
const SOURCE: &str = "Falco";

/// Falco's priority as falcosidekick maps it to a report severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = FalcoSeverity)]
pub enum Severity {
    /// Debug or Informational.
    Info,
    /// Notice.
    Low,
    /// Warning.
    Medium,
    /// Error.
    High,
    /// Critical, Alert or Emergency.
    Critical,
}

impl Severity {
    fn parse(text: &str) -> Self {
        match text {
            "critical" => Self::Critical,
            "high" => Self::High,
            "medium" => Self::Medium,
            "low" => Self::Low,
            _ => Self::Info,
        }
    }
}

/// One alert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = FalcoAlert)]
pub struct Alert {
    /// The rule that fired (`Terminal shell in container`).
    pub rule: String,
    /// How serious Falco's rule says it is.
    pub severity: Severity,
    /// When it happened, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// The pod it concerns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pod: Option<String>,
    /// The container it concerns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// The command line that triggered it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// The user it ran as.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Falco's own sentence, with every field it names.
    pub output: String,
    /// Falco's event source (`syscall`, `k8s_audit`).
    pub source: String,
    /// The report result it was read from.
    pub origin: Origin,
}

/// Falco's alerts in `objects` (policy reports, namespaced or cluster-wide), newest first.
#[must_use]
pub fn alerts(objects: &[DynamicObject]) -> Vec<Alert> {
    let mut alerts: Vec<Alert> = objects
        .iter()
        .flat_map(|object| {
            let origin = Origin::complete(
                &format!("{}/{}", POLICY_REPORTS.group, POLICY_REPORTS.version),
                object
                    .types
                    .as_ref()
                    .map_or("PolicyReport", |t| t.kind.as_str()),
                object.metadata.namespace.as_deref(),
                object.metadata.name.as_deref().unwrap_or_default(),
            );
            object
                .data
                .get("results")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
                .filter(|(_, result)| result.get("source").and_then(Value::as_str) == Some(SOURCE))
                .map(move |(index, result)| {
                    alert(
                        result,
                        origin.clone().with_field(format!("results[{index}]")),
                    )
                })
        })
        .collect();
    alerts.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| a.rule.cmp(&b.rule)));
    alerts
}

fn alert(result: &Value, origin: Origin) -> Alert {
    let text = |key: &str| {
        result
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let property = |key: &str| {
        result
            .pointer(&format!("/properties/{}", key.replace('/', "~1")))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty() && *s != "<NA>" && *s != "<nil>")
            .map(str::to_owned)
    };
    let subject = result
        .get("resources")
        .and_then(Value::as_array)
        .and_then(|r| r.first())
        .filter(|r| r.get("kind").and_then(Value::as_str) == Some("Pod"))
        .and_then(|r| r.get("name").and_then(Value::as_str))
        .map(str::to_owned);
    Alert {
        rule: text("rule").unwrap_or_default(),
        severity: Severity::parse(&text("severity").unwrap_or_default()),
        time: time(result).or_else(|| {
            property("evt.time")
                .and_then(|ns| ns.parse::<i128>().ok())
                .and_then(|ns| Timestamp::from_nanosecond(ns).ok())
                .map(|t| t.to_string())
        }),
        pod: property("k8s.pod.name").or(subject),
        container: property("container.name"),
        command: property("proc.cmdline"),
        user: property("user.name"),
        output: text("message").unwrap_or_default(),
        source: text("policy").unwrap_or_default(),
        origin,
    }
}

/// The result's `timestamp`, unless it is too early to be an alert's (see the module docs).
fn time(result: &Value) -> Option<String> {
    const EARLIEST: i64 = 1_000_000_000;
    let seconds = result.pointer("/timestamp/seconds")?.as_i64()?;
    let nanos = result
        .pointer("/timestamp/nanos")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    (seconds >= EARLIEST)
        .then(|| Timestamp::new(seconds, i32::try_from(nanos).unwrap_or(0)).ok())
        .flatten()
        .map(|t| t.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn report(results: &Value) -> DynamicObject {
        serde_json::from_value(json!({
            "apiVersion": "wgpolicyk8s.io/v1alpha2", "kind": "PolicyReport",
            "metadata": {"name": "falco-policy-report", "namespace": "shop"},
            "results": results,
        }))
        .unwrap()
    }

    /// A result as falcosidekick 2.35.0 wrote it on the k3s test cluster.
    fn shadow_read() -> Value {
        json!({
            "category": "SI - System and Information Integrity",
            "message": "14:34:14.423441584: Warning Sensitive file opened for reading by non-trusted program | file=/etc/shadow",
            "policy": "syscall",
            "properties": {
                "container.name": "talker",
                "evt.time": "1792679654423441584",
                "k8s.ns.name": "shop",
                "k8s.pod.name": "talker-5896db5447-9fr5j",
                "proc.aname[2]": "<nil>",
                "proc.cmdline": "cat /etc/shadow",
                "user.name": "root"
            },
            "resources": [{"apiVersion": "v1", "kind": "Pod", "name": "talker-5896db5447-9fr5j", "namespace": "shop"}],
            "result": "warn",
            "rule": "Read sensitive file untrusted",
            "severity": "medium",
            "source": "Falco",
            "timestamp": {"nanos": 423_441_584, "seconds": 1_792_679_654_i64}
        })
    }

    #[test]
    fn a_result_gives_rule_severity_time_pod_container_and_command() {
        let alerts = alerts(&[report(&json!([shadow_read()]))]);
        let alert = &alerts[0];
        assert_eq!(alert.rule, "Read sensitive file untrusted");
        assert_eq!(alert.severity, Severity::Medium);
        assert_eq!(
            alert.time.as_deref(),
            Some("2026-10-22T14:34:14.423441584Z")
        );
        assert_eq!(alert.pod.as_deref(), Some("talker-5896db5447-9fr5j"));
        assert_eq!(alert.container.as_deref(), Some("talker"));
        assert_eq!(alert.command.as_deref(), Some("cat /etc/shadow"));
        assert_eq!(alert.user.as_deref(), Some("root"));
        assert_eq!(alert.source, "syscall");
        assert_eq!(alert.origin.namespace.as_deref(), Some("shop"));
        assert_eq!(alert.origin.field_path.as_deref(), Some("results[0]"));
    }

    #[test]
    fn a_seconds_within_the_minute_timestamp_gives_way_to_the_event_time() {
        let mut old = shadow_read();
        old["timestamp"] = json!({"seconds": 14, "nanos": 423_441_584});
        let alerts = alerts(&[report(&json!([old]))]);
        assert_eq!(
            alerts[0].time.as_deref(),
            Some("2026-10-22T14:34:14.423441584Z")
        );
        old["properties"]
            .as_object_mut()
            .unwrap()
            .remove("evt.time");
        assert_eq!(super::alerts(&[report(&json!([old]))])[0].time, None);
    }

    #[test]
    fn other_producers_results_are_left_out_and_the_newest_comes_first() {
        let mut later = shadow_read();
        later["rule"] = json!("Terminal shell in container");
        later["severity"] = json!("low");
        later["timestamp"]["seconds"] = json!(1_792_679_700_i64);
        let kyverno = json!({"policy": "require-labels", "rule": "check", "source": "kyverno", "result": "fail"});
        let alerts = alerts(&[report(&json!([shadow_read(), kyverno, later]))]);
        let rules: Vec<&str> = alerts.iter().map(|a| a.rule.as_str()).collect();
        assert_eq!(
            rules,
            [
                "Terminal shell in container",
                "Read sensitive file untrusted"
            ]
        );
        assert_eq!(alerts[0].severity, Severity::Low);
        assert_eq!(alerts[0].origin.field_path.as_deref(), Some("results[2]"));
    }

    #[test]
    fn missing_fields_are_absent_not_placeholders() {
        let alerts = alerts(&[report(&json!([{
            "rule": "Contact K8S API Server From Container", "source": "Falco", "severity": "high",
            "properties": {"container.name": "<NA>"}
        }]))]);
        assert_eq!(alerts[0].container, None);
        assert_eq!(alerts[0].pod, None);
        assert_eq!(alerts[0].severity, Severity::High);
    }
}
