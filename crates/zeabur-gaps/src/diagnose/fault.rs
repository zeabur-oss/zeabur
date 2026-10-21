//! Where each diagnosis's cause lies and who can remove it, by the sentence that states it.
//!
//! A person must not read a full node or an unreachable registry as a bug of their own.
//!
//! - A program that exits, or a Service with no ready pod because of it, is the **code**.
//! - A command, image name, probe, port, variable, limit or node selector the service sets is its
//!   **configuration**; whoever may change the service fixes both.
//! - No node with room, and a `ResourceQuota`, are the **cluster**'s: a cluster administrator adds
//!   nodes or raises the quota. A container killed for memory without a limit of its own ran the
//!   node out of memory, the cluster's, though setting a limit is the service's fix.
//! - A registry that throttles, cannot be reached or answers what Zeabur cannot read is
//!   **upstream**, named by its host; a registry the nodes cannot reach is the cluster
//!   administrator's to fix.
//! - An ingress controller with no ready replica is that **component**'s, fixed by whoever runs it;
//!   a certificate cert-manager has not issued is cert-manager's to report, though the cause (a DNS
//!   record, a challenge the domain does not answer) is usually the service's to fix.
//! - The fallback says nothing about where the cause lies: **unknown**.

use zeabur_model::{DiagnosisKind, Fault, FaultOrigin, Fixer, Message};

/// The fault of a diagnosis of `kind` whose cause is `cause`.
pub(crate) fn fault(kind: DiagnosisKind, cause: &Message) -> Fault {
    use FaultOrigin::{Cluster, Code, Configuration, Unknown, Upstream};
    use Fixer::{ClusterAdmin, You};
    let variant = cause.key.rsplit('.').next().unwrap_or_default();
    match kind {
        DiagnosisKind::CrashLoop | DiagnosisKind::RunFailed => match variant {
            "exitCode" => Fault::new(Code, You),
            _ => Fault::new(Configuration, You),
        },
        DiagnosisKind::OutOfMemory => match variant {
            "noLimit" => Fault::new(Cluster, You),
            _ => Fault::new(Configuration, You),
        },
        DiagnosisKind::ImagePull => {
            let registry = cause.params.get("image").map(|image| registry(image));
            let upstream = |fixer| {
                let fault = Fault::new(Upstream, fixer);
                match &registry {
                    Some(host) => fault.of(host.clone()),
                    None => fault,
                }
            };
            match variant {
                "rateLimited" | "unknown" => upstream(You),
                "unreachable" => upstream(ClusterAdmin),
                _ => Fault::new(Configuration, You),
            }
        }
        DiagnosisKind::ProbeFailure | DiagnosisKind::ConfigMissing => {
            Fault::new(Configuration, You)
        }
        DiagnosisKind::Unreachable => match variant {
            "noReadyEndpoints" => Fault::new(Code, You),
            "controllerDown" => {
                Fault::new(FaultOrigin::Component, Fixer::ComponentOwner).of("ingress-controller")
            }
            "certificateNotReady" => Fault::new(FaultOrigin::Component, You).of("cert-manager"),
            _ => Fault::new(Configuration, You),
        },
        DiagnosisKind::QuotaBlocked => Fault::new(Cluster, ClusterAdmin),
        DiagnosisKind::Unschedulable => match variant {
            "placement" => Fault::new(Configuration, You),
            _ => Fault::new(Cluster, ClusterAdmin),
        },
        DiagnosisKind::Unexplained => Fault::new(Unknown, You),
    }
}

/// The registry an image reference names: its first segment when that is a host (it has a dot or
/// a port, or is `localhost`), else Docker Hub, as the container runtimes resolve a short name
/// [distribution-reference].
///
/// [distribution-reference]: https://github.com/distribution/reference/blob/main/normalize.go
fn registry(image: &str) -> String {
    match image.split_once('/') {
        Some((first, _)) if first.contains(['.', ':']) || first == "localhost" => first.to_owned(),
        _ => "docker.io".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_names_its_registry() {
        assert_eq!(registry("nginx:1.29"), "docker.io");
        assert_eq!(registry("library/nginx"), "docker.io");
        assert_eq!(registry("ghcr.io/example/blog:2.4.1"), "ghcr.io");
        assert_eq!(registry("registry.local:5000/app"), "registry.local:5000");
        assert_eq!(registry("localhost/app"), "localhost");
    }

    #[test]
    fn every_sentence_has_a_fault_and_only_the_fallback_is_unknown() {
        let cases = [
            (
                DiagnosisKind::CrashLoop,
                "diagnosis.crashLoop.cause.exitCode",
                FaultOrigin::Code,
            ),
            (
                DiagnosisKind::CrashLoop,
                "diagnosis.crashLoop.cause.commandNotFound",
                FaultOrigin::Configuration,
            ),
            (
                DiagnosisKind::OutOfMemory,
                "diagnosis.outOfMemory.cause.noLimit",
                FaultOrigin::Cluster,
            ),
            (
                DiagnosisKind::ImagePull,
                "diagnosis.imagePull.cause.rateLimited",
                FaultOrigin::Upstream,
            ),
            (
                DiagnosisKind::ImagePull,
                "diagnosis.imagePull.cause.notFound",
                FaultOrigin::Configuration,
            ),
            (
                DiagnosisKind::Unschedulable,
                "diagnosis.unschedulable.cause.memory",
                FaultOrigin::Cluster,
            ),
            (
                DiagnosisKind::QuotaBlocked,
                "diagnosis.quotaBlocked.cause",
                FaultOrigin::Cluster,
            ),
            (
                DiagnosisKind::Unreachable,
                "diagnosis.unreachable.cause.targetPort",
                FaultOrigin::Configuration,
            ),
            (
                DiagnosisKind::Unexplained,
                "diagnosis.unexplained.cause.waiting",
                FaultOrigin::Unknown,
            ),
        ];
        for (kind, key, origin) in cases {
            assert_eq!(fault(kind, &Message::new(key)).origin, origin, "{key}");
        }
        let pull = Message::new("diagnosis.imagePull.cause.unreachable")
            .with("image", "registry.example.test/app:1");
        assert_eq!(
            fault(DiagnosisKind::ImagePull, &pull),
            Fault::new(FaultOrigin::Upstream, Fixer::ClusterAdmin).of("registry.example.test")
        );
    }
}
