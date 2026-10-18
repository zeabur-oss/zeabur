//! A Prometheus-compatible metrics store: what a service used over time.
//!
//! History comes from the Prometheus HTTP API's range queries [prom-api]. Prometheus and
//! VictoriaMetrics serve the same API [vm-single]; engawa's recommendation installs
//! VictoriaMetrics.
//!
//! # Fixed queries
//!
//! The queries are engawa's, never a person's: one project's namespace and one service's pods,
//! by name. They read the series the kubelet's cAdvisor endpoint exports, which every
//! Prometheus-compatible stack scrapes ([kps-values], [vmks-values]), so they need no recording
//! rule:
//!
//! - CPU: `rate` of `container_cpu_usage_seconds_total`, summed per pod, in millicores.
//! - Memory: the highest `container_memory_working_set_bytes` of each step (`max_over_time`),
//!   summed per pod, so a climb just before an out-of-memory kill is not averaged away.
//! - Restarts (Extended): `increase` of kube-state-metrics'
//!   `kube_pod_container_status_restarts_total`, and whether the last termination was `OOMKilled`
//!   (`kube_pod_container_status_last_terminated_reason`) [ksm-pod]. Without kube-state-metrics
//!   there are none of these series, and the answer says restarts are unknown, not zero.
//!
//! Series without a `container` (the pod's cgroup) or an `image` (the pause container) are left
//! out, as the kubernetes-mixin rules leave them out [mixin-rules].
//!
//! # A service's pods
//!
//! Pods are matched by the names their controllers give them, so pods a rollout replaced still
//! count: `<name>-<template hash>-<5>` for a Deployment, `<name>-<ordinal>` for a StatefulSet,
//! `<name>-<5>` for a DaemonSet or a Job, `<name>-<schedule>-<5>` for a CronJob's Jobs
//! [k8s-generate-name]. The 5 characters are random, from lower-case letters and digits; a
//! second service whose name is this one's plus such a suffix would match too.
//!
//! # Ranges
//!
//! Four ranges, each with a step that gives 240 to 360 points per series, far under
//! Prometheus' 11,000 [prom-api]: 1 hour at 15 s, 6 hours at 1 min, 24 hours at 5 min, 7 days at
//! 30 min.
//!
//! [prom-api]: https://prometheus.io/docs/prometheus/latest/querying/api/
//! [vm-single]: https://docs.victoriametrics.com/victoriametrics/single-server-victoriametrics/
//! [kps-values]: https://github.com/prometheus-community/helm-charts/blob/main/charts/kube-prometheus-stack/values.yaml
//! [vmks-values]: https://github.com/VictoriaMetrics/helm-charts/blob/master/charts/victoria-metrics-k8s-stack/values.yaml
//! [ksm-pod]: https://github.com/kubernetes/kube-state-metrics/blob/main/docs/metrics/workload/pod-metrics.md
//! [mixin-rules]: https://github.com/kubernetes-monitoring/kubernetes-mixin/blob/master/rules/apps.libsonnet
//! [k8s-generate-name]: https://kubernetes.io/docs/reference/using-api/api-concepts/#generated-values

use std::collections::BTreeMap;

use serde::de::value::{self, StrDeserializer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::{HttpApi, Module};

/// The module.
pub static MODULE: Module = Module {
    id: "prometheus-api",
    capability: "metrics",
    reads: &[],
    http: &[PROMETHEUS_HTTP],
};

/// The Prometheus HTTP API, version 1. `/api/v1/status/buildinfo` answers on Prometheus and on
/// VictoriaMetrics, without a query, so it tells whether a store is there.
pub const PROMETHEUS_HTTP: HttpApi = HttpApi {
    name: "prometheus-http/v1",
    probe: "/api/v1/status/buildinfo",
};

/// The path of a range query.
pub const QUERY_RANGE: &str = "/api/v1/query_range";

// The API's `range` parameter and the `service_history` MCP tool's `range` argument, by their
// serde names. The doc comments are the schemas' descriptions.
/// How far back a history reaches.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum PrometheusRange {
    /// The last hour, a point every 15 s.
    #[default]
    #[serde(rename = "1h")]
    Hour,
    /// The last 6 hours, a point a minute.
    #[serde(rename = "6h")]
    SixHours,
    /// The last 24 hours, a point every 5 min.
    #[serde(rename = "24h")]
    Day,
    /// The last 7 days, a point every 30 min.
    #[serde(rename = "7d")]
    Week,
}

