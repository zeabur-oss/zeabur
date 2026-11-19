//! The S3 bucket name rule [s3-names].
//!
//! [s3-names]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/bucketnamingrules.html

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
}
