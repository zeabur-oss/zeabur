//! The log module's parser: never panics, and whatever it accepts names exactly one valid
//! namespace, the one the query string gave once.

#![no_main]

use http::Method;
use libfuzzer_sys::fuzz_target;
use zeabur_proxy::logs::Logs;
use zeabur_proxy::namespace::Namespace;
use zeabur_proxy::rules::{Backend, Scoped};

const PATHS: [&str; 4] = [
    "/logs/select/logsql/query",
    "/logs/select/logsql/hits",
    "/logs/select/logsql/tail",
    "/logs/select/logsql/query/",
];

fuzz_target!(|data: &[u8]| {
    let Some((&first, rest)) = data.split_first() else {
        return;
    };
    let path = PATHS[usize::from(first) % PATHS.len()];
    let method = if first & 0x80 == 0 { Method::GET } else { Method::POST };
    let query = String::from_utf8_lossy(rest);
    let Ok(parsed) = Logs::parse(&method, path, Some(&query)) else {
        return;
    };
    assert_eq!(method, Method::GET);
    assert!(path == PATHS[0] || path == PATHS[1]);
    let namespace = parsed.namespace().as_str();
    assert_eq!(Namespace::parse(namespace).ok().as_ref(), Some(parsed.namespace()));
    let given: Vec<String> = form_urlencoded::parse(query.as_bytes())
        .filter(|(key, _)| key == "namespace")
        .map(|(_, value)| value.into_owned())
        .collect();
    assert_eq!(given, [namespace]);
});
