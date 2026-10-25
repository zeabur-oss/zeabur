//! Errors as RFC 9457 Problem Details, in plain language, with the Kubernetes error kept.
//!
//! Every error Zeabur returns tells a newcomer what happened and what to do about it (`detail`
//! and `fix`), and keeps the response kube-apiserver gave, unchanged, for people who know
//! Kubernetes (`kubernetes`). The HTTP status and the category come from the Kubernetes
//! `Status` reason [api-conventions], so the meaning is the cluster's, not Zeabur's guess.
//!
//! Problem types are relative URI references under `/problems/` [rfc9457] (§3.1.1): they
//! identify the category and resolve against whatever origin serves Zeabur, so no domain has to
//! host them. Where Zeabur serves its web interface, each resolves to a page of it, the
//! human-readable documentation the RFC asks a type URI to lead to.
//!
//! **The plain-language layers are keyed.** `title`, `detail` and `fix` stay English, as RFC 9457
//! has them. Beside them, `summary` (what happened, naming the
//! object) and `why` (the rule, with this occurrence's numbers) are [`Message`]s a client words
//! in its own language; `evidence` names the objects and fields the problem rests on, and
//! `fields` the fields kube-apiserver found invalid (`details.causes` [api-conventions]), so a
//! form can mark them.
//!
//! **[`Kind`] is open.** New values may be added; clients must treat an unknown `kind` by its
//! HTTP status. RFC 9457 already requires clients to "ignore any such extensions that they don't
//! recognize" (§3.2), and AIP-180 asks that response enums "expected to receive new values should
//! document this" [aip-180].
//!
//! **`Retry-After` comes from kube-apiserver.** When the cluster asks clients to wait (`429` or
//! `503`), the seconds it names travel in [`KubernetesStatus::retry_after_seconds`] and the API
//! repeats them as a `Retry-After` header [rfc9110] (§10.2.3). kube-apiserver answers unpaginated
//! LISTs with `429` and a dynamic `Retry-After` while its watch cache initialises [k8s-concepts],
//! so a client that honours the header recovers on its own. kube-rs keeps the `Status` body, not
//! the response headers, so the value is read from `details.retryAfterSeconds`, which
//! kube-apiserver fills for these answers ("If specified, the time in seconds before the
//! operation should be retried" [api-conventions]); an answer that sets only the header is passed
//! on without one (not verified whether any does).
//!
//! # References
//!
//! - [rfc9457] M. Nottingham, E. Wilde and S. Dalal, "Problem Details for HTTP APIs," RFC 9457,
//!   IETF, July 2023. <https://www.rfc-editor.org/rfc/rfc9457>
//! - [api-conventions] The Kubernetes Authors, "API Conventions: Response Status Kind,"
//!   kubernetes/community, sig-architecture.
//!   <https://github.com/kubernetes/community/blob/master/contributors/devel/sig-architecture/api-conventions.md#response-status-kind>
//! - [aip-180] Google, "AIP-180: Backwards compatibility." <https://google.aip.dev/180>
//! - [rfc9110] R. Fielding, M. Nottingham and J. Reschke, Eds., "HTTP Semantics," RFC 9110,
//!   IETF, June 2022. <https://www.rfc-editor.org/rfc/rfc9110>
//! - [k8s-concepts] The Kubernetes Authors, "Kubernetes API Concepts," Kubernetes Documentation. <https://kubernetes.io/docs/reference/using-api/api-concepts/>
//!
//! [rfc9457]: https://www.rfc-editor.org/rfc/rfc9457
//! [api-conventions]: https://github.com/kubernetes/community/blob/master/contributors/devel/sig-architecture/api-conventions.md#response-status-kind
//! [aip-180]: https://google.aip.dev/180
//! [rfc9110]: https://www.rfc-editor.org/rfc/rfc9110
//! [k8s-concepts]: https://kubernetes.io/docs/reference/using-api/api-concepts/

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
pub use zeabur_kernel::denied::Denied;

use crate::message::Message;
use crate::origin::Origin;
use crate::tool::Tool;

/// The media type of a problem response.
pub const MEDIA_TYPE: &str = "application/problem+json";