impl PrometheusRange {
    /// The range its name names: `1h`, `6h`, `24h` or `7d`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::deserialize(StrDeserializer::<value::Error>::new(text)).ok()
    }

    /// Its name, as [`PrometheusRange::parse`] reads it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hour => "1h",
            Self::SixHours => "6h",
            Self::Day => "24h",
            Self::Week => "7d",
        }
    }

    /// Its length in seconds.
    #[must_use]
    pub const fn seconds(self) -> i64 {
        match self {
            Self::Hour => 3_600,
            Self::SixHours => 21_600,
            Self::Day => 86_400,
            Self::Week => 604_800,
        }
    }

    /// The seconds between two points.
    #[must_use]
    pub const fn step(self) -> i64 {
        match self {
            Self::Hour => 15,
            Self::SixHours => 60,
            Self::Day => 300,
            Self::Week => 1_800,
        }
    }

    /// The times of its points, ending at `now` rounded down to a step, oldest first.
    #[must_use]
    pub fn times(self, now: i64) -> Vec<i64> {
        let step = self.step();
        let end = now.div_euclid(step).saturating_mul(step);
        let start = end.saturating_sub(self.seconds());
        (0..=self.seconds() / step)
            .map(|i| start.saturating_add(i.saturating_mul(step)))
            .collect()
    }
}

/// The regular expression that matches the names of the pods a workload of `kind` named `name`
/// creates, or `None` for another kind or a name that is not a Kubernetes name. PromQL anchors it
/// at both ends.
#[must_use]
pub fn pod_pattern(kind: &str, name: &str) -> Option<String> {
    let plain = !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.');
    if !plain {
        return None;
    }
    // `.` is the one character of a name that means something in a regular expression.
    let name = name.replace('.', "[.]");
    let suffix = match kind.to_ascii_lowercase().as_str() {
        "deployment" => "-[a-z0-9]{1,10}-[a-z0-9]{5}",
        "statefulset" => "-[0-9]+",
        "daemonset" | "job" => "-[a-z0-9]{5}",
        "cronjob" => "-[0-9]+-[a-z0-9]{5}",
        _ => return None,
    };
    Some(format!("{name}{suffix}"))
}

/// One range query: its PromQL, and what the series are converted with.
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    /// The PromQL.
    pub promql: String,
    /// What each value is multiplied by (1,000 for cores to millicores).
    pub scale: f64,
}

/// The queries of one history.
#[derive(Debug, Clone, PartialEq)]
pub struct Queries {
    /// CPU per pod, in millicores.
    pub cpu: Query,
    /// Memory per pod, the working set's highest in each step, in bytes.
    pub memory: Query,
    /// Restarts per pod during each step.
    pub restarts: Query,
    /// 1 where a pod's container last ended `OOMKilled`.
    pub oom_killed: Query,
}

/// The queries for the pods matching `pods` (from [`pod_pattern`]) in `namespace`, at `range`'s
/// step. `namespace` must be a Kubernetes name (a DNS label), so it needs no quoting.
#[must_use]
pub fn queries(namespace: &str, pods: &str, range: PrometheusRange) -> Queries {
    let step = range.step();
    // `rate` needs two samples in its window; a store scraping every 30 s has them in 2 minutes.
    let rate_window = step.max(120);
    // At least one sample in each window, also at a 15 s step.
    let sample_window = step.max(60);
    let containers = format!(
        "namespace=\"{namespace}\",pod=~\"{pods}\",container!=\"\",container!=\"POD\",image!=\"\""
    );
    let pods_only = format!("namespace=\"{namespace}\",pod=~\"{pods}\"");
    Queries {
        cpu: Query {
            promql: format!(
                "sum by (pod) (rate(container_cpu_usage_seconds_total{{{containers}}}[{rate_window}s]))"
            ),
            scale: 1_000.0,
        },
        memory: Query {
            promql: format!(
                "sum by (pod) (max_over_time(container_memory_working_set_bytes{{{containers}}}[{sample_window}s]))"
            ),
            scale: 1.0,
        },
        restarts: Query {
            promql: format!(
                "sum by (pod) (increase(kube_pod_container_status_restarts_total{{{pods_only}}}[{sample_window}s]))"
            ),
            scale: 1.0,
        },
        oom_killed: Query {
            promql: format!(
                "max by (pod) (kube_pod_container_status_last_terminated_reason{{{pods_only},reason=\"OOMKilled\"}})"
            ),
            scale: 1.0,
        },
    }
}

/// What a store answered that is not a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreError {
    /// The store's words (`errorType: error`), or what was wrong with its answer.
    pub message: String,
}

