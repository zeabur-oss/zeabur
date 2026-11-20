//! Reading an answer of the API into the server's own types: the type a handler serializes on a
//! success, a [`Problem`] otherwise.

use http::StatusCode;
use serde::de::DeserializeOwned;
use zeabur_core::problem::{Kind, Problem};

/// The body of an answer with `status` from `from`, as `T` or as the problem the API answered
/// with.
pub(crate) fn read<T: DeserializeOwned>(
    status: StatusCode,
    body: &[u8],
    from: &str,
) -> Result<T, Box<Problem>> {
    if status.is_success() {
        serde_json::from_slice(body)
            .map_err(|e| internal(&format!("Cannot read the answer of {from}: {e}")))
    } else {
        Err(serde_json::from_slice(body).map_or_else(
            |_e| internal(&format!("{from} answered {status} without a problem.")),
            Box::new,
        ))
    }
}

/// A problem of `zeaburctl`'s own, in the API's shape.
pub(crate) fn internal(detail: &str) -> Box<Problem> {
    Box::new(Problem::new(Kind::Internal, detail))
}

#[cfg(test)]
mod tests {
    use zeabur_api::views::WhoAmI;

    use super::*;

    #[test]
    fn a_success_is_the_servers_type_and_a_refusal_its_problem() {
        let whoami = WhoAmI {
            username: "alice".to_owned(),
            groups: Vec::new(),
            member: false,
            admin: false,
        };
        let body = serde_json::to_vec(&whoami).unwrap();
        assert_eq!(read::<WhoAmI>(StatusCode::OK, &body, "x"), Ok(whoami));
        let refused = Problem::new(Kind::Forbidden, "You cannot list databases in shop.");
        let body = serde_json::to_vec(&refused).unwrap();
        assert_eq!(
            read::<WhoAmI>(StatusCode::FORBIDDEN, &body, "x"),
            Err(Box::new(refused))
        );
        let broken = read::<WhoAmI>(StatusCode::BAD_GATEWAY, b"<html>", "Zeabur").unwrap_err();
        assert_eq!(broken.kind, Kind::Internal);
        assert!(broken.detail.contains("502"), "{}", broken.detail);
    }
}
