//! The DNS name rules Kubernetes object names and hostnames follow [k8s-names].
//!
//! [k8s-names]: https://kubernetes.io/docs/concepts/overview/working-with-objects/names/

/// The longest DNS label.
pub const LABEL_MAX: usize = 63;

/// The longest DNS subdomain or hostname.
pub const SUBDOMAIN_MAX: usize = 253;

const fn is_label_byte(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'
}

/// Whether `part` is lower-case letters, digits and `-`, starting and ending with a letter or
/// digit, with no limit on its length.
fn is_part(part: &str) -> bool {
    let bytes = part.as_bytes();
    bytes.iter().copied().all(is_label_byte)
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
}

/// Whether `name` is an RFC 1123 label: at most 63 lower-case letters, digits and `-`, starting
/// and ending with a letter or digit [k8s-label]. RFC 1123 lets a label start with a digit
/// [rfc1123].
///
/// [k8s-label]: https://kubernetes.io/docs/concepts/overview/working-with-objects/names/#dns-label-names
/// [rfc1123]: https://www.rfc-editor.org/rfc/rfc1123#section-2.1
#[must_use]
pub fn is_dns_label(name: &str) -> bool {
    name.len() <= LABEL_MAX && is_part(name)
}

/// Whether `name` is an RFC 1035 label of at most `max` characters: an RFC 1123 label that starts
/// with a letter [k8s-rfc1035].
///
/// [k8s-rfc1035]: https://kubernetes.io/docs/concepts/overview/working-with-objects/names/#rfc-1035-label-names
#[must_use]
pub fn is_rfc1035_label(name: &str, max: usize) -> bool {
    name.len() <= max.min(LABEL_MAX)
        && is_part(name)
        && name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
}

/// Whether `name` is a DNS subdomain, the form of most Kubernetes object names.
///
/// At most 253 characters, each part between dots lower-case letters, digits and `-` that starts
/// and ends with a letter or digit [k8s-subdomain]. Kubernetes does not limit a part to 63
/// characters here.
///
/// [k8s-subdomain]: https://kubernetes.io/docs/concepts/overview/working-with-objects/names/#dns-subdomain-names
#[must_use]
pub fn is_dns_subdomain(name: &str) -> bool {
    name.len() <= SUBDOMAIN_MAX && name.split('.').all(is_part)
}

/// Whether `name` is a lower-case hostname: a DNS subdomain whose every part is an RFC 1123
/// label, so at most 63 characters [rfc1123].
///
/// [rfc1123]: https://www.rfc-editor.org/rfc/rfc1123#section-2.1
#[must_use]
pub fn is_hostname(name: &str) -> bool {
    name.len() <= SUBDOMAIN_MAX && name.split('.').all(is_dns_label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_dns_rules() {
        let label_63 = "a".repeat(63);
        let label_64 = "a".repeat(64);
        let dotted_253 = format!("{label_63}.{label_63}.{label_63}.{}", "a".repeat(61));
        let dotted_254 = format!("{dotted_253}a");
        let long_part = format!("{label_64}.example.com");
        // name, label, rfc1035 (max 63), subdomain, hostname
        let cases: &[(&str, bool, bool, bool, bool)] = &[
            ("web", true, true, true, true),
            ("a-1", true, true, true, true),
            ("1abc", true, false, true, true),
            (&label_63, true, true, true, true),
            (&label_64, false, false, true, false),
            ("", false, false, false, false),
            ("-web", false, false, false, false),
            ("web-", false, false, false, false),
            ("Web", false, false, false, false),
            ("we_b", false, false, false, false),
            ("shop.example.com", false, false, true, true),
            ("shop..com", false, false, false, false),
            (".shop", false, false, false, false),
            ("shop.-x.com", false, false, false, false),
            (&dotted_253, false, false, true, true),
            (&dotted_254, false, false, false, false),
            (&long_part, false, false, true, false),
        ];
        for &(name, label, rfc1035, subdomain, hostname) in cases {
            assert_eq!(is_dns_label(name), label, "label {name:?}");
            assert_eq!(
                is_rfc1035_label(name, LABEL_MAX),
                rfc1035,
                "rfc1035 {name:?}"
            );
            assert_eq!(is_dns_subdomain(name), subdomain, "subdomain {name:?}");
            assert_eq!(is_hostname(name), hostname, "hostname {name:?}");
        }
        assert!(is_rfc1035_label("abcd", 4));
        assert!(!is_rfc1035_label("abcde", 4));
    }
}
