//! Namespaces and object names from a request, checked before they become part of a URL path.
//!
//! kube's `Api` puts a name into the request path as it is given, without percent-encoding it,
//! so a name such as `../secrets/db` would address another resource than the one asked for; the
//! writes, which encode their path segments, still keep `..` as it is. A read or a write
//! is therefore refused before it is sent when its namespace is not a DNS-1123 label (the
//! rule for a Namespace's name), or when its object name is one no object can have: empty, `.`
//! or `..`, or holding `/` or `%`, which kube-apiserver refuses in every name
//! ([k8s-path-segment]). Zeabur also refuses characters that a URL path cannot carry unencoded
//! (anything outside RFC 3986's `pchar`, [rfc3986-path]); no object Zeabur shows has one, as
//! most kinds' names are DNS-1123 subdomains ([k8s-names]).
//!
//! A refused name is answered as kube-apiserver answers a read of a name that does not exist:
//! not found ([`ClusterError::InvalidName`]).
//!
//! [k8s-path-segment]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apimachinery/pkg/api/validation/path/name.go
//! [rfc3986-path]: https://www.rfc-editor.org/rfc/rfc3986#section-3.3
//! [k8s-names]: https://kubernetes.io/docs/concepts/overview/working-with-objects/names/

use crate::port::ClusterError;

/// `Ok` when `namespace` is a DNS-1123 label: at most 63 lower-case letters, digits and `-`,
/// starting and ending with a letter or digit.
pub(crate) fn namespace(namespace: &str) -> Result<(), ClusterError> {
    let bytes = namespace.as_bytes();
    let edge =
        |byte: Option<&u8>| byte.is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    let valid = bytes.len() <= 63
        && edge(bytes.first())
        && edge(bytes.last())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-');
    if valid {
        Ok(())
    } else {
        Err(ClusterError::InvalidName(format!(
            "{namespace:?} is not a namespace name"
        )))
    }
}

/// `Ok` when `name` is a name an object can have and a URL path can carry as it is.
pub(crate) fn name(name: &str) -> Result<(), ClusterError> {
    let valid = !matches!(name, "" | "." | "..")
        && name.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'-' | b'.'
                        | b'_'
                        | b'~'
                        | b'!'
                        | b'$'
                        | b'&'
                        | b'\''
                        | b'('
                        | b')'
                        | b'*'
                        | b'+'
                        | b','
                        | b';'
                        | b'='
                        | b':'
                        | b'@'
                )
        });
    if valid {
        Ok(())
    } else {
        Err(ClusterError::InvalidName(format!(
            "{name:?} is not a name an object can have"
        )))
    }
}

/// [`namespace`] when there is one.
pub(crate) fn optional_namespace(namespace: Option<&str>) -> Result<(), ClusterError> {
    namespace.map_or(Ok(()), self::namespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaces_are_dns_labels() {
        for valid in ["shop", "a", "kube-system", "0abc", &"a".repeat(63)] {
            assert_eq!(namespace(valid), Ok(()), "{valid}");
        }
        for invalid in [
            "",
            "Shop",
            "-shop",
            "shop-",
            "shop.example",
            "..",
            "../secrets",
            "shop%2F",
            &"a".repeat(64),
        ] {
            assert!(namespace(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn names_cannot_leave_their_path_segment() {
        for valid in [
            "web",
            "web-1.example",
            "system:aggregate-to-admin",
            "web.17a8b9c0d1e2f3a4",
            "a..b",
        ] {
            assert_eq!(name(valid), Ok(()), "{valid}");
        }
        for invalid in [
            "",
            ".",
            "..",
            "../secrets/db",
            "..%2Fsecrets%2Fdb",
            "web/status",
            "web?watch=true",
            "web#x",
            "web x",
            "w\u{e9}b",
            "web\n",
        ] {
            assert!(name(invalid).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn a_refused_name_reads_as_not_found() {
        let error = name("..").unwrap_err();
        assert_eq!(error.code(), Some(404));
        assert_eq!(
            error.into_problem("the ConfigMap").kind,
            zeabur_core::problem::Kind::NotFound
        );
    }
}
