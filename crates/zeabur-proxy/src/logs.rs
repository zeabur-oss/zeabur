//! VictoriaLogs: log history, one namespace at a time.
//!
//! The person's query goes to VictoriaLogs with `extra_stream_filters` naming the namespace the
//! access review allowed. VictoriaLogs applies extra filters to every subquery of a LogsQL query,
//! so a pipe or a subquery cannot leave the namespace
//! (<https://docs.victoriametrics.com/victorialogs/querying/#extra-filters>). The filter is on
//! the stream field the collector sets from the pod's own metadata, which a log line cannot
//! change (the catalog's VLAgent keeps fields parsed from a line out of the stream).

use std::str::FromStr as _;

use http::Method;
use k8s_openapi::jiff::{SignedDuration, Timestamp};

use crate::namespace::Namespace;
use crate::reject::Reject;
use crate::rules::{Authorized, Backend, Scoped, Upstream};

/// The stream field VictoriaLogs' Kubernetes collector (vlagent) fills with the pod's namespace.
pub const STREAM_FIELD: &str = "kubernetes.pod_namespace";

/// The longest query accepted, in bytes.
pub const MAX_QUERY: usize = 4096;

/// The paths served, below [`Logs::PREFIX`].
const QUERY_PATH: &str = "/select/logsql/query";
const HITS_PATH: &str = "/select/logsql/hits";

/// The VictoriaLogs backend.
#[derive(Debug, Clone, Copy)]
pub struct Logs;

/// Which VictoriaLogs endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    /// `/select/logsql/query`: matching lines, newest first.
    Query,
    /// `/select/logsql/hits`: how many lines match, per time step.
    Hits,
}

/// A log query, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogsQuery {
    /// The endpoint.
    pub endpoint: Endpoint,
    /// The namespace.
    pub namespace: Namespace,
    /// The LogsQL query; `*` when none was given.
    pub query: String,
    /// The range's start.
    pub start: Option<Timestamp>,
    /// The range's end.
    pub end: Option<Timestamp>,
    /// How many lines.
    pub limit: Option<u32>,
}

impl Scoped for LogsQuery {
    fn namespace(&self) -> &Namespace {
        &self.namespace
    }
}

/// The bounds of a log query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogsLimits {
    /// Lines when the query names no limit.
    pub default_limit: u32,
    /// The most lines a query gets; a larger limit is lowered to it.
    pub max_limit: u32,
    /// The range when the query names no start.
    pub default_span: SignedDuration,
    /// The longest range; an earlier start is moved up to it.
    pub max_span: SignedDuration,
}

impl Default for LogsLimits {
    fn default() -> Self {
        Self {
            default_limit: 100,
            max_limit: 1000,
            default_span: SignedDuration::from_hours(1),
            max_span: SignedDuration::from_hours(24 * 7),
        }
    }
}

impl Backend for Logs {
    const PREFIX: &'static str = "/logs/";
    const RESOURCE: &'static str = "logs";
    type Query = LogsQuery;
    type Limits = LogsLimits;

