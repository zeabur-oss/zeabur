//! The few pages the server writes itself, outside the web interface: a sign-in that could not
//! finish. [`zeabur_api::page`] writes the pages.

use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
pub(crate) use zeabur_api::page::secure;
use zeabur_api::page::{Lang, html};

/// Why a sign-in could not finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SignInProblem {
    /// The issuer answered with an error, or the person cancelled.
    Failed,
    /// No sign-in in progress matches the answer: expired, or started in another browser.
    Expired,
    /// Zeabur Identity could not be reached.
    IssuerUnreachable,
    /// kube-apiserver did not accept the token.
    ClusterRefused,
}

impl SignInProblem {
    const fn text(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Self::Failed, Lang::En) => "Sign-in did not finish.",
            (Self::Failed, Lang::ZhTw) => "登入沒有完成。",
            (Self::Failed, Lang::ZhCn) => "登录没有完成。",
            (Self::Expired, Lang::En) => "This sign-in expired or was started in another browser.",
            (Self::Expired, Lang::ZhTw) => "這次登入已過期，或是從其他瀏覽器開始的。",
            (Self::Expired, Lang::ZhCn) => "这次登录已过期，或是在其他浏览器中发起的。",
            (Self::IssuerUnreachable, Lang::En) => {
                "Unable to reach the sign-in service. Try again in a moment."
            }
            (Self::IssuerUnreachable, Lang::ZhTw) => "無法連上登入服務，稍後再試一次。",
            (Self::IssuerUnreachable, Lang::ZhCn) => "无法连接登录服务，稍后再试一次。",
            (Self::ClusterRefused, Lang::En) => {
                "The cluster did not accept your sign-in. Ask a cluster administrator to check your access."
            }
            (Self::ClusterRefused, Lang::ZhTw) => "叢集不接受這次登入。請叢集管理員確認你的權限。",
            (Self::ClusterRefused, Lang::ZhCn) => "集群不接受这次登录。请集群管理员确认你的权限。",
        }
    }

    const fn status(self) -> StatusCode {
        match self {
            Self::Failed | Self::Expired => StatusCode::BAD_REQUEST,
            Self::IssuerUnreachable => StatusCode::SERVICE_UNAVAILABLE,
            Self::ClusterRefused => StatusCode::FORBIDDEN,
        }
    }
}

/// The page for a sign-in that could not finish, with a link to start again.
pub(crate) fn sign_in_problem(request: &HeaderMap, problem: SignInProblem) -> Response {
    let lang = Lang::of(request);
    let again = match lang {
        Lang::En => "Sign in again",
        Lang::ZhTw => "重新登入",
        Lang::ZhCn => "重新登录",
    };
    html(
        problem.status(),
        lang,
        "",
        None,
        &format!(
            "<p>{}</p><p><a href=\"/auth/login\">{again}</a></p>",
            problem.text(lang)
        ),
    )
}
