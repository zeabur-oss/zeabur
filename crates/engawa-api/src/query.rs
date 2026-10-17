//! The query string of the event lists: `limit` and each list's own filters.
//!
//! Lists other than events return every item: Engawa reads the kind whole from kube-apiserver to
//! sort and filter it, so a page would save bytes, not reads, and the lists are short. Events are
//! long-lived only for an hour by default and can be many, so their lists return the newest
//! `limit` (100 unless asked, at most 500) and say in `totalSize` how many matched. There is no
//! next page: a client that needs more asks for a larger `limit`.
//!
//! Parsed by hand, like the log parameters: axum's `Query` extractor would bring
//! `serde_urlencoded` into the binary for a few parameters. Unknown parameters are ignored.

use std::collections::BTreeMap;

use engawa_core::problem::{Kind, Problem};
use utoipa::IntoParams;

use crate::error::ApiError;

/// The most events one answer holds.
pub(crate) const MAX_LIMIT: usize = 500;
/// Events per answer when the client names no `limit`.
pub(crate) const DEFAULT_LIMIT: usize = 100;

/// The `profile` parameter of an install guide (for the description; parsed by hand).
#[expect(
    dead_code,
    reason = "describes the parameter in the OpenAPI document; parsed by hand"
)]
#[derive(Debug, Default, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ProfileParams {
    /// `single-node` or `multi-node`; by default the one that fits the cluster's node count.
    profile: Option<String>,
}

/// The `limit` parameter of the event lists (for the description; parsed by [`EventQuery`]).
#[expect(
    dead_code,
    reason = "describes the parameter in the OpenAPI document; parsed by hand"
)]
#[derive(Debug, Default, IntoParams)]
#[into_params(parameter_in = Query, rename_all = "camelCase")]
pub(crate) struct LimitParams {
    /// The most events to return, newest first: 100 by default, at most 500 (a larger value is
    /// lowered to 500).
    limit: Option<i64>,
}

/// An event list's query string: `limit` and the list's own filters.
#[derive(Debug, Default)]
pub(crate) struct EventQuery {
    /// How many events to return.
    pub(crate) limit: usize,
    /// The other parameters this list knows, by name.
    pub(crate) filters: BTreeMap<String, String>,
}

impl EventQuery {
    /// Reads `query`, keeping the parameters named in `filters` besides `limit`.
    pub(crate) fn parse(query: Option<&str>, filters: &[&str]) -> Result<Self, ApiError> {
        let mut parsed = Self {
            limit: DEFAULT_LIMIT,
            filters: BTreeMap::new(),
        };
        for (name, value) in form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
            match name.as_ref() {
                "limit" => {
                    let limit = value
                        .parse::<i64>()
                        .ok()
                        .filter(|&n| n > 0)
                        .ok_or_else(|| {
                            ApiError::from(Problem::new(
                                Kind::BadRequest,
                                format!("`limit` must be a positive whole number, not {value:?}."),
                            ))
                        })?;
                    parsed.limit = usize::try_from(limit).map_or(MAX_LIMIT, |n| n.min(MAX_LIMIT));
                }
                other if filters.contains(&other) => {
                    parsed.filters.insert(other.to_owned(), value.into_owned());
                }
                _ => {}
            }
        }
        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_defaults_is_capped_and_must_be_positive() {
        assert_eq!(EventQuery::parse(None, &[]).unwrap().limit, DEFAULT_LIMIT);
        assert_eq!(EventQuery::parse(Some("limit=7"), &[]).unwrap().limit, 7);
        assert_eq!(
            EventQuery::parse(Some("limit=100000"), &[]).unwrap().limit,
            MAX_LIMIT
        );
        for bad in ["limit=0", "limit=-1", "limit=ten"] {
            let error = EventQuery::parse(Some(bad), &[]).unwrap_err();
            assert_eq!(error.0.status, 400, "{bad}");
        }
    }

    #[test]
    fn only_named_filters_are_kept() {
        let query =
            EventQuery::parse(Some("regarding=pod/a&other=x&pageToken=y"), &["regarding"]).unwrap();
        assert_eq!(
            query.filters,
            BTreeMap::from([("regarding".to_owned(), "pod/a".to_owned())])
        );
    }
}
