//! Which deployment tool manages an object, from the labels and annotations it writes.

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

use crate::label;

/// The recommended label naming the tool that manages an object, read verbatim.
pub const MANAGED_BY_LABEL: &str = "app.kubernetes.io/managed-by";

/// Argo CD's annotation-based resource tracking.
pub const ARGO_CD_TRACKING_ID_ANNOTATION: &str = "argocd.argoproj.io/tracking-id";
/// Argo CD's label-based resource tracking.
pub const ARGO_CD_INSTANCE_LABEL: &str = "argocd.argoproj.io/instance";
/// Set by Flux's kustomize-controller on the objects a `Kustomization` applies.
pub const FLUX_KUSTOMIZATION_LABEL: &str = "kustomize.toolkit.fluxcd.io/name";
/// Set by Flux's helm-controller on the objects a `HelmRelease` installs.
pub const FLUX_HELM_RELEASE_LABEL: &str = "helm.toolkit.fluxcd.io/name";
/// Set by Helm on every object of a release.
pub const HELM_RELEASE_NAME_ANNOTATION: &str = "meta.helm.sh/release-name";
/// The `type` of the Secrets Helm's default storage driver keeps release records in.
pub const HELM_RELEASE_SECRET_TYPE: &str = "helm.sh/release.v1";

pub use zeabur_core::tool::Tool;

/// The tool managing an object, if its marks are present.
///
/// The GitOps tools are checked before Helm: Flux installs charts with Helm, and Argo CD can
/// render them, so a Helm mark beside theirs means the GitOps tool is the one that reapplies.
#[must_use]
pub fn detect_tool(meta: &ObjectMeta) -> Option<Tool> {
    let labels = meta.labels.as_ref();
    let annotations = meta.annotations.as_ref();
    if label(annotations, ARGO_CD_TRACKING_ID_ANNOTATION).is_some()
        || label(labels, ARGO_CD_INSTANCE_LABEL).is_some()
    {
        Some(Tool::ArgoCd)
    } else if label(labels, FLUX_KUSTOMIZATION_LABEL).is_some()
        || label(labels, FLUX_HELM_RELEASE_LABEL).is_some()
    {
        Some(Tool::Flux)
    } else if label(annotations, HELM_RELEASE_NAME_ANNOTATION).is_some()
        || managed_by(meta).is_some_and(|tool| tool.eq_ignore_ascii_case("Helm"))
    {
        Some(Tool::Helm)
    } else {
        None
    }
}

/// The [`MANAGED_BY_LABEL`], verbatim.
#[must_use]
pub fn managed_by(meta: &ObjectMeta) -> Option<&str> {
    label(meta.labels.as_ref(), MANAGED_BY_LABEL)
}

/// Whether a Secret of this `type` is one of Helm's release records rather than configuration.
#[must_use]
pub fn is_helm_release_secret(secret_type: &str) -> bool {
    secret_type == HELM_RELEASE_SECRET_TYPE
}
