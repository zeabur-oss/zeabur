//! VictoriaMetrics: usage history, one namespace at a time.
//!
//! The person's query goes to VictoriaMetrics with `extra_label=namespace=<namespace>`, the
//! namespace the access review allowed. VictoriaMetrics adds this label filter to every series
//! selector of the query, subqueries included, and to the series and label APIs
//! (<https://docs.victoriametrics.com/victoriametrics/single-server-victoriametrics/#prometheus-querying-api-enhancements>).
//! The filter is joined by AND to the query's own filters, so a query for another namespace, or
//! for `{__name__=~".+"}`, matches only this namespace's series. The `namespace` label of the
//! container and kube-state-metrics series is the pod's namespace, which the scrape sets from
//! kubelet and kube-state-metrics, not from the pod.
//!
//! Only the Prometheus query, range query, series, label names and label values APIs are served.
//! Others, such as `export`, `federate`, `status/tsdb`, `metadata` or the vmalert ones, are not.
//!
//! The namespace label keeps other namespaces out. It does not keep out an earlier namespace of
//! the same name, so time does: VictoriaMetrics reads samples before a query's first point (a
//! range selector, a subquery, `offset`; see [`crate::reach`]), and the first point is moved to
//! after the namespace's creation by that much. A query whose reach cannot be bounded is refused.
//! The series and label APIs find series by VictoriaMetrics' per-day index, which also holds a
//! series active in the last hour of the day before. Their range starts on the second day after
//! the creation (UTC).

use http::Method;
use k8s_openapi::jiff::{SignedDuration, Timestamp};

use crate::logs::{forbidden_char, positive, time};
use crate::namespace::Namespace;
use crate::reach::reach;
use crate::reject::Reject;
use crate::rules::{Authorized, Backend, Scoped, Upstream};

/// The label every series selector is held to.
pub const LABEL: &str = "namespace";

/// The longest query or `match[]` accepted, in bytes.
pub const MAX_QUERY: usize = 4096;

/// The most `match[]` parameters one request may have.
pub const MAX_MATCHES: usize = 16;

/// The most points one range query may return per series (Prometheus' own limit).
pub const MAX_POINTS: i64 = 11_000;

/// The longest label name accepted.
const MAX_LABEL: usize = 128;

/// The step of an instant query, sent with it: VictoriaMetrics' default when its flags are
/// unset.
pub const INSTANT_STEP: u32 = 300;

const DAY: i64 = 86_400;

const QUERY_PATH: &str = "/api/v1/query";
const QUERY_RANGE_PATH: &str = "/api/v1/query_range";
const SERIES_PATH: &str = "/api/v1/series";
const LABELS_PATH: &str = "/api/v1/labels";

/// The VictoriaMetrics backend.
#[derive(Debug, Clone, Copy)]
pub struct Metrics;

/// Which Prometheus API endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// `/api/v1/query`: an instant query.
    Query,
    /// `/api/v1/query_range`: a range query.
    QueryRange,
    /// `/api/v1/series`: the series that match.
    Series,
    /// `/api/v1/labels`: label names.
    Labels,
    /// `/api/v1/label/<name>/values`: the values of one label.
    LabelValues(String),
}

impl Endpoint {
    const fn allows(&self, key: &str) -> bool {
        match self {
            Self::Query => matches!(key.as_bytes(), b"query" | b"time"),
            Self::QueryRange => matches!(key.as_bytes(), b"query" | b"start" | b"end" | b"step"),
            Self::Series | Self::Labels | Self::LabelValues(_) => {
                matches!(key.as_bytes(), b"match[]" | b"start" | b"end" | b"limit")
            }
        }
    }
}

/// A metrics request, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricsQuery {
    /// The endpoint.
    pub endpoint: Endpoint,
    /// The namespace.
    pub namespace: Namespace,
    /// The query (`PromQL` or `MetricsQL`), for the two query endpoints.
    pub query: Option<String>,
    /// The series selectors, for the series and label endpoints.
    pub matches: Vec<String>,
    /// The instant query's time.
    pub time: Option<Timestamp>,
    /// The range's start.
    pub start: Option<Timestamp>,
    /// The range's end.
    pub end: Option<Timestamp>,
    /// Seconds between two points of a range query.
    pub step: Option<u32>,
    /// How many series, names or values.
    pub limit: Option<u32>,
    /// For the two query endpoints, the seconds before a point the query reads samples from.
    pub reach: i64,
}

