//! Why a request gets no data.

use http::StatusCode;

/// Every refusal. A request out of the person's reach gets one of these, never an empty result
/// (as prom-label-proxy's `--error-on-replace`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Reject {
    /// No `Authorization: Bearer` header.
    #[error("send the token as Authorization: Bearer")]
    NoToken,
    /// kube-apiserver did not accept the token.
    #[error("kube-apiserver did not accept the token")]
    Unauthenticated,
    /// No backend serves the path's prefix.
    #[error("no backend is served at this path")]
    UnknownBackend,
    /// A method other than `GET`.
    #[error("only GET is served")]
    Method,
    /// A path the backend does not serve.
    #[error("this path is not served")]
    Path,
    /// A parameter not on the backend's list.
    #[error("a parameter is not allowed")]
    UnknownParameter,
    /// A parameter given more than once.
    #[error("a parameter is given more than once")]
    Repeated,
    /// No `namespace` parameter.
    #[error("namespace is required")]
    MissingNamespace,
    /// `namespace` is not a DNS label.
    #[error("namespace is not a valid namespace name")]
    InvalidNamespace,
    /// `start` or `end` is not an RFC 3339 time or Unix seconds, or `start` is after `end`.
    #[error("start and end must be RFC 3339 times or Unix seconds, start before end")]
    InvalidTime,
    /// `limit` is not a positive integer.
    #[error("limit must be a positive integer")]
    InvalidLimit,
    /// `step` is not a whole number of seconds from 1, or makes too many points.
    #[error("step must be whole seconds from 1, with at most 11000 points in the range")]
    InvalidStep,
    /// `query` is too long or holds control characters.
    #[error("query is too long or holds control characters")]
    InvalidQuery,
    /// The range ends before the namespace was created, so it could only show an earlier
    /// namespace of the same name.
    #[error("the range ends before the namespace was created")]
    BeforeNamespace,
    /// The person may not read this resource in this namespace, or may not read the namespace.
    #[error("you may not read this in this namespace")]
    Forbidden,
    /// The person has too many queries in flight, or the proxy has.
    #[error("too many queries in flight")]
    Busy,
    /// kube-apiserver could not answer the access review in time.
    #[error("kube-apiserver did not answer the access review")]
    Unavailable,
    /// The backend failed or did not answer in time.
    #[error("the backend did not answer")]
    Backend,
}

impl Reject {
    /// The HTTP status.
    #[must_use]
    pub const fn status(self) -> StatusCode {
        match self {
            Self::NoToken | Self::Unauthenticated => StatusCode::UNAUTHORIZED,
            Self::UnknownBackend => StatusCode::NOT_FOUND,
            Self::Method
            | Self::Path
            | Self::UnknownParameter
            | Self::Repeated
            | Self::MissingNamespace
            | Self::InvalidNamespace
            | Self::InvalidTime
            | Self::InvalidLimit
            | Self::InvalidStep
            | Self::InvalidQuery
            | Self::BeforeNamespace => StatusCode::BAD_REQUEST,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Busy => StatusCode::TOO_MANY_REQUESTS,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Backend => StatusCode::BAD_GATEWAY,
        }
    }

    /// A stable `snake_case` code for clients.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::NoToken => "no_token",
            Self::Unauthenticated => "unauthenticated",
            Self::UnknownBackend => "unknown_backend",
            Self::Method => "method",
            Self::Path => "path",
            Self::UnknownParameter => "unknown_parameter",
            Self::Repeated => "repeated_parameter",
            Self::MissingNamespace => "missing_namespace",
            Self::InvalidNamespace => "invalid_namespace",
            Self::InvalidTime => "invalid_time",
            Self::InvalidLimit => "invalid_limit",
            Self::InvalidStep => "invalid_step",
            Self::InvalidQuery => "invalid_query",
            Self::BeforeNamespace => "before_namespace",
            Self::Forbidden => "forbidden",
            Self::Busy => "busy",
            Self::Unavailable => "unavailable",
            Self::Backend => "backend",
        }
    }
}