/// What went wrong, in categories a person can act on. An open set: clients must expect new
/// values and treat an unknown one by its HTTP status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// The request itself is malformed.
    BadRequest,
    /// No valid credential: not signed in, or the sign-in expired.
    Unauthenticated,
    /// The request carries no Zeabur session: the person never signed in here, signed out, or
    /// the session ended. Zeabur's own refinement of [`Kind::Unauthenticated`], with the same
    /// `401`, so the web interface can tell "go and sign in" from a credential the cluster
    /// refused.
    SignedOut,
    /// Signed in, but the cluster does not allow this to this person.
    Forbidden,
    /// The cluster refused a request (a read, a write, a terminal) to the person's everyday
    /// credential, and they may retry it as an administrator after confirming with their passkey.
    /// Zeabur's own refinement of [`Kind::Forbidden`], with the same `403` and the same
    /// `denied` member; only where Zeabur signs people in and elevation is turned on.
    ElevationRequired,
    /// The person may make the change, but it would take the project over one of its limits
    /// (a `ResourceQuota`): kube-apiserver's quota admission refused it with `403` after its
    /// authorizers allowed it [k8s-quota]. Zeabur's own refinement of [`Kind::Forbidden`], with
    /// the same `403`; `why` gives the limit and what is used of it. Admin rights do not
    /// change it, so it never becomes [`Kind::ElevationRequired`].
    ///
    /// [k8s-quota]: https://kubernetes.io/docs/concepts/policy/resource-quotas/
    QuotaExceeded,
    /// The person may make the change, but a policy in the cluster refused it: an admission
    /// webhook, a `LimitRange`, Pod Security admission or another admission plugin
    /// [k8s-admission]. Zeabur's own refinement of [`Kind::Forbidden`], with the same `403`;
    /// `why` quotes the policy's own words. A webhook's denial arrives as `403`, or as `400` when
    /// the webhook gave no status code of `400` or more [k8s-webhook-errors]; both are this
    /// kind. Admin rights do not change it, so it never becomes [`Kind::ElevationRequired`].
    ///
    /// [k8s-admission]: https://kubernetes.io/docs/reference/access-authn-authz/admission-controllers/
    /// [k8s-webhook-errors]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/admission/plugin/webhook/errors/statuserror.go
    PolicyDenied,
    /// The object does not exist.
    NotFound,
    /// The container log asked for cannot be read: the container has not started, has not
    /// restarted (for the previous run), or is gone from the node; which one travels in
    /// [`Problem::unavailable_log`]. Zeabur's own refinement of [`Kind::NotFound`], with the same
    /// `404`: the request is well-formed, but the log has no current representation (RFC 9110,
    /// §15.5.5) [rfc9110], although kube-apiserver passes the kubelet's refusal on as
    /// `400 BadRequest`.
    ///
    /// [rfc9110]: https://www.rfc-editor.org/rfc/rfc9110
    LogUnavailable,
    /// The object changed, or someone else owns the field.
    Conflict,
    /// Helm, Argo CD or Flux manages the object or owns the field, and would undo the change
    /// (named in [`Problem::managed_by`]). Zeabur's own refinement of [`Kind::Conflict`]: the
    /// same `409`, since the request conflicts with who manages the object and the person can
    /// resolve that and resubmit (RFC 9110, §15.5.10) [rfc9110]; its content is not at fault,
    /// as `422` would say (§15.5.21). It has a type and title of its own because RFC 9457 keeps
    /// a title the same for every occurrence of a type (§3.1.3) [rfc9457].
    ///
    /// [rfc9110]: https://www.rfc-editor.org/rfc/rfc9110
    /// [rfc9457]: https://www.rfc-editor.org/rfc/rfc9457
    Managed,
    /// The token of a plan was presented after its lifetime: the plan has to be made again.
    /// Zeabur's own refinement of [`Kind::BadRequest`], with the same `400`, so a client can tell
    /// it from a malformed or altered token by the type instead of the wording of `detail`.
    PlanExpired,
    /// The namespace is one Kubernetes keeps for itself (`kube-` prefix) and cannot be adopted
    /// as a project. Zeabur's own refusal, before kube-apiserver is asked: `422`, since the
    /// request is well-formed but its instructions cannot be carried out (RFC 9110, §15.5.21)
    /// [rfc9110]; with a type of its own so a client can tell it by the type.
    ///
    /// [rfc9110]: https://www.rfc-editor.org/rfc/rfc9110
    SystemNamespace,
    /// kube-apiserver or an admission webhook rejected the content.
    Invalid,
    /// The cluster is asking clients to slow down.
    TooManyRequests,
    /// kube-apiserver cannot be reached or did not answer in time.
    Unavailable,
    /// Anything else; a bug or an unexpected answer.
    Internal,
}