impl Scoped for MetricsQuery {
    fn namespace(&self) -> &Namespace {
        &self.namespace
    }
}

/// The bounds of a metrics request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricsLimits {
    /// The range when the request names no start.
    pub default_span: SignedDuration,
    /// The longest range; an earlier start is moved up to it.
    pub max_span: SignedDuration,
    /// The most series, names or values; a larger limit is lowered to it.
    pub max_limit: u32,
}

impl Default for MetricsLimits {
    fn default() -> Self {
        Self {
            default_span: SignedDuration::from_hours(1),
            max_span: SignedDuration::from_hours(24 * 7),
            max_limit: 10_000,
        }
    }
}

fn endpoint(path: &str) -> Result<Endpoint, Reject> {
    let path = path.strip_prefix("/metrics").ok_or(Reject::Path)?;
    match path {
        QUERY_PATH => Ok(Endpoint::Query),
        QUERY_RANGE_PATH => Ok(Endpoint::QueryRange),
        SERIES_PATH => Ok(Endpoint::Series),
        LABELS_PATH => Ok(Endpoint::Labels),
        _ => {
            let name = path
                .strip_prefix("/api/v1/label/")
                .and_then(|rest| rest.strip_suffix("/values"))
                .ok_or(Reject::Path)?;
            let bytes = name.as_bytes();
            let valid = !bytes.is_empty()
                && bytes.len() <= MAX_LABEL
                && !bytes.first().is_some_and(u8::is_ascii_digit)
                && bytes
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || *b == b'_');
            if valid {
                Ok(Endpoint::LabelValues(name.to_owned()))
            } else {
                Err(Reject::Path)
            }
        }
    }
}

fn selector(text: &str) -> Result<String, Reject> {
    if text.trim().is_empty() || text.len() > MAX_QUERY || text.chars().any(forbidden_char) {
        return Err(Reject::InvalidQuery);
    }
    Ok(text.to_owned())
}

impl Backend for Metrics {
    const PREFIX: &'static str = "/metrics/";
    const RESOURCE: &'static str = "metrics";
    type Query = MetricsQuery;
    type Limits = MetricsLimits;