    fn parse(method: &Method, path: &str, query: Option<&str>) -> Result<LogsQuery, Reject> {
        if method != Method::GET {
            return Err(Reject::Method);
        }
        let endpoint = match path.strip_prefix("/logs") {
            Some(QUERY_PATH) => Endpoint::Query,
            Some(HITS_PATH) => Endpoint::Hits,
            Some(_) | None => return Err(Reject::Path),
        };
        let mut namespace = None;
        let mut text = None;
        let mut start = None;
        let mut end = None;
        let mut limit = None;
        for (key, value) in form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
            let slot = match key.as_ref() {
                "namespace" => &mut namespace,
                "query" => &mut text,
                "start" => &mut start,
                "end" => &mut end,
                "limit" => &mut limit,
                _ => return Err(Reject::UnknownParameter),
            };
            if slot.replace(value.into_owned()).is_some() {
                return Err(Reject::Repeated);
            }
        }
        let namespace = Namespace::parse(&namespace.ok_or(Reject::MissingNamespace)?)?;
        let query = match text {
            None => "*".to_owned(),
            Some(text) if text.trim().is_empty() => "*".to_owned(),
            Some(text) if text.len() > MAX_QUERY || text.chars().any(forbidden_char) => {
                return Err(Reject::InvalidQuery);
            }
            Some(text) => text,
        };
        let start = start.as_deref().map(time).transpose()?;
        let end = end.as_deref().map(time).transpose()?;
        if let (Some(start), Some(end)) = (start, end)
            && start >= end
        {
            return Err(Reject::InvalidTime);
        }
        let limit = match limit {
            None => None,
            Some(limit) => Some(positive(&limit).ok_or(Reject::InvalidLimit)?),
        };
        Ok(LogsQuery {
            endpoint,
            namespace,
            query,
            start,
            end,
            limit,
        })
    }

    /// The range is cut to start no earlier than the namespace's creation, so a namespace
    /// recreated under the same name never shows the earlier one's lines, and to the longest
    /// span; the limit is lowered to the most allowed.
    fn build(
        authorized: &Authorized<LogsQuery>,
        limits: &LogsLimits,
        now: Timestamp,
    ) -> Result<Upstream, Reject> {
        let query = authorized.query();
        let created = authorized.grant().created();
        let end = query.end.unwrap_or(now).min(now);
        if end <= created {
            return Err(Reject::BeforeNamespace);
        }
        let earliest = end
            .checked_sub(limits.max_span)
            .map_err(|_invalid| Reject::InvalidTime)?
            .max(created);
        let start = match query.start {
            Some(start) => start,
            None => end
                .checked_sub(limits.default_span)
                .map_err(|_invalid| Reject::InvalidTime)?,
        }
        .max(earliest);
        if start >= end {
            return Err(Reject::InvalidTime);
        }
        let mut params = vec![
            ("query", query.query.clone()),
            ("start", start.to_string()),
            ("end", end.to_string()),
            (
                "extra_stream_filters",
                format!("{{{STREAM_FIELD}=\"{}\"}}", authorized.grant().namespace()),
            ),
        ];
        let path = match query.endpoint {
            Endpoint::Query => {
                let limit = query.limit.unwrap_or(limits.default_limit);
                params.push(("limit", limit.min(limits.max_limit).to_string()));
                QUERY_PATH
            }
            Endpoint::Hits => {
                let span = u64::try_from(start.duration_until(end).as_secs()).unwrap_or(0);
                let seconds = span.div_ceil(60).max(1);
                params.push(("step", format!("{seconds}s")));
                HITS_PATH
            }
        };
        Ok(Upstream::new(path, params))
    }
}

pub(crate) const fn forbidden_char(c: char) -> bool {
    c.is_control() && c != '\t' && c != '\n'
}

/// An RFC 3339 time, or whole Unix seconds.
pub(crate) fn time(text: &str) -> Result<Timestamp, Reject> {
    if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
        let seconds = text
            .parse::<i64>()
            .map_err(|_invalid| Reject::InvalidTime)?;
        return Timestamp::from_second(seconds).map_err(|_invalid| Reject::InvalidTime);
    }
    Timestamp::from_str(text).map_err(|_invalid| Reject::InvalidTime)
}