impl Kind {
    /// Every kind, in the order they were added.
    pub const ALL: [Self; 17] = [
        Self::BadRequest,
        Self::Unauthenticated,
        Self::Forbidden,
        Self::NotFound,
        Self::Conflict,
        Self::Invalid,
        Self::TooManyRequests,
        Self::Unavailable,
        Self::Internal,
        Self::Managed,
        Self::PlanExpired,
        Self::SystemNamespace,
        Self::SignedOut,
        Self::ElevationRequired,
        Self::LogUnavailable,
        Self::QuotaExceeded,
        Self::PolicyDenied,
    ];

    /// The HTTP status code for this kind.
    #[must_use]
    pub const fn status(self) -> u16 {
        match self {
            Self::BadRequest | Self::PlanExpired => 400,
            Self::Unauthenticated | Self::SignedOut => 401,
            Self::Forbidden
            | Self::ElevationRequired
            | Self::QuotaExceeded
            | Self::PolicyDenied => 403,
            Self::NotFound | Self::LogUnavailable => 404,
            Self::Conflict | Self::Managed => 409,
            Self::Invalid | Self::SystemNamespace => 422,
            Self::TooManyRequests => 429,
            Self::Unavailable => 503,
            Self::Internal => 500,
        }
    }

    /// The kind as it is written on the wire (`elevation-required`), and in message keys.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.type_uri()
            .strip_prefix("/problems/")
            .unwrap_or_default()
    }

    /// The problem type URI (a relative reference).
    #[must_use]
    pub const fn type_uri(self) -> &'static str {
        match self {
            Self::BadRequest => "/problems/bad-request",
            Self::Unauthenticated => "/problems/unauthenticated",
            Self::SignedOut => "/problems/signed-out",
            Self::Forbidden => "/problems/forbidden",
            Self::ElevationRequired => "/problems/elevation-required",
            Self::QuotaExceeded => "/problems/quota-exceeded",
            Self::PolicyDenied => "/problems/policy-denied",
            Self::NotFound => "/problems/not-found",
            Self::LogUnavailable => "/problems/log-unavailable",
            Self::Conflict => "/problems/conflict",
            Self::Managed => "/problems/managed",
            Self::PlanExpired => "/problems/plan-expired",
            Self::SystemNamespace => "/problems/system-namespace",
            Self::Invalid => "/problems/invalid",
            Self::TooManyRequests => "/problems/too-many-requests",
            Self::Unavailable => "/problems/unavailable",
            Self::Internal => "/problems/internal",
        }
    }

    /// A short title, the same for every problem of this kind.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::BadRequest => "The request is not valid",
            Self::Unauthenticated => "You are not signed in",
            Self::SignedOut => "You are signed out",
            Self::Forbidden => "You do not have permission",
            Self::ElevationRequired => "This needs admin rights",
            Self::QuotaExceeded => "This would go over the project's limit",
            Self::PolicyDenied => "A policy in the cluster refused this",
            Self::NotFound => "It does not exist",
            Self::LogUnavailable => "The log is not available",
            Self::Conflict => "It changed in the meantime",
            Self::Managed => "Another tool manages it",
            Self::PlanExpired => "The plan expired",
            Self::SystemNamespace => "Kubernetes keeps this namespace for itself",
            Self::Invalid => "The cluster rejected the change",
            Self::TooManyRequests => "The cluster is busy",
            Self::Unavailable => "The cluster cannot be reached",
            Self::Internal => "Something went wrong in Zeabur",
        }
    }

    /// What the person can do about it.
    #[must_use]
    pub const fn fix(self) -> &'static str {
        match self {
            Self::BadRequest => "Check the request and try again.",
            Self::Unauthenticated | Self::SignedOut => "Sign in again.",
            Self::ElevationRequired => "Retry as admin, or ask a cluster administrator for access.",
            Self::QuotaExceeded => {
                "Ask for less, free some of what the project uses, or ask a cluster \
                 administrator to raise the limit."
            }
            Self::PolicyDenied => {
                "Change what the policy names, or ask a cluster administrator about the policy."
            }
            Self::Forbidden => {
                "Ask a cluster administrator for access, or use an account that has it."
            }
            Self::NotFound => "Reload the page to see what the cluster has now.",
            Self::LogUnavailable => "Read the current run's log, or wait a moment and refresh.",
            Self::Conflict => "Reload to see the latest version, then make the change again.",
            Self::Managed => {
                "Change it where the managing tool reads it, such as Helm values or a Git \
                 repository. Restarting still works here."
            }
            Self::PlanExpired => "Make the plan again.",
            Self::SystemNamespace => "Adopt a namespace of your own instead.",
            Self::Invalid => "Correct the fields the cluster names in the details, then try again.",
            Self::TooManyRequests => "Wait a moment and try again.",
            Self::Unavailable => {
                "Check that the cluster is running and that this machine can reach it."
            }
            Self::Internal => "Try again. If it keeps happening, report it with the details.",
        }
    }

    /// The kind for a Kubernetes `Status`: its `reason` first, its `code` otherwise.
    #[must_use]
    pub fn from_status(reason: &str, code: u16) -> Self {
        match reason {
            "Unauthorized" => Self::Unauthenticated,
            "Forbidden" => Self::Forbidden,
            "NotFound" | "Gone" => Self::NotFound,
            "AlreadyExists" | "Conflict" | "Expired" => Self::Conflict,
            "Invalid" | "NotAcceptable" | "UnsupportedMediaType" | "RequestEntityTooLarge" => {
                Self::Invalid
            }
            "BadRequest" | "MethodNotAllowed" => Self::BadRequest,
            "TooManyRequests" => Self::TooManyRequests,
            "Timeout" | "ServerTimeout" | "ServiceUnavailable" => Self::Unavailable,
            _ => Self::from_code(code),
        }
    }

    const fn from_code(code: u16) -> Self {
        match code {
            401 => Self::Unauthenticated,
            403 => Self::Forbidden,
            404 | 410 => Self::NotFound,
            409 | 412 => Self::Conflict,
            400 | 405 => Self::BadRequest,
            406 | 413 | 415 | 422 => Self::Invalid,
            429 => Self::TooManyRequests,
            502..=504 => Self::Unavailable,
            _ => Self::Internal,
        }
    }
}

