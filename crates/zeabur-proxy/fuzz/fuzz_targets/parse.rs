//! The log and metrics modules' parsers: never panic, and whatever they accept names exactly
//! one valid namespace, the one the query string gave once.

#![no_main]

use http::Method;
use libfuzzer_sys::fuzz_target;
use zeabur_proxy::logs::Logs;
use zeabur_proxy::metrics::Metrics;
use zeabur_proxy::namespace::Namespace;
use zeabur_proxy::rules::{Backend, Scoped};

const PATHS: [&str; 10] = [
    "/logs/select/logsql/query",
    "/logs/select/logsql/hits",
    "/metrics/api/v1/query",
    "/metrics/api/v1/query_range",
    "/metrics/api/v1/series",
    "/metrics/api/v1/labels",
    "/metrics/api/v1/label/namespace/values",
    "/logs/select/logsql/tail",
    "/logs/select/logsql/query/",
    "/metrics/api/v1/export",
];

fuzz_target!(|data: &[u8]| {
    let Some((&first, rest)) = data.split_first() else {
        return;
    };
    let path = PATHS[usize::from(first) % PATHS.len()];
    let method = if first & 0x80 == 0 { Method::GET } else { Method::POST };
    let query = String::from_utf8_lossy(rest);
    let namespace = if path.starts_with("/logs/") {
        let Ok(parsed) = Logs::parse(&method, path, Some(&query)) else {
            return;
        };
        parsed.namespace().clone()
    } else {
        let Ok(parsed) = Metrics::parse(&method, path, Some(&query)) else {
            return;
        };
        parsed.namespace().clone()
    };
    assert_eq!(method, Method::GET);
    assert!(PATHS[..7].contains(&path));
    assert_eq!(Namespace::parse(namespace.as_str()).ok().as_ref(), Some(&namespace));
    let namespace = namespace.as_str();
    let given: Vec<String> = form_urlencoded::parse(query.as_bytes())
        .filter(|(key, _)| key == "namespace")
        .map(|(_, value)| value.into_owned())
        .collect();
    assert_eq!(given, [namespace]);
});
