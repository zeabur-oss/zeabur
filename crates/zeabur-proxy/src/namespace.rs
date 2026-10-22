//! A namespace name as the request gave it, checked once.

use std::fmt;

use crate::reject::Reject;

/// A Kubernetes namespace name, checked.
///
/// An RFC 1123 DNS label: 1 to 63 lowercase letters, digits and `-`, starting and ending with a
/// letter or digit. Only [`Namespace::parse`] makes one, so a value of this type is safe to place
/// inside a LogsQL stream filter's quotes.
///
/// <https://kubernetes.io/docs/concepts/overview/working-with-objects/names/#dns-label-names>
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Namespace(String);

impl Namespace {
    /// The longest DNS label.
    pub const MAX_LEN: usize = 63;

    /// Checks `name`.
    ///
    /// # Errors
    ///
    /// [`Reject::InvalidNamespace`] when it is not a DNS label.
    pub fn parse(name: &str) -> Result<Self, Reject> {
        let bytes = name.as_bytes();
        let inner = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-';
        let edge = |b: Option<&u8>| b.is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
        if bytes.len() <= Self::MAX_LEN
            && edge(bytes.first())
            && edge(bytes.last())
            && bytes.iter().all(inner)
        {
            Ok(Self(name.to_owned()))
        } else {
            Err(Reject::InvalidNamespace)
        }
    }

    /// The name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_dns_labels_are_namespaces() {
        for good in ["a", "shop", "shop-2", "0", &"a".repeat(63)] {
            assert_eq!(Namespace::parse(good).unwrap().as_str(), good);
        }
        for bad in [
            "",
            "-a",
            "a-",
            "Shop",
            "shop.prod",
            "shop_prod",
            "shop prod",
            "shop\"",
            "shop\n",
            "shöp",
            "shop%2C",
            "shop}",
            &"a".repeat(64),
        ] {
            assert_eq!(
                Namespace::parse(bad),
                Err(Reject::InvalidNamespace),
                "{bad:?}"
            );
        }
    }
}
