//! Members, kept in Zeabur Identity's custom resources. One's own account is not read here (see
//! [`crate::account`]).
//!
//! The people Zeabur Identity signs in, their passkeys and their one-time setup links are read
//! and changed through `identity.zeabur.com/v1alpha1` (`User`, `Credential`, `Invitation`,
//! `Grant`) in its namespace.
//!
//! # Rights
//!
//! Like everything else, every read and write goes to kube-apiserver as the person, so RBAC
//! decides: Zeabur Identity's install gives `zeabur:admins` the `view` role, which does not
//! reach these custom resources, and `zeabur:elevated:admins` `cluster-admin`, so in practice an
//! administrator manages members, and everyone else sees the refusal with the rule it needs.
//! A member who is not an admin has the rights of the [`access`] bindings: Developer or Viewer in
//! chosen projects, and the names of nodes and projects. RBAC cannot narrow a `list` to the
//! objects labelled with one's name ([k8s-rbac-names]), which is why one's own passkeys are
//! managed at the issuer instead.
//!
//! # Setup links
//!
//! A setup link is an `Invitation`, made as `zeabur-identityctl invite` makes one: a token
//! `<invitation name>.<32 random bytes, base64url>`, of which only `sha256:<hex>` is stored in
//! `spec.tokenHash`, and the link `<issuer>/setup#token=<token>`, the token in the fragment so it
//! never reaches a server log. Zeabur Identity redeems it with `--invitations` only; the answer
//! says whether its Deployment has that, where it can be read. An invitation for a user who
//! already has passkeys adds one more; a reset first deletes the user's invitations, credentials
//! and grants, in that order (each invitation could enrol a passkey into the recovered account),
//! as `zeabur-identityctl setup-link --reset` does.
//!
//! # Removing
//!
//! A member is removed as Zeabur Identity's documentation says: their invitations, credentials
//! and grants first, then the `User`, so no credential outlives the user and signs in as a new
//! user of the same name. Nobody removes themselves here, nor their own last passkey: admin rights
//! need a passkey, and one lost device must not lock the only administrator out.
//!
//! # Labels and last use
//!
//! A `Credential`'s `spec` is immutable. Zeabur Identity keeps the name given after enrolment in
//! `status.label`, which takes the place of `spec.passkey.label`. It keeps the last passkey
//! sign-in in `status.lastUsedAt` (Unix seconds). A member's last sign-in is the latest of
//! those times and of their grants' `authTime`.
//!
//! [k8s-rbac-names]: https://kubernetes.io/docs/reference/access-authn-authz/rbac/#referring-to-resources

pub mod access;
mod directory;
pub(crate) mod handlers;
pub(crate) mod plan;
mod writes;

pub use directory::{
    InvitationView, LinkedAccountView, MemberRole, MemberView, MembersView, PasskeyView, SetupLinks,
};
pub use plan::{
    MemberApplied, MemberApplyRequest, MemberChange, MemberPlan, MemberWrite, MemberWriteAction,
};
pub use writes::{SetupLinkRequest, SetupLinkView};

/// Zeabur Identity's API group.
pub const GROUP: &str = "identity.zeabur.com";
/// Its version.
pub const VERSION: &str = "v1alpha1";
/// `apiVersion` of its objects.
pub const API_VERSION: &str = "identity.zeabur.com/v1alpha1";
/// The namespace Zeabur Identity installs into by default.
pub const DEFAULT_NAMESPACE: &str = "zeabur-identity";
/// Label naming the `User` an invitation, credential or grant belongs to.
pub const USER_LABEL: &str = "identity.zeabur.com/user";
/// Annotation holding an invitation's expiry, Unix seconds.
pub const EXPIRES_AT_ANNOTATION: &str = "identity.zeabur.com/expires-at";
/// Label selecting expiring objects for Zeabur Identity's garbage collector: the expiry hour.
pub const EXPIRY_BUCKET_LABEL: &str = "identity.zeabur.com/expiry-bucket";
/// The group Zeabur Identity's install binds: `zeabur:admins` reads the cluster, and
/// `zeabur:elevated:admins` is `cluster-admin`.
pub const ADMIN_GROUP: &str = "admins";
/// How Kubernetes sees a Zeabur Identity user: `zeabur:<username>`.
pub const USER_PREFIX: &str = "zeabur:";
/// The Identity Deployment, read for its issuer and whether it redeems invitations.
pub const DEPLOYMENT: &str = "zeabur-identity";