/// One pod's values, one per time of the range; `null` where it had no sample.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrometheusSeries {
    /// The pod.
    pub pod: String,
    /// The values.
    pub values: Vec<Option<f64>>,
}

/// A restart of a pod's container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrometheusRestart {
    /// The pod.
    pub pod: String,
    /// The end of the step it happened in (Unix seconds).
    pub at: i64,
    /// Its container had last ended `OOMKilled` by then: it went over its memory limit.
    pub oom_killed: bool,
}

/// Each pod's values from a range query's answer (`resultType: matrix`), on `times`, multiplied
/// by `scale`. `NaN` and infinities, which PromQL gives for a division by zero, are gaps.
///
/// # Errors
///
/// When the store answered `status: error`, or with something else than a matrix.
pub fn series(
    answer: &Value,
    times: &[i64],
    scale: f64,
) -> Result<Vec<PrometheusSeries>, StoreError> {
    if answer.get("status").and_then(Value::as_str) != Some("success") {
        let message = answer
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("the store answered without a status")
            .to_owned();
        return Err(StoreError { message });
    }
    let data = answer.get("data").unwrap_or(&Value::Null);
    if data.get("resultType").and_then(Value::as_str) != Some("matrix") {
        return Err(StoreError {
            message: "the store's answer is not a range of values (resultType matrix)".to_owned(),
        });
    }
    let index: BTreeMap<i64, usize> = times.iter().enumerate().map(|(i, &t)| (t, i)).collect();
    let mut pods: BTreeMap<String, Vec<Option<f64>>> = BTreeMap::new();
    for result in data
        .get("result")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(pod) = result.pointer("/metric/pod").and_then(Value::as_str) else {
            continue;
        };
        let values = pods
            .entry(pod.to_owned())
            .or_insert_with(|| vec![None; times.len()]);
        for point in result
            .get("values")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (Some(time), Some(value)) = (point.get(0).and_then(seconds), point.get(1)) else {
                continue;
            };
            let value = value
                .as_str()
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite());
            if let (Some(&slot), Some(value)) = (index.get(&time), value)
                && let Some(cell) = values.get_mut(slot)
            {
                *cell = Some(value * scale);
            }
        }
    }
    Ok(pods
        .into_iter()
        .map(|(pod, values)| PrometheusSeries { pod, values })
        .collect())
}

/// A point's time: Unix seconds, which Prometheus sends as a number with a fraction.
fn seconds(value: &Value) -> Option<i64> {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Unix seconds of a range's points fit an i64 and have no fraction to keep"
    )]
    value.as_f64().map(|t| t.round() as i64)
}