/// The answer kube-apiserver gave, unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct KubernetesStatus {
    /// HTTP status code.
    pub code: u16,
    /// Machine-readable reason (`Forbidden`, `NotFound`, …).
    pub reason: String,
    /// kube-apiserver's message.
    pub message: String,
    /// How long kube-apiserver asked clients to wait before retrying
    /// (`details.retryAfterSeconds`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u32>,
    /// Whether kube-apiserver said which object or field it found invalid (`details.name`,
    /// `details.causes`). A JSON Patch that does not apply (a failed `test`, a `remove` of a
    /// path that is gone) is refused with `422` and empty details; a patch whose result is
    /// invalid names the field (observed on k3s v1.36.5).
    #[serde(skip)]
    #[schema(ignore)]
    pub names_invalid_field: bool,
    /// The object and the fields it named in `details`, when it named any. Boxed: every
    /// `Result` that can fail with a status carries it.
    #[serde(skip)]
    #[schema(ignore)]
    pub details: Option<Box<StatusDetails>>,
}

/// What kube-apiserver named in a `Status`'s `details` [api-conventions].
///
/// [api-conventions]: https://github.com/kubernetes/community/blob/master/contributors/devel/sig-architecture/api-conventions.md#response-status-kind
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusDetails {
    /// The object (`details.kind`, `details.name`), when it named one.
    pub object: Option<(String, String)>,
    /// The fields it found invalid (`details.causes` that name a field).
    pub causes: Vec<FieldCause>,
}

/// A field kube-apiserver, or an admission webhook, found invalid: one of the `causes` in its
/// `Status` details [api-conventions].
///
/// [api-conventions]: https://github.com/kubernetes/community/blob/master/contributors/devel/sig-architecture/api-conventions.md#response-status-kind
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FieldCause {
    /// The field, as kube-apiserver writes it (`spec.replicas`,
    /// `spec.template.spec.containers[0].image`).
    pub field: String,
    /// Why, as a machine-readable cause (`FieldValueInvalid`, `FieldValueRequired`, …).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
    /// kube-apiserver's message about the field.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
}