    fn parse(method: &Method, path: &str, query: Option<&str>) -> Result<MetricsQuery, Reject> {
        if method != Method::GET {
            return Err(Reject::Method);
        }
        let endpoint = endpoint(path)?;
        let mut namespace = None;
        let mut text = None;
        let mut matches = Vec::new();
        let mut instant = None;
        let mut start = None;
        let mut end = None;
        let mut step = None;
        let mut limit = None;
        for (key, value) in form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
            if key != "namespace" && !endpoint.allows(&key) {
                return Err(Reject::UnknownParameter);
            }
            if key == "match[]" {
                if matches.len() >= MAX_MATCHES {
                    return Err(Reject::InvalidQuery);
                }
                matches.push(selector(&value)?);
                continue;
            }
            let slot = match key.as_ref() {
                "namespace" => &mut namespace,
                "query" => &mut text,
                "time" => &mut instant,
                "start" => &mut start,
                "end" => &mut end,
                "step" => &mut step,
                "limit" => &mut limit,
                _ => return Err(Reject::UnknownParameter),
            };
            if slot.replace(value.into_owned()).is_some() {
                return Err(Reject::Repeated);
            }
        }
        let namespace = Namespace::parse(&namespace.ok_or(Reject::MissingNamespace)?)?;
        let query = match (&endpoint, text) {
            (Endpoint::Query | Endpoint::QueryRange, None) => return Err(Reject::InvalidQuery),
            (_, text) => text.as_deref().map(selector).transpose()?,
        };
        if endpoint == Endpoint::Series && matches.is_empty() {
            return Err(Reject::InvalidQuery);
        }
        let step = match (&endpoint, step) {
            (Endpoint::QueryRange, None) => return Err(Reject::InvalidStep),
            (_, None) => None,
            (_, Some(step)) => Some(positive(&step).ok_or(Reject::InvalidStep)?),
        };
        let start = start.as_deref().map(time).transpose()?;
        let end = end.as_deref().map(time).transpose()?;
        if let (Some(start), Some(end)) = (start, end)
            && start > end
        {
            return Err(Reject::InvalidTime);
        }
        let limit = match limit {
            None => None,
            Some(limit) => Some(positive(&limit).ok_or(Reject::InvalidLimit)?),
        };
        let reach = match (&endpoint, &query) {
            (Endpoint::Query | Endpoint::QueryRange, Some(text)) => {
                let step = i64::from(step.unwrap_or(INSTANT_STEP)).saturating_mul(1000);
                reach(text, step)?.saturating_add(999) / 1000
            }
            _ => 0,
        };
        Ok(MetricsQuery {
            endpoint,
            namespace,
            query,
            matches,
            time: instant.as_deref().map(time).transpose()?,
            start,
            end,
            step,
            limit,
            reach,
        })
    }

    /// Times are cut to now, and to read no sample from before the namespace's creation: a
    /// query's points start its reach after it, the series and label APIs on the second day
    /// after it. A range is cut to the longest span (a range query's start moves by whole
    /// steps, so its points stay on the asked grid), and a limit to the most allowed.
    fn build(
        authorized: &Authorized<MetricsQuery>,
        limits: &MetricsLimits,
        now: Timestamp,
    ) -> Result<Upstream, Reject> {
        let query = authorized.query();
        let created = ceil(authorized.grant().created());
        let now = now.as_second();
        let filter = (
            "extra_label",
            format!("{LABEL}={}", authorized.grant().namespace()),
        );
        let text = || query.query.clone().ok_or(Reject::InvalidQuery);
        let first = match query.endpoint {
            Endpoint::Query | Endpoint::QueryRange => {
                created.saturating_add(query.reach).saturating_add(1)
            }
            Endpoint::Series | Endpoint::Labels | Endpoint::LabelValues(_) => created
                .div_euclid(DAY)
                .saturating_add(2)
                .saturating_mul(DAY),
        };
        if query.endpoint == Endpoint::Query {
            let at = query.time.map_or(now, Timestamp::as_second).min(now);
            if at < first {
                return Err(Reject::BeforeNamespace);
            }
            return Ok(Upstream::new(
                QUERY_PATH,
                vec![
                    ("query", text()?),
                    ("time", at.to_string()),
                    ("step", INSTANT_STEP.to_string()),
                    filter,
                ],
            ));
        }
        let end = query.end.map_or(now, Timestamp::as_second).min(now);
        if end < first {
            return Err(Reject::BeforeNamespace);
        }
        let earliest = end.saturating_sub(limits.max_span.as_secs()).max(first);
        let mut start = query.start.map_or_else(
            || end.saturating_sub(limits.default_span.as_secs()),
            Timestamp::as_second,
        );
        if let Some(step) = query
            .step
            .filter(|_| query.endpoint == Endpoint::QueryRange)
        {
            let step = i64::from(step);
            if start < earliest {
                let steps = (earliest - start).saturating_add(step - 1) / step;
                start = start.saturating_add(steps.saturating_mul(step));
            }
            if start > end {
                return Err(Reject::InvalidTime);
            }
            if (end - start) / step + 1 > MAX_POINTS {
                return Err(Reject::InvalidStep);
            }
            return Ok(Upstream::new(
                QUERY_RANGE_PATH,
                vec![
                    ("query", text()?),
                    ("start", start.to_string()),
                    ("end", end.to_string()),
                    ("step", step.to_string()),
                    filter,
                ],
            ));
        }
        start = start.max(earliest);
        if start > end {
            return Err(Reject::InvalidTime);
        }
        let path = match &query.endpoint {
            Endpoint::Series => SERIES_PATH.to_owned(),
            Endpoint::Labels => LABELS_PATH.to_owned(),
            Endpoint::LabelValues(name) => format!("/api/v1/label/{name}/values"),
            Endpoint::Query | Endpoint::QueryRange => return Err(Reject::InvalidStep),
        };
        let mut params: Vec<(&'static str, String)> = query
            .matches
            .iter()
            .map(|m| ("match[]", m.clone()))
            .collect();
        let limit = query
            .limit
            .unwrap_or(limits.max_limit)
            .min(limits.max_limit);
        params.extend([
            ("start", start.to_string()),
            ("end", end.to_string()),
            ("limit", limit.to_string()),
            filter,
        ]);
        Ok(Upstream::new(path, params))
    }
}