pub(crate) fn positive(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<u32>().ok().filter(|n| *n > 0)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::access::Grant;

    const PATH: &str = "/logs/select/logsql/query";

    fn now() -> Timestamp {
        "2026-10-22T12:00:00Z".parse().unwrap()
    }

    fn created() -> Timestamp {
        "2026-10-01T00:00:00Z".parse().unwrap()
    }

    fn grant(namespace: &str) -> Grant {
        Grant::for_tests(namespace, Logs::RESOURCE, created())
    }

    fn parse(query: &str) -> Result<LogsQuery, Reject> {
        Logs::parse(&Method::GET, PATH, Some(query))
    }

    fn build(query: LogsQuery, namespace: &str) -> Result<Upstream, Reject> {
        let authorized = Authorized::new::<Logs>(query, grant(namespace))?;
        Logs::build(&authorized, &LogsLimits::default(), now())
    }

    fn param<'a>(upstream: &'a Upstream, key: &str) -> Vec<&'a str> {
        upstream
            .params()
            .iter()
            .filter(|(k, _)| *k == key)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// Known ways to reach another namespace's lines, each refused before anything is asked of
    /// kube-apiserver.
    #[test]
    fn attack_cases_are_refused() {
        let cases: &[(&str, Reject)] = &[
            ("namespace=a&namespace=b", Reject::Repeated),
            ("namespace=a&namespace=a", Reject::Repeated),
            ("namespace=a&Namespace=b", Reject::UnknownParameter),
            ("namespace=a&NAMESPACE=b", Reject::UnknownParameter),
            ("namespace=a%2Cb", Reject::InvalidNamespace),
            ("namespace=a,b", Reject::InvalidNamespace),
            ("namespace=a%26namespace%3Db", Reject::InvalidNamespace),
            ("namespace=a%22%7D", Reject::InvalidNamespace),
            ("namespace=a\"}", Reject::InvalidNamespace),
            ("namespace=A", Reject::InvalidNamespace),
            ("namespace=", Reject::InvalidNamespace),
            ("namespace", Reject::InvalidNamespace),
            ("namespace=a%00", Reject::InvalidNamespace),
            ("namespace=a%0A", Reject::InvalidNamespace),
            ("namespace=%E2%80%8Ba", Reject::InvalidNamespace),
            ("namespace=%C3%A1", Reject::InvalidNamespace),
            ("namespace=%ff", Reject::InvalidNamespace),
            ("namespace=a+b", Reject::InvalidNamespace),
            ("namespace=*", Reject::InvalidNamespace),
            ("namespace=.*", Reject::InvalidNamespace),
            ("query=*", Reject::MissingNamespace),
            ("", Reject::MissingNamespace),
            ("namespace=a&extra_filters=x", Reject::UnknownParameter),
            (
                "namespace=a&extra_stream_filters=%7B%7D",
                Reject::UnknownParameter,
            ),
            (
                "namespace=a&extra_filters%5B%5D=x",
                Reject::UnknownParameter,
            ),
            ("namespace=a&AccountID=1", Reject::UnknownParameter),
            ("namespace=a&ProjectID=1", Reject::UnknownParameter),
            ("namespace=a&tenant=1", Reject::UnknownParameter),
            ("namespace=a&=b", Reject::UnknownParameter),
            ("namespace=a&step=1s", Reject::UnknownParameter),
            ("namespace=a&query=x&query=y", Reject::Repeated),
            ("namespace=a&limit=0", Reject::InvalidLimit),
            ("namespace=a&limit=-1", Reject::InvalidLimit),
            ("namespace=a&limit=1e9", Reject::InvalidLimit),
            ("namespace=a&limit=99999999999", Reject::InvalidLimit),
            ("namespace=a&start=yesterday", Reject::InvalidTime),
            ("namespace=a&start=1h", Reject::InvalidTime),
            (
                "namespace=a&start=2026-10-22&end=2026-10-21",
                Reject::InvalidTime,
            ),
            (
                "namespace=a&start=2026-10-22T00:00:00Z&end=2026-10-21T00:00:00Z",
                Reject::InvalidTime,
            ),
            ("namespace=a&query=%00", Reject::InvalidQuery),
            ("namespace=a&query=%1B%5B31m", Reject::InvalidQuery),
        ];
        for (query, reject) in cases {
            assert_eq!(parse(query), Err(*reject), "{query}");
        }
        let long = format!("namespace=a&query={}", "x".repeat(MAX_QUERY + 1));
        assert_eq!(parse(&long), Err(Reject::InvalidQuery));
    }

    #[test]
    fn only_get_on_the_two_paths_is_served() {
        let query = Some("namespace=a");
        for method in [Method::POST, Method::PUT, Method::DELETE, Method::HEAD] {
            assert_eq!(Logs::parse(&method, PATH, query), Err(Reject::Method));
        }
        for path in [
            "/logs/select/logsql/query/",
            "/logs//select/logsql/query",
            "/logs/select/logsql/%71uery",
            "/logs/select/logsql/../logsql/query",
            "/logs/select/logsql/QUERY",
            "/logs/select/logsql/tail",
            "/logs/select/logsql/field_values",
            "/logs/select/logsql/stream_ids",
            "/logs/insert/jsonline",
            "/logs/internal/insert",
            "/logs/delete/run_task",
            "/logs/metrics",
            "/logs/",
            "/select/logsql/query",
        ] {
            assert_eq!(
                Logs::parse(&Method::GET, path, query),
                Err(Reject::Path),
                "{path}"
            );
        }
        assert_eq!(
            Logs::parse(&Method::GET, "/logs/select/logsql/hits", query)
                .unwrap()
                .endpoint,
            Endpoint::Hits
        );
    }

    #[test]
    fn a_grant_for_another_namespace_or_resource_authorizes_nothing() {
        let query = parse("namespace=a").unwrap();
        assert_eq!(
            Authorized::new::<Logs>(query.clone(), grant("b")).err(),
            Some(Reject::Forbidden)
        );
        let metrics = Grant::for_tests("a", "metrics", created());
        assert_eq!(
            Authorized::new::<Logs>(query, metrics).err(),
            Some(Reject::Forbidden)
        );
    }

    #[test]
    fn the_request_is_rebuilt_with_the_namespace_filter_and_the_bounds() {
        let query =
            parse("namespace=shop&query=error+%7C+limit+5&limit=5000&start=2020-01-01T00:00:00Z")
                .unwrap();
        let upstream = build(query, "shop").unwrap();
        assert_eq!(upstream.path(), "/select/logsql/query");
        assert_eq!(
            upstream.params(),
            [
                ("query", "error | limit 5".to_owned()),
                // Not before the namespace was created, nor more than a week before the end.
                ("start", "2026-10-15T12:00:00Z".to_owned()),
                ("end", "2026-10-22T12:00:00Z".to_owned()),
                (
                    "extra_stream_filters",
                    "{kubernetes.pod_namespace=\"shop\"}".to_owned()
                ),
                ("limit", "1000".to_owned()),
            ]
        );
        assert_eq!(
            upstream.path_and_query(),
            "/select/logsql/query?query=error+%7C+limit+5&start=2026-10-15T12%3A00%3A00Z\
             &end=2026-10-22T12%3A00%3A00Z\
             &extra_stream_filters=%7Bkubernetes.pod_namespace%3D%22shop%22%7D&limit=1000"
        );
    }

    #[test]
    fn the_range_starts_when_the_namespace_was_created_and_never_ends_before() {
        let query = parse("namespace=shop&start=1700000000&end=2026-10-02T00:00:00Z").unwrap();
        let upstream = build(query, "shop").unwrap();
        assert_eq!(param(&upstream, "start"), ["2026-10-01T00:00:00Z"]);
        let before = parse("namespace=shop&start=1700000000&end=2026-09-30T00:00:00Z").unwrap();
        assert_eq!(build(before, "shop"), Err(Reject::BeforeNamespace));
        let defaults = build(parse("namespace=shop").unwrap(), "shop").unwrap();
        assert_eq!(param(&defaults, "start"), ["2026-10-22T11:00:00Z"]);
        assert_eq!(param(&defaults, "limit"), ["100"]);
        assert_eq!(param(&defaults, "query"), ["*"]);
    }

    #[test]
    fn hits_get_a_step_and_no_limit() {
        let query = Logs::parse(
            &Method::GET,
            "/logs/select/logsql/hits",
            Some("namespace=shop&start=2026-10-22T11:00:00Z&limit=3"),
        )
        .unwrap();
        let upstream = build(query, "shop").unwrap();
        assert_eq!(upstream.path(), "/select/logsql/hits");
        assert_eq!(param(&upstream, "step"), ["60s"]);
        assert_eq!(param(&upstream, "limit"), Vec::<&str>::new());
    }

    fn piece() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("namespace=a".to_owned()),
            Just("namespace=b".to_owned()),
            Just("namespace=a%2Cb".to_owned()),
            Just("Namespace=b".to_owned()),
            Just("extra_filters=%7Bkubernetes.pod_namespace%3D%22b%22%7D".to_owned()),
            Just("extra_stream_filters=%7B%7D".to_owned()),
            Just("AccountID=0".to_owned()),
            Just("query=%7Bkubernetes.pod_namespace%3D%22b%22%7D".to_owned()),
            Just("query=*+%7C+union+(%7Bkubernetes.pod_namespace%3D%22b%22%7D)".to_owned()),
            Just("limit=10".to_owned()),
            Just("start=2026-10-20T00:00:00Z".to_owned()),
            Just("end=1798000000".to_owned()),
            "[a-z=&%+0-9A-F{}\"]{0,12}",
            any::<String>(),
        ]
    }

    proptest! {
        /// Whatever the request, the backend's request has exactly one namespace filter, equal
        /// to the namespace the grant is for, and no filter or tenant parameter of the person's.
        #[test]
        fn every_forwarded_request_reads_exactly_the_granted_namespace(
            pieces in prop::collection::vec(piece(), 0..6),
            granted in prop_oneof![Just("a"), Just("b")],
            hits in any::<bool>(),
        ) {
            let path = if hits { "/logs/select/logsql/hits" } else { PATH };
            let raw = pieces.join("&");
            let Ok(query) = Logs::parse(&Method::GET, path, Some(&raw)) else {
                return Ok(());
            };
            let asked = query.namespace.clone();
            let Ok(authorized) = Authorized::new::<Logs>(query, grant(granted)) else {
                prop_assert_ne!(asked.as_str(), granted);
                return Ok(());
            };
            prop_assert_eq!(asked.as_str(), granted);
            let Ok(upstream) = Logs::build(&authorized, &LogsLimits::default(), now()) else {
                return Ok(());
            };
            let filter = format!("{{{STREAM_FIELD}=\"{granted}\"}}");
            prop_assert_eq!(param(&upstream, "extra_stream_filters"), vec![filter.as_str()]);
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
            prop_assert_eq!(&filters[0].1, &filter);
            for (key, _) in &decoded {
                prop_assert!(
                    ["query", "start", "end", "extra_stream_filters", "limit", "step"]
                        .contains(&key.as_str()),
                    "{}", key
                );
            }
        }
    }
}