impl KubernetesStatus {
    /// An answer with `code`, `reason` and `message`, and no retry hint or invalid field.
    #[must_use]
    pub fn new(code: u16, reason: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
            message: message.into(),
            retry_after_seconds: None,
            names_invalid_field: false,
            details: None,
        }
    }

    /// Whether kube-apiserver refused the person: `401 Unauthorized` or `403 Forbidden`, which
    /// kube's own `Status::is_forbidden` tells apart from the rest [k8s-codes].
    ///
    /// [k8s-codes]: https://kubernetes.io/docs/reference/using-api/api-concepts/#http-status-codes
    #[must_use]
    pub const fn is_refusal(&self) -> bool {
        matches!(self.code, 401 | 403)
    }

    /// Whether the object, or the API, does not exist: `404 Not Found`, as kube's
    /// `Status::is_not_found`.
    #[must_use]
    pub const fn is_not_found(&self) -> bool {
        self.code == 404
    }
}

/// An RFC 9457 problem, with Zeabur's extension members `kind`, `summary`, `why`, `fix`,
/// `evidence`, `fields`, `kubernetes`, `managedBy`, `denied`, `at` and `applied`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    /// Problem type URI.
    #[serde(rename = "type")]
    pub type_uri: String,
    /// Short summary of the kind.
    pub title: String,
    /// HTTP status code.
    pub status: u16,
    /// What happened, in plain language, for this occurrence.
    pub detail: String,
    /// The category.
    pub kind: Kind,
    /// What happened, naming the object where Zeabur knows it: `problem.summary.{kind}`, with
    /// the values it names (`name`, `namespace`, `resource`, `verb`) as far as they are known.
    pub summary: Message,
    /// Why: the rule that applied, with this occurrence's numbers. Absent when the summary and
    /// fix say all there is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<Message>,
    /// What to do about it.
    pub fix: String,
    /// The fix as a sentence a client words in its own language, where Zeabur wrote `fix` for
    /// this occasion rather than for the kind (the quota to raise, the rule to ask for); absent
    /// when `fix` is the kind's own, which a client words from `kind`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix_key: Option<Message>,
    /// The objects and fields it rests on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<Origin>,
    /// The fields kube-apiserver found invalid, so a form can mark them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldCause>,
    /// kube-apiserver's own answer, when the problem came from the cluster.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kubernetes: Option<KubernetesStatus>,
    /// The tool that manages the object, or owns the field, a write was refused for: the change
    /// belongs in that tool's configuration. A client tells this refusal apart by this member,
    /// not by the wording of `detail`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_by: Option<Tool>,
    /// The request the cluster refused, on a `403` from kube-apiserver to a request Zeabur can
    /// describe. A client names the missing permission from this member, not from the wording
    /// of `kubernetes.message`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denied: Option<Denied>,
    /// Why the container log asked for cannot be read, on a [`Kind::LogUnavailable`]. A client
    /// tells the reason by this member, not by the wording of `detail`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_log: Option<UnavailableLog>,
    /// Where in the request it lies, for a request of several changes or one written as YAML:
    /// the change, and the line of its YAML or the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<ProblemAt>,
    /// The objects an apply wrote before this refusal, in order. A plan of several writes is
    /// not one transaction, so these stay; a client can offer to delete exactly these.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied: Vec<AppliedObject>,
}

/// Where in a request a problem lies.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProblemAt {
    /// The index of the change in the request.
    pub change: usize,
    /// The line of the change's YAML, from 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    /// The field, as kube-apiserver writes it (`metadata.namespace`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

/// An object an apply wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppliedObject {
    /// Its `apiVersion`.
    pub api_version: String,
    /// Its kind.
    pub kind: String,
    /// The plural resource name in its path.
    pub plural: String,
    /// Its name.
    pub name: String,
    /// Its `resourceVersion` after the write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_version: Option<String>,
}