/// Whole seconds, rounded up.
fn ceil(at: Timestamp) -> i64 {
    let seconds = at.as_second();
    if at.subsec_nanosecond() > 0 {
        seconds.saturating_add(1)
    } else {
        seconds
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::access::Grant;

    const RANGE: &str = "/metrics/api/v1/query_range";
    /// 2026-10-22T12:00:00Z.
    const NOW: i64 = 1_792_670_400;
    /// 2026-10-01T00:00:00Z.
    const CREATED: i64 = 1_790_812_800;

    fn now() -> Timestamp {
        Timestamp::from_second(NOW).unwrap()
    }

    fn grant(namespace: &str) -> Grant {
        Grant::for_tests(
            namespace,
            Metrics::RESOURCE,
            Timestamp::from_second(CREATED).unwrap(),
        )
    }

    fn parse(path: &str, query: &str) -> Result<MetricsQuery, Reject> {
        Metrics::parse(&Method::GET, path, Some(query))
    }

    fn build(query: MetricsQuery, namespace: &str) -> Result<Upstream, Reject> {
        let authorized = Authorized::new::<Metrics>(query, grant(namespace))?;
        Metrics::build(&authorized, &MetricsLimits::default(), now())
    }

    fn param<'a>(upstream: &'a Upstream, key: &str) -> Vec<&'a str> {
        upstream
            .params()
            .iter()
            .filter(|(k, _)| *k == key)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// Known ways to reach another namespace's series, each refused before anything is asked
    /// of kube-apiserver.
    #[test]
    fn attack_cases_are_refused() {
        let q = "namespace=a&query=up&step=60";
        let cases: &[(&str, &str, Reject)] = &[
            (
                RANGE,
                "namespace=a&namespace=b&query=up&step=60",
                Reject::Repeated,
            ),
            (
                RANGE,
                "namespace=a,b&query=up&step=60",
                Reject::InvalidNamespace,
            ),
            (
                RANGE,
                "namespace=a%22%7D&query=up&step=60",
                Reject::InvalidNamespace,
            ),
            (
                RANGE,
                "namespace=.%2A&query=up&step=60",
                Reject::InvalidNamespace,
            ),
            (RANGE, "query=up&step=60", Reject::MissingNamespace),
            (
                RANGE,
                &format!("{q}&extra_label=namespace%3Db"),
                Reject::UnknownParameter,
            ),
            (
                RANGE,
                &format!("{q}&extra_label=x"),
                Reject::UnknownParameter,
            ),
            (
                RANGE,
                &format!("{q}&extra_filters%5B%5D=%7Bnamespace%3D%22b%22%7D"),
                Reject::UnknownParameter,
            ),
            (
                RANGE,
                &format!("{q}&extra_filters=x"),
                Reject::UnknownParameter,
            ),
            (RANGE, &format!("{q}&Namespace=b"), Reject::UnknownParameter),
            (
                RANGE,
                &format!("{q}&match%5B%5D=up"),
                Reject::UnknownParameter,
            ),
            (RANGE, &format!("{q}&nocache=1"), Reject::UnknownParameter),
            (RANGE, &format!("{q}&AccountID=1"), Reject::UnknownParameter),
        ];
        for (path, query, reject) in cases {
            assert_eq!(parse(path, query), Err(*reject), "{path}?{query}");
        }
        let many = format!("namespace=a{}", "&match%5B%5D=up".repeat(MAX_MATCHES + 1));
        assert_eq!(
            parse("/metrics/api/v1/series", &many),
            Err(Reject::InvalidQuery)
        );
        let long = format!("namespace=a&step=60&query={}", "x".repeat(MAX_QUERY + 1));
        assert_eq!(parse(RANGE, &long), Err(Reject::InvalidQuery));
    }

    #[test]
    fn malformed_requests_are_refused() {
        let q = "namespace=a&query=up&step=60";
        let cases: &[(&str, &str, Reject)] = &[
            (RANGE, &format!("{q}&query=down"), Reject::Repeated),
            (RANGE, "namespace=a&query=up", Reject::InvalidStep),
            (RANGE, "namespace=a&query=up&step=0", Reject::InvalidStep),
            (RANGE, "namespace=a&query=up&step=1m", Reject::InvalidStep),
            (RANGE, "namespace=a&query=up&step=0.5", Reject::InvalidStep),
            (RANGE, "namespace=a&step=60", Reject::InvalidQuery),
            (RANGE, "namespace=a&query=+&step=60", Reject::InvalidQuery),
            (RANGE, "namespace=a&query=%00&step=60", Reject::InvalidQuery),
            (
                RANGE,
                "namespace=a&query=up&step=60&start=1792670400&end=1792670000",
                Reject::InvalidTime,
            ),
            (
                "/metrics/api/v1/query",
                "namespace=a&query=up&step=60",
                Reject::UnknownParameter,
            ),
            (
                "/metrics/api/v1/query",
                "namespace=a&query=up&start=1",
                Reject::UnknownParameter,
            ),
            (
                "/metrics/api/v1/series",
                "namespace=a",
                Reject::InvalidQuery,
            ),
            (
                "/metrics/api/v1/series",
                "namespace=a&query=up",
                Reject::UnknownParameter,
            ),
            (
                "/metrics/api/v1/labels",
                "namespace=a&extra_label=namespace%3Db",
                Reject::UnknownParameter,
            ),
            (
                "/metrics/api/v1/labels",
                "namespace=a&limit=0",
                Reject::InvalidLimit,
            ),
        ];
        for (path, query, reject) in cases {
            assert_eq!(parse(path, query), Err(*reject), "{path}?{query}");
        }
    }

    #[test]
    fn only_get_on_the_query_series_and_label_paths_is_served() {
        let query = Some("namespace=a&match%5B%5D=up");
        for method in [Method::POST, Method::PUT, Method::DELETE, Method::HEAD] {
            assert_eq!(
                Metrics::parse(&method, "/metrics/api/v1/labels", query),
                Err(Reject::Method)
            );
        }
        for path in [
            "/metrics/api/v1/export",
            "/metrics/api/v1/export/native",
            "/metrics/federate",
            "/metrics/api/v1/status/tsdb",
            "/metrics/api/v1/status/active_queries",
            "/metrics/api/v1/metadata",
            "/metrics/api/v1/query_exemplars",
            "/metrics/api/v1/rules",
            "/metrics/api/v1/alerts",
            "/metrics/api/v1/write",
            "/metrics/api/v1/admin/tsdb/delete_series",
            "/metrics/vmui",
            "/metrics/prometheus/api/v1/labels",
            "/metrics/api/v1/labels/",
            "/metrics//api/v1/labels",
            "/metrics/api/v1/../v1/labels",
            "/metrics/api/v1/label//values",
            "/metrics/api/v1/label/1a/values",
            "/metrics/api/v1/label/a%2Fb/values",
            "/metrics/api/v1/label/a/b/values",
            "/metrics/api/v1/label/a/values/",
            "/metrics/",
            "/api/v1/labels",
        ] {
            assert_eq!(
                Metrics::parse(&Method::GET, path, query),
                Err(Reject::Path),
                "{path}"
            );
        }
        assert_eq!(
            Metrics::parse(&Method::GET, "/metrics/api/v1/label/__name__/values", query)
                .unwrap()
                .endpoint,
            Endpoint::LabelValues("__name__".to_owned())
        );
    }

    #[test]
    fn a_grant_for_another_namespace_or_resource_authorizes_nothing() {
        let query = parse(RANGE, "namespace=a&query=up&step=60").unwrap();
        assert_eq!(
            Authorized::new::<Metrics>(query.clone(), grant("b")).err(),
            Some(Reject::Forbidden)
        );
        let logs = Grant::for_tests("a", "logs", Timestamp::from_second(CREATED).unwrap());
        assert_eq!(
            Authorized::new::<Metrics>(query, logs).err(),
            Some(Reject::Forbidden)
        );
    }

    #[test]
    fn a_query_for_another_namespace_is_sent_with_the_granted_namespace_filter() {
        let query = parse(
            RANGE,
            "namespace=shop&query=sum%28rate%28x%7Bnamespace%3D%22bank%22%7D%5B5m%5D%29%29\
             &start=1792666800&end=1792670400&step=60",
        )
        .unwrap();
        let upstream = build(query, "shop").unwrap();
        assert_eq!(upstream.path(), "/api/v1/query_range");
        assert_eq!(
            upstream.params(),
            [
                ("query", "sum(rate(x{namespace=\"bank\"}[5m]))".to_owned()),
                ("start", "1792666800".to_owned()),
                ("end", "1792670400".to_owned()),
                ("step", "60".to_owned()),
                ("extra_label", "namespace=shop".to_owned()),
            ]
        );
    }

    #[test]
    fn series_and_label_requests_get_the_filter_a_range_and_a_limit() {
        let series = parse(
            "/metrics/api/v1/series",
            "namespace=shop&match%5B%5D=%7B__name__%3D~%22.%2B%22%7D&limit=99999",
        )
        .unwrap();
        let upstream = build(series, "shop").unwrap();
        assert_eq!(upstream.path(), "/api/v1/series");
        assert_eq!(param(&upstream, "match[]"), ["{__name__=~\".+\"}"]);
        assert_eq!(param(&upstream, "extra_label"), ["namespace=shop"]);
        assert_eq!(param(&upstream, "limit"), ["10000"]);
        assert_eq!(param(&upstream, "start"), [(NOW - 3600).to_string()]);
        let values = parse("/metrics/api/v1/label/namespace/values", "namespace=shop").unwrap();
        let upstream = build(values, "shop").unwrap();
        assert_eq!(upstream.path(), "/api/v1/label/namespace/values");
        assert_eq!(param(&upstream, "extra_label"), ["namespace=shop"]);
    }

    /// The seconds before its first point `query` reads, at `step` seconds.
    fn reach_of(query: &str, step: u32) -> i64 {
        (reach(query, i64::from(step) * 1000).unwrap() + 999) / 1000
    }

    /// A namespace deleted and created again under the same name: no sample the query reads is
    /// from before the creation, however far back it reaches.
    #[test]
    fn no_query_reads_a_sample_from_before_the_namespace() {
        for query in [
            "up",
            "rate(up[1h])",
            "up offset 2d",
            "max_over_time(up[1d:1h])",
        ] {
            let asked = format!(
                "namespace=shop&step=3600&start=1790800000&query={}",
                form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>()
            );
            let upstream = build(parse(RANGE, &asked).unwrap(), "shop").unwrap();
            let start: i64 = param(&upstream, "start")[0].parse().unwrap();
            assert!(start - reach_of(query, 3600) > CREATED, "{query}");
            assert_eq!((start - 1_790_800_000) % 3600, 0, "{query}");
            assert_eq!(param(&upstream, "end"), [NOW.to_string()]);
        }

        let before = parse(RANGE, "namespace=shop&query=up&step=60&end=1790000000").unwrap();
        assert_eq!(build(before, "shop"), Err(Reject::BeforeNamespace));
        let near = format!("namespace=shop&query=up&step=60&end={}", CREATED + 60);
        assert_eq!(
            build(parse(RANGE, &near).unwrap(), "shop"),
            Err(Reject::BeforeNamespace)
        );

        let instant = parse("/metrics/api/v1/query", "namespace=shop&query=up&time=1").unwrap();
        assert_eq!(build(instant, "shop"), Err(Reject::BeforeNamespace));
        let reach = reach_of("up offset 1d", INSTANT_STEP);
        for (time, read) in [(CREATED + reach, false), (CREATED + reach + 1, true)] {
            let asked = format!("namespace=shop&query=up+offset+1d&time={time}");
            let built = build(parse("/metrics/api/v1/query", &asked).unwrap(), "shop");
            assert_eq!(built.is_ok(), read, "{time}");
        }
        let later = parse(
            "/metrics/api/v1/query",
            "namespace=shop&query=up&time=1900000000",
        );
        let upstream = build(later.unwrap(), "shop").unwrap();
        assert_eq!(param(&upstream, "time"), [NOW.to_string()]);
        assert_eq!(param(&upstream, "step"), [INSTANT_STEP.to_string()]);

        let series = format!(
            "namespace=shop&match%5B%5D=up&start={}&end={}",
            CREATED + 1,
            CREATED + 3 * DAY
        );
        let upstream = build(parse("/metrics/api/v1/series", &series).unwrap(), "shop").unwrap();
        assert_eq!(param(&upstream, "start"), [(CREATED + 2 * DAY).to_string()]);
        let labels = format!("namespace=shop&end={}", CREATED + DAY);
        assert_eq!(
            build(parse("/metrics/api/v1/labels", &labels).unwrap(), "shop"),
            Err(Reject::BeforeNamespace)
        );

        let dense = parse(RANGE, "namespace=shop&query=up&step=1&start=1792600000").unwrap();
        assert_eq!(build(dense, "shop"), Err(Reject::InvalidStep));
    }

    fn piece() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("namespace=a".to_owned()),
            Just("namespace=b".to_owned()),
            Just("namespace=a%2Cb".to_owned()),
            Just("query=up".to_owned()),
            Just("query=x%7Bnamespace%3D%22b%22%7D".to_owned()),
            Just("query=x%5B1h%3A1m%5D+offset+1d".to_owned()),
            Just("query=rate%28x%5B7d%5D%29+offset+2d".to_owned()),
            Just("time=1790813000".to_owned()),
            Just("start=1790812900".to_owned()),
            Just("match%5B%5D=%7B__name__%3D~%22.%2B%22%7D".to_owned()),
            Just("match%5B%5D=%7Bnamespace%3D%22b%22%7D".to_owned()),
            Just("extra_label=namespace%3Db".to_owned()),
            Just("extra_filters%5B%5D=%7Bnamespace%3D%22b%22%7D".to_owned()),
            Just("step=60".to_owned()),
            Just("limit=10".to_owned()),
            Just("time=1792000000".to_owned()),
            Just("start=1791000000".to_owned()),
            Just("end=1792500000".to_owned()),
            "[a-z=&%+0-9A-F{}\"\\[\\]]{0,12}",
            any::<String>(),
        ]
    }

    fn path() -> impl Strategy<Value = &'static str> {
        prop_oneof![
            Just(RANGE),
            Just("/metrics/api/v1/query"),
            Just("/metrics/api/v1/series"),
            Just("/metrics/api/v1/labels"),
            Just("/metrics/api/v1/label/namespace/values"),
        ]
    }

    proptest! {
        /// Whatever the request, the backend's request has exactly one namespace filter, equal
        /// to the namespace the grant is for, and no other filter or parameter of the person's.
        #[test]
        fn every_forwarded_request_reads_exactly_the_granted_namespace(
            pieces in prop::collection::vec(piece(), 0..7),
            granted in prop_oneof![Just("a"), Just("b")],
            path in path(),
        ) {
            let raw = pieces.join("&");
            let Ok(query) = Metrics::parse(&Method::GET, path, Some(&raw)) else {
                return Ok(());
            };
            let asked = query.namespace.clone();
            let Ok(authorized) = Authorized::new::<Metrics>(query, grant(granted)) else {
                prop_assert_ne!(asked.as_str(), granted);
                return Ok(());
            };
            prop_assert_eq!(asked.as_str(), granted);
            let Ok(upstream) = Metrics::build(&authorized, &MetricsLimits::default(), now())
            else {
                return Ok(());
            };
            let filter = format!("namespace={granted}");
            let encoded = upstream.path_and_query();
            let (_, query_string) = encoded.split_once('?').unwrap();
            let decoded: Vec<(String, String)> = form_urlencoded::parse(query_string.as_bytes())
                .into_owned()
                .collect();
            let filters: Vec<&(String, String)> = decoded
                .iter()
                .filter(|(k, _)| k.starts_with("extra_"))
                .collect();
            prop_assert_eq!(filters.len(), 1);
            prop_assert_eq!(&filters[0].0, "extra_label");
            prop_assert_eq!(&filters[0].1, &filter);
            for (key, _) in &decoded {
                prop_assert!(
                    ["query", "time", "start", "end", "step", "match[]", "limit", "extra_label"]
                        .contains(&key.as_str()),
                    "{}", key
                );
            }
            let value = |key: &str| decoded.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
            if let Some(text) = value("query") {
                let first: i64 = value("start").or_else(|| value("time")).unwrap().parse().unwrap();
                let step: u32 = value("step").unwrap().parse().unwrap();
                prop_assert!(first - reach_of(&text, step) > CREATED);
            }
        }
    }
}
