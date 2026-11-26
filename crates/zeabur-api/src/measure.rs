//! The answer to each save, for the experience program (docs/internal/measurement.md).

use std::sync::Arc;

use zeabur_telemetry::Submitted;

use crate::error::ApiError;
use crate::handlers::AppState;

/// The answer one person got to one save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The person's user name, which the receiver hashes and never sends.
    pub user: String,
    /// When they saved, in Unix seconds.
    pub at: i64,
    /// The answer.
    pub result: Submitted,
}

/// What receives each [`Report`]; it decides whether to send.
pub type Changes = Arc<dyn Fn(Report) + Send + Sync>;

/// Reports the answer `user` got to a save.
pub(crate) fn submitted(state: &AppState, user: &str, answer: Result<(), &ApiError>) {
    let Some(changes) = &state.changes else {
        return;
    };
    changes(Report {
        user: user.to_owned(),
        at: (state.clock)().as_second(),
        result: answer.map_or_else(|e| refusal(e.0.status), |()| Submitted::Accepted),
    });
}

/// The class of the HTTP status the person got.
const fn refusal(status: u16) -> Submitted {
    match status {
        403 => Submitted::Forbidden,
        409 => Submitted::Conflict,
        422 => Submitted::Invalid,
        502..=504 => Submitted::Unreachable,
        _ => Submitted::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_is_named_by_its_status() {
        assert_eq!(refusal(403), Submitted::Forbidden);
        assert_eq!(refusal(409), Submitted::Conflict);
        assert_eq!(refusal(422), Submitted::Invalid);
        assert_eq!(refusal(503), Submitted::Unreachable);
        assert_eq!(refusal(400), Submitted::Other);
    }
}