/// Why a container's log cannot be read, from the kubelet's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum UnavailableLog {
    /// The previous run was asked for, but the container has not restarted.
    NoPreviousRun,
    /// The container has not started yet, so it has written nothing.
    NotStarted,
    /// The run's container, and its log with it, is not on the node: the kubelet has removed it
    /// (it keeps only the latest runs of a container), or, right after a restart, does not have
    /// it yet.
    Removed,
}

impl UnavailableLog {
    /// What happened, about `what` (for example "the log of pod web-1").
    #[must_use]
    pub fn detail(self, what: &str) -> String {
        match self {
            Self::NoPreviousRun => {
                format!("There is no previous run for {what}: the container has not restarted.")
            }
            Self::NotStarted => format!("There is no {what} yet: the container has not started."),
            Self::Removed => format!("The cluster no longer has that run's container or {what}."),
        }
    }

    /// [`UnavailableLog::fix`] as a sentence key.
    #[must_use]
    pub fn fix_key(self) -> Message {
        Message::new(match self {
            Self::NoPreviousRun => "problem.logUnavailable.fix.noPreviousRun",
            Self::NotStarted => "problem.logUnavailable.fix.notStarted",
            Self::Removed => "problem.logUnavailable.fix.removed",
        })
    }

    /// What the person can do about it.
    #[must_use]
    pub const fn fix(self) -> &'static str {
        match self {
            Self::NoPreviousRun => "Read the current run's log instead.",
            Self::NotStarted => {
                "Wait for the container to start; the replica's events say what it is waiting for."
            }
            Self::Removed => {
                "Wait a moment and refresh. If it stays unavailable, read the current run's log. \
                 To keep logs past restarts, send them to a log store."
            }
        }
    }
}

impl Problem {
    /// A problem of `kind` with a plain-language `detail`.
    #[must_use]
    pub fn new(kind: Kind, detail: impl Into<String>) -> Self {
        Self {
            type_uri: kind.type_uri().to_owned(),
            title: kind.title().to_owned(),
            status: kind.status(),
            detail: detail.into(),
            kind,
            summary: Message::new(&format!("problem.summary.{}", kind.name())),
            why: None,
            fix: kind.fix().to_owned(),
            fix_key: None,
            evidence: Vec::new(),
            fields: Vec::new(),
            kubernetes: None,
            managed_by: None,
            denied: None,
            unavailable_log: None,
            at: None,
            applied: Vec::new(),
        }
    }

    /// The problem for a container log the kubelet cannot give, for `reason`, with
    /// kube-apiserver's answer, about `what` (for example "the log of pod web-1").
    #[must_use]
    pub fn log_unavailable(reason: UnavailableLog, status: KubernetesStatus, what: &str) -> Self {
        let mut problem = Self {
            kubernetes: Some(status),
            unavailable_log: Some(reason),
            ..Self::new(Kind::LogUnavailable, reason.detail(what))
        };
        problem.fix_for_occasion(reason.fix(), reason.fix_key());
        problem
    }

    /// The seconds to send in `Retry-After`: kube-apiserver's hint, only on `429` and `503`.
    #[must_use]
    pub fn retry_after(&self) -> Option<u32> {
        matches!(self.status, 429 | 503)
            .then(|| self.kubernetes.as_ref()?.retry_after_seconds)
            .flatten()
    }

    /// The problem for a Kubernetes `Status` answer, about `what` (for example "pods in
    /// namespace shop").
    #[must_use]
    pub fn from_kubernetes(status: KubernetesStatus, what: &str) -> Self {
        let kind = Kind::from_status(&status.reason, status.code);
        let detail = match kind {
            Kind::Forbidden => format!("The cluster does not let you read or change {what}."),
            // Nor with Zeabur's own sign-in kinds.
            Kind::ElevationRequired | Kind::QuotaExceeded | Kind::PolicyDenied => {
                format!("The cluster does not let you change {what}.")
            }
            Kind::Unauthenticated | Kind::SignedOut => {
                "The cluster did not accept your credentials. They may have expired.".to_owned()
            }
            Kind::NotFound => format!("{what} could not be found."),
            // kube-apiserver never answers with Zeabur's own kind.
            Kind::Conflict | Kind::Managed => format!("{what} changed after you loaded it."),
            Kind::Invalid => format!("The cluster rejected the change to {what}."),
            Kind::TooManyRequests => "The cluster asked Zeabur to slow down.".to_owned(),
            Kind::Unavailable => "The cluster did not answer in time.".to_owned(),
            // Nor with a plan's expiry, a system namespace's refusal or a missing log, which are
            // Zeabur's.
            Kind::BadRequest
            | Kind::PlanExpired
            | Kind::SystemNamespace
            | Kind::LogUnavailable
            | Kind::Internal => {
                format!("The cluster answered unexpectedly about {what}.")
            }
        };
        let mut problem = Self::new(kind, detail);
        let details = status.details.as_deref().cloned().unwrap_or_default();
        if let Some((object_kind, name)) = &details.object {
            problem.summary = problem
                .summary
                .with("resource", object_kind)
                .with("name", name);
            problem.evidence.push(Origin::object(object_kind, name));
        }
        problem.fields = details.causes;
        Self {
            kubernetes: Some(status),
            ..problem
        }
    }

