//! Who reapplies the fields they own; the ownership itself is read with
//! [`engawa_graph::fields`].

use engawa_adapters::provenance::Tool;
pub use engawa_graph::fields::{Step, owners};

/// The tool behind a manager that reapplies what it owns: a field it owns changed by hand comes
/// back on its next sync or upgrade, so engawa never takes one over.
///
/// - Helm 3 writes as `helm` [helm-manager]; Flux's helm-controller and kustomize-controller as
///   `helm-controller` and `kustomize-controller` [flux-ssa]; Argo CD as `argocd-controller`
///   (server-side apply) or `argocd-application-controller` [argocd-ssa].
///
/// [helm-manager]: https://github.com/helm/helm/blob/main/pkg/kube/client.go
/// [flux-ssa]: https://fluxcd.io/flux/components/kustomize/kustomizations/#server-side-apply
/// [argocd-ssa]: https://argo-cd.readthedocs.io/en/stable/user-guide/sync-options/#server-side-apply
#[must_use]
pub fn reapplying_tool(manager: &str) -> Option<Tool> {
    match manager {
        "helm" => Some(Tool::Helm),
        "kustomize-controller" | "helm-controller" => Some(Tool::Flux),
        _ if manager.starts_with("argocd") => Some(Tool::ArgoCd),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_that_reapply_are_known_by_their_manager() {
        assert_eq!(reapplying_tool("argocd-controller"), Some(Tool::ArgoCd));
        assert_eq!(reapplying_tool("kubectl-client-side-apply"), None);
    }
}
