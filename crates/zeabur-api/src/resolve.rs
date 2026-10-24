//! Looking up a domain from where Zeabur runs, for a route's trace.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

pub use zeabur_gaps::Resolved;

/// How long one lookup may take.
const TIMEOUT: Duration = Duration::from_secs(3);

/// Looks names up.
pub trait Resolve: Send + Sync {
    /// The addresses of `name`.
    fn resolve<'a>(&'a self, name: &'a str) -> Pin<Box<dyn Future<Output = Resolved> + Send + 'a>>;
}

/// The system's resolver, as the server's process sees it.
#[derive(Debug, Clone, Copy, Default)]
pub struct System;

impl Resolve for System {
    fn resolve<'a>(&'a self, name: &'a str) -> Pin<Box<dyn Future<Output = Resolved> + Send + 'a>> {
        Box::pin(async move {
            match tokio::time::timeout(TIMEOUT, tokio::net::lookup_host((name, 443))).await {
                Ok(Ok(addresses)) => Resolved::Addresses(
                    addresses
                        .map(|a| a.ip().to_string())
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                ),
                Ok(Err(error)) => Resolved::NotFound(error.to_string()),
                Err(_) => Resolved::Unknown,
            }
        })
    }
}

/// Fixed answers, for tests; a name it does not know is not found. A `*.` name answers for one
/// label under it, as a wildcard record does.
#[derive(Debug, Clone, Default)]
pub struct Fixed(pub BTreeMap<String, Vec<String>>);

impl Resolve for Fixed {
    fn resolve<'a>(&'a self, name: &'a str) -> Pin<Box<dyn Future<Output = Resolved> + Send + 'a>> {
        let wildcard = name.split_once('.').map(|(_, rest)| format!("*.{rest}"));
        let found = self
            .0
            .get(name)
            .or_else(|| wildcard.and_then(|w| self.0.get(&w)));
        let answer = found.map_or_else(
            || Resolved::NotFound(format!("no such host: {name}")),
            |addresses| Resolved::Addresses(addresses.clone()),
        );
        Box::pin(async move { answer })
    }
}

/// Each of `names`, looked up at the same time.
pub(crate) async fn all(resolver: &dyn Resolve, names: Vec<String>) -> BTreeMap<String, Resolved> {
    let answers = futures_util::future::join_all(names.iter().map(|n| resolver.resolve(n))).await;
    names.into_iter().zip(answers).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_system_resolver_finds_localhost_and_fixed_answers_only_what_they_know() {
        let local = System.resolve("localhost").await;
        assert!(
            matches!(&local, Resolved::Addresses(a) if a.iter().any(|a| a == "127.0.0.1" || a == "::1")),
            "{local:?}"
        );
        let fixed = Fixed(BTreeMap::from([(
            "a.example.test".to_owned(),
            vec!["192.0.2.1".to_owned()],
        )]));
        assert_eq!(
            fixed.resolve("a.example.test").await,
            Resolved::Addresses(vec!["192.0.2.1".to_owned()])
        );
        assert!(matches!(
            fixed.resolve("b.example.test").await,
            Resolved::NotFound(_)
        ));
    }
}