    /// Records the request the cluster refused (`denied`), and names it in the summary.
    pub fn deny(&mut self, denied: Denied) {
        let params = &mut self.summary.params;
        params.insert("verb".to_owned(), denied.verb.clone());
        params.insert("resource".to_owned(), denied.resource.clone());
        for (name, value) in [("name", &denied.name), ("namespace", &denied.namespace)] {
            if let Some(value) = value {
                params.insert(name.to_owned(), value.clone());
            }
        }
        self.denied = Some(denied);
    }

    /// Sets a fix written for this occasion: `fix` in English, and `key` for clients to word.
    pub fn fix_for_occasion(&mut self, fix: impl Into<String>, key: Message) {
        self.fix = fix.into();
        self.fix_key = Some(key);
    }

    /// This problem as `kind`: its type, title, status, fix (and `fixKey`) and summary key become
    /// the new kind's; its detail, the values its summary names, its evidence, fields and
    /// kube-apiserver's answer stay.
    #[must_use]
    pub fn refined(self, kind: Kind) -> Self {
        let summary = Message {
            params: self.summary.params,
            ..Message::new(&format!("problem.summary.{}", kind.name()))
        };
        Self {
            summary,
            why: self.why,
            evidence: self.evidence,
            fields: self.fields,
            kubernetes: self.kubernetes,
            denied: self.denied,
            managed_by: self.managed_by,
            unavailable_log: self.unavailable_log,
            at: self.at,
            applied: self.applied,
            ..Self::new(kind, self.detail)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(code: u16, reason: &str) -> KubernetesStatus {
        KubernetesStatus::new(code, reason, format!("{reason} from kube-apiserver"))
    }

    #[test]
    fn a_forbidden_answer_keeps_the_original_and_explains_it() {
        let problem = Problem::from_kubernetes(status(403, "Forbidden"), "pods in namespace shop");
        assert_eq!(problem.status, 403);
        assert_eq!(problem.kind, Kind::Forbidden);
        assert!(problem.detail.contains("pods in namespace shop"));
        assert_eq!(
            problem.kubernetes.as_ref().map(|k| k.message.as_str()),
            Some("Forbidden from kube-apiserver")
        );
        let json = serde_json::to_value(&problem).unwrap();
        assert_eq!(json["type"], "/problems/forbidden");
        assert_eq!(json["kubernetes"]["reason"], "Forbidden");
        // Only the caller knows the request; the answer alone does not name it.
        assert!(json.get("denied").is_none());
    }

    #[test]
    fn the_summary_names_the_object_and_the_refused_request() {
        let invalid = KubernetesStatus {
            details: Some(Box::new(StatusDetails {
                object: Some(("Deployment".to_owned(), "web".to_owned())),
                causes: vec![FieldCause {
                    field: "spec.replicas".to_owned(),
                    reason: "FieldValueInvalid".to_owned(),
                    message: "must be greater than or equal to 0".to_owned(),
                }],
            })),
            ..status(422, "Invalid")
        };
        let json =
            serde_json::to_value(Problem::from_kubernetes(invalid, "Deployment web")).unwrap();
        assert_eq!(
            json["summary"],
            serde_json::json!({"key": "problem.summary.invalid",
                               "params": {"resource": "Deployment", "name": "web"}})
        );
        assert_eq!(json["evidence"][0]["name"], "web");
        assert_eq!(json["fields"][0]["field"], "spec.replicas");
        assert!(json.get("why").is_none());

        let mut refused = Problem::from_kubernetes(status(403, "Forbidden"), "Deployment web");
        refused.deny(Denied {
            verb: "patch".to_owned(),
            group: "apps".to_owned(),
            resource: "deployments".to_owned(),
            name: Some("web".to_owned()),
            namespace: Some("shop".to_owned()),
        });
        let elevated = refused.refined(Kind::ElevationRequired);
        assert_eq!(elevated.summary.key, "problem.summary.elevation-required");
        assert_eq!(
            elevated.summary.params.get("verb").map(String::as_str),
            Some("patch")
        );
        assert_eq!(elevated.status, 403);
        assert!(elevated.denied.is_some() && elevated.kubernetes.is_some());
    }

    #[test]
    fn an_expired_plan_has_its_own_type_on_a_400() {
        let json =
            serde_json::to_value(Problem::new(Kind::PlanExpired, "the plan expired")).unwrap();
        assert_eq!(json["type"], "/problems/plan-expired");
        assert_eq!(json["kind"], "plan-expired");
        assert_eq!(json["status"], 400);
    }

    #[test]
    fn retry_after_is_kube_apiservers_hint_on_429_and_503_only() {
        let busy = KubernetesStatus {
            retry_after_seconds: Some(3),
            ..status(429, "TooManyRequests")
        };
        assert_eq!(
            Problem::from_kubernetes(busy.clone(), "pods").retry_after(),
            Some(3)
        );
        let forbidden = KubernetesStatus {
            retry_after_seconds: Some(3),
            ..status(403, "Forbidden")
        };
        assert_eq!(
            Problem::from_kubernetes(forbidden, "pods").retry_after(),
            None
        );
        let json = serde_json::to_value(Problem::from_kubernetes(busy, "pods")).unwrap();
        assert_eq!(json["kubernetes"]["retryAfterSeconds"], 3);
        assert!(json.get("reason").is_none());
    }

    #[test]
    fn an_unknown_reason_falls_back_to_the_code() {
        assert_eq!(Kind::from_status("", 429), Kind::TooManyRequests);
        assert_eq!(Kind::from_status("Whatever", 503), Kind::Unavailable);
        assert_eq!(Kind::from_status("", 418), Kind::Internal);
    }

    /// Exhaustive: a new kind does not compile until it has an index here, which is the
    /// reminder to add it to [`Kind::ALL`] at that index (the test checks the two agree).
    const fn position(kind: Kind) -> usize {
        match kind {
            Kind::BadRequest => 0,
            Kind::Unauthenticated => 1,
            Kind::Forbidden => 2,
            Kind::NotFound => 3,
            Kind::Conflict => 4,
            Kind::Invalid => 5,
            Kind::TooManyRequests => 6,
            Kind::Unavailable => 7,
            Kind::Internal => 8,
            Kind::Managed => 9,
            Kind::PlanExpired => 10,
            Kind::SystemNamespace => 11,
            Kind::SignedOut => 12,
            Kind::ElevationRequired => 13,
            Kind::LogUnavailable => 14,
            Kind::QuotaExceeded => 15,
            Kind::PolicyDenied => 16,
        }
    }

    /// A client that does not know a kind treats it by its status; Zeabur's own refinements
    /// read back as the kinds they refine.
    #[test]
    fn every_kind_survives_the_round_trip_through_its_status() {
        for (index, kind) in Kind::ALL.into_iter().enumerate() {
            assert_eq!(position(kind), index, "{kind:?}");
            let expected = match kind {
                Kind::Managed => Kind::Conflict,
                Kind::PlanExpired => Kind::BadRequest,
                Kind::SystemNamespace => Kind::Invalid,
                Kind::SignedOut => Kind::Unauthenticated,
                Kind::ElevationRequired | Kind::QuotaExceeded | Kind::PolicyDenied => {
                    Kind::Forbidden
                }
                Kind::LogUnavailable => Kind::NotFound,
                Kind::BadRequest
                | Kind::Unauthenticated
                | Kind::Forbidden
                | Kind::NotFound
                | Kind::Conflict
                | Kind::Invalid
                | Kind::TooManyRequests
                | Kind::Unavailable
                | Kind::Internal => kind,
            };
            assert_eq!(Kind::from_code(kind.status()), expected, "{kind:?}");
        }
    }
}
