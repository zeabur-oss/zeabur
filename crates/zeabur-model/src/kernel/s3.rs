//! The S3 bucket name rule [s3-names], and the shape of a store's CA certificates in PEM
//! [rfc7468].
//!
//! [s3-names]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/bucketnamingrules.html
//! [rfc7468]: https://www.rfc-editor.org/rfc/rfc7468#section-3

/// [`is_bucket_name`] as an ECMA-262 pattern, for the API's description.
pub const BUCKET_PATTERN: &str = "^[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]$";

/// Whether `bucket` is 3 to 63 lower-case letters, digits, `.` and `-`, starting and ending with
/// a letter or digit.
#[must_use]
pub fn is_bucket_name(bucket: &str) -> bool {
    (3..=63).contains(&bucket.len())
        && bucket
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
        && bucket.starts_with(|c: char| c.is_ascii_alphanumeric())
        && bucket.ends_with(|c: char| c.is_ascii_alphanumeric())
}

/// The most bytes a CA bundle may have; a ConfigMap and a Secret hold at most 1 MiB.
pub const CA_BUNDLE_MAX: usize = 64 * 1024;

const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
const END: &str = "-----END CERTIFICATE-----";

/// Whether `pem` is one or more `CERTIFICATE` blocks in PEM [rfc7468] and nothing else, at most
/// [`CA_BUNDLE_MAX`] bytes. It checks the shape only: the store's TLS client parses the
/// certificates.
///
/// [rfc7468]: https://www.rfc-editor.org/rfc/rfc7468#section-3
#[must_use]
pub fn is_ca_bundle(pem: &str) -> bool {
    let mut rest = pem.trim();
    if rest.is_empty() || pem.len() > CA_BUNDLE_MAX {
        return false;
    }
    while !rest.is_empty() {
        let Some(body) = rest.strip_prefix(BEGIN) else {
            return false;
        };
        let Some((body, after)) = body.split_once(END) else {
            return false;
        };
        let base64 = |b: u8| b.is_ascii_alphanumeric() || b"+/=".contains(&b);
        if body.trim().is_empty() || !body.bytes().all(|b| base64(b) || b.is_ascii_whitespace()) {
            return false;
        }
        rest = after.trim_start();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_follow_s3_and_the_pattern_agrees() {
        let pattern = regex::Regex::new(BUCKET_PATTERN).unwrap();
        let long = "a".repeat(64);
        let cases = [
            ("my.bucket-1", true),
            ("abc", true),
            ("ab", false),
            ("My", false),
            (".ab", false),
            ("ab-", false),
            ("a b", false),
            ("a_b", false),
            (long.as_str(), false),
        ];
        for (bucket, ok) in cases {
            assert_eq!(is_bucket_name(bucket), ok, "{bucket:?}");
            assert_eq!(pattern.is_match(bucket), ok, "pattern {bucket:?}");
        }
    }

    #[test]
    fn a_ca_bundle_is_certificate_blocks_only() {
        let one = "-----BEGIN CERTIFICATE-----\nMIIB+zCCAaGgAwIBAgIU\n-----END CERTIFICATE-----";
        let two = format!("{one}\n\n{one}\n");
        let key = "-----BEGIN PRIVATE KEY-----\nMIIB\n-----END PRIVATE KEY-----";
        let big = format!(
            "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----",
            "A".repeat(CA_BUNDLE_MAX)
        );
        let cases = [
            (one, true),
            (two.as_str(), true),
            ("", false),
            ("not a certificate", false),
            (key, false),
            (&format!("{one}\n{key}"), false),
            ("-----BEGIN CERTIFICATE-----\nMIIB", false),
            (
                "-----BEGIN CERTIFICATE-----\n-----END CERTIFICATE-----",
                false,
            ),
            (
                "-----BEGIN CERTIFICATE-----\nMI*B\n-----END CERTIFICATE-----",
                false,
            ),
            (big.as_str(), false),
        ];
        for (pem, ok) in cases {
            assert_eq!(is_ca_bundle(pem), ok, "{pem:?}");
        }
    }
}