/// The restarts in `restarts` (from [`Queries::restarts`]), sorted by time.
///
/// One per run of steps with an increase, at its first step, marked `OOMKilled` where
/// `oom_killed` (from [`Queries::oom_killed`]) says so at that step or the next.
#[must_use]
pub fn restarts(
    restarts: &[PrometheusSeries],
    oom_killed: &[PrometheusSeries],
    times: &[i64],
) -> Vec<PrometheusRestart> {
    let mut out = Vec::new();
    for series in restarts {
        let oom = oom_killed.iter().find(|s| s.pod == series.pod);
        let mut before = false;
        for (i, value) in series.values.iter().enumerate() {
            // `increase` extrapolates: a restart shows as a value near 1, never far under.
            let now = value.is_some_and(|v| v >= 0.5);
            if now && !before {
                let oom_killed = oom.is_some_and(|s| {
                    [i, i.saturating_add(1)]
                        .iter()
                        .any(|&j| s.values.get(j).copied().flatten().is_some_and(|v| v >= 1.0))
                });
                if let Some(&at) = times.get(i) {
                    out.push(PrometheusRestart {
                        pod: series.pod.clone(),
                        at,
                        oom_killed,
                    });
                }
            }
            before = now;
        }
    }
    out.sort_by(|a, b| (a.at, &a.pod).cmp(&(b.at, &b.pod)));
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn ranges_have_their_steps_and_end_on_a_step() {
        for (text, points) in [("1h", 241), ("6h", 361), ("24h", 289), ("7d", 337)] {
            let range = PrometheusRange::parse(text).unwrap();
            let times = range.times(1_760_000_007);
            assert_eq!(times.len(), points, "{text}");
            assert_eq!(times.last().unwrap() % range.step(), 0);
            assert!(*times.last().unwrap() <= 1_760_000_007);
            assert_eq!(times.last().unwrap() - times[0], range.seconds());
        }
        assert_eq!(PrometheusRange::parse("30d"), None);
    }

    #[test]
    fn a_range_is_named_as_serde_names_it() {
        use PrometheusRange::{Day, Hour, SixHours, Week};
        for range in [Hour, SixHours, Day, Week] {
            assert_eq!(serde_json::to_value(range).unwrap(), range.as_str());
            assert_eq!(PrometheusRange::parse(range.as_str()), Some(range));
        }
        assert_eq!(PrometheusRange::default(), Hour);
    }

    #[test]
    fn pod_patterns_follow_each_controllers_names() {
        assert_eq!(
            pod_pattern("Deployment", "web").unwrap(),
            "web-[a-z0-9]{1,10}-[a-z0-9]{5}"
        );
        assert_eq!(
            pod_pattern("statefulset", "db.v2").unwrap(),
            "db[.]v2-[0-9]+"
        );
        assert_eq!(
            pod_pattern("CronJob", "nightly").unwrap(),
            "nightly-[0-9]+-[a-z0-9]{5}"
        );
        assert_eq!(pod_pattern("Service", "web"), None);
        // A value that would end the PromQL string or widen the expression is not a name.
        for name in ["web\"}", "web|.*", "Web", ""] {
            assert_eq!(pod_pattern("deployment", name), None, "{name}");
        }
    }

    #[test]
    fn queries_name_the_namespace_and_pods_and_leave_out_pod_cgroups() {
        let q = queries("shop", "web-[0-9]+", PrometheusRange::Hour);
        assert_eq!(
            q.cpu.promql,
            "sum by (pod) (rate(container_cpu_usage_seconds_total{namespace=\"shop\",\
             pod=~\"web-[0-9]+\",container!=\"\",container!=\"POD\",image!=\"\"}[120s]))"
        );
        assert!(
            q.memory
                .promql
                .contains("max_over_time(container_memory_working_set_bytes")
        );
        assert!(q.memory.promql.ends_with("[60s]))"));
        assert_eq!(q.cpu.scale, 1_000.0);
        let q = queries("shop", "web-[0-9]+", PrometheusRange::Week);
        assert!(q.cpu.promql.ends_with("[1800s]))"));
    }

    /// A Prometheus answer with a gap, `NaN`, a point between steps and a series without a pod.
    #[test]
    fn a_matrix_becomes_one_row_of_values_per_pod() {
        let times = [100, 115, 130];
        let answer = json!({"status": "success", "data": {"resultType": "matrix", "result": [
            {"metric": {"pod": "web-b"}, "values": [[100, "0.25"], [130.0, "NaN"]]},
            {"metric": {"pod": "web-a"}, "values": [[115, "0.5"], [120, "9"]]},
            {"metric": {}, "values": [[100, "1"]]}
        ]}});
        let rows = series(&answer, &times, 1_000.0).unwrap();
        assert_eq!(
            rows,
            [
                PrometheusSeries {
                    pod: "web-a".to_owned(),
                    values: vec![None, Some(500.0), None]
                },
                PrometheusSeries {
                    pod: "web-b".to_owned(),
                    values: vec![Some(250.0), None, None]
                },
            ]
        );
    }

    #[test]
    fn a_store_error_keeps_its_words() {
        let answer = json!({"status": "error", "errorType": "bad_data",
            "error": "invalid parameter \"query\": 1:5: parse error"});
        assert_eq!(
            series(&answer, &[0], 1.0).unwrap_err().message,
            "invalid parameter \"query\": 1:5: parse error"
        );
        let vector = json!({"status": "success", "data": {"resultType": "vector", "result": []}});
        assert!(series(&vector, &[0], 1.0).is_err());
    }

    #[test]
    fn restarts_are_one_per_run_and_marked_when_out_of_memory() {
        let times = [0, 15, 30, 45, 60];
        let restarts_of = |pod: &str, values: Vec<Option<f64>>| PrometheusSeries {
            pod: pod.to_owned(),
            values,
        };
        let found = restarts(
            &[
                restarts_of(
                    "a",
                    vec![Some(0.0), Some(1.1), Some(0.9), Some(0.0), Some(2.0)],
                ),
                restarts_of("b", vec![Some(0.0), Some(0.2), None, None, None]),
            ],
            &[restarts_of(
                "a",
                vec![None, None, Some(1.0), Some(1.0), Some(0.0)],
            )],
            &times,
        );
        assert_eq!(
            found,
            [
                PrometheusRestart {
                    pod: "a".to_owned(),
                    at: 15,
                    oom_killed: true
                },
                PrometheusRestart {
                    pod: "a".to_owned(),
                    at: 60,
                    oom_killed: false
                },
            ]
        );
    }
}
