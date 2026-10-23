//! Which Pod Security Standards level a pod spec meets [k8s-pss].
//!
//! The checks are the standards' controls that a pod template can break, read from the spec as
//! written: host namespaces, privileged containers, capabilities, `hostPath` volumes and host
//! ports, and the seccomp profile (baseline); volume types, privilege escalation, running as
//! non-root, `runAsUser` and dropping every capability (restricted). AppArmor, SELinux,
//! `/proc` mount types and sysctls are not checked: a template that sets them is rare, and
//! Pod Security admission still has the final word. Running as root is decided by the image
//! when nothing sets `runAsUser`; only `runAsNonRoot` makes the kubelet check it.
//!
//! [k8s-pss]: https://kubernetes.io/docs/concepts/security/pod-security-standards/

use k8s_openapi::api::core::v1::{Container, PodSpec, SeccompProfile, Volume};
use zeabur_model::PodSecurityLevel;

/// A control of the standards a pod spec breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Breach {
    /// The level it breaks; it meets every level below.
    pub breaks: PodSecurityLevel,
    /// The field, as the standards name it (`hostNetwork`, `runAsNonRoot`).
    pub field: &'static str,
}

/// The capabilities baseline lets a container add.
const BASELINE_CAPABILITIES: &[&str] = &[
    "AUDIT_WRITE",
    "CHOWN",
    "DAC_OVERRIDE",
    "FOWNER",
    "FSETID",
    "KILL",
    "MKNOD",
    "NET_BIND_SERVICE",
    "SETFCAP",
    "SETGID",
    "SETPCAP",
    "SETUID",
    "SYS_CHROOT",
];

fn containers(spec: &PodSpec) -> impl Iterator<Item = &Container> {
    spec.init_containers
        .iter()
        .flatten()
        .chain(&spec.containers)
}

const fn seccomp_type(profile: &SeccompProfile) -> &str {
    profile.type_.as_str()
}

/// The control a volume breaks: a `hostPath` breaks baseline, any type but the restricted
/// standard's list breaks restricted.
const fn volume_breach(volume: &Volume) -> Option<Breach> {
    if volume.host_path.is_some() {
        return Some(Breach {
            breaks: PodSecurityLevel::Baseline,
            field: "hostPath",
        });
    }
    let allowed = volume.config_map.is_some()
        || volume.csi.is_some()
        || volume.downward_api.is_some()
        || volume.empty_dir.is_some()
        || volume.ephemeral.is_some()
        || volume.persistent_volume_claim.is_some()
        || volume.projected.is_some()
        || volume.secret.is_some();
    if allowed {
        None
    } else {
        Some(Breach {
            breaks: PodSecurityLevel::Restricted,
            field: "volumes",
        })
    }
}

/// The controls `spec` breaks, baseline's first.
#[must_use]
pub fn breaches(spec: &PodSpec) -> Vec<Breach> {
    use PodSecurityLevel::{Baseline, Restricted};
    let mut out = Vec::new();
    let mut breach = |breaks, field| {
        if !out.contains(&Breach { breaks, field }) {
            out.push(Breach { breaks, field });
        }
    };
    let pod = spec.security_context.as_ref();
    for (set, field) in [
        (spec.host_network, "hostNetwork"),
        (spec.host_pid, "hostPID"),
        (spec.host_ipc, "hostIPC"),
    ] {
        if set == Some(true) {
            breach(Baseline, field);
        }
    }
    let pod_seccomp = pod
        .and_then(|p| p.seccomp_profile.as_ref())
        .map(seccomp_type);
    if pod_seccomp == Some("Unconfined") {
        breach(Baseline, "seccompProfile");
    }
    for container in containers(spec) {
        let context = container.security_context.as_ref();
        if context.and_then(|c| c.privileged) == Some(true) {
            breach(Baseline, "privileged");
        }
        let added = context
            .and_then(|c| c.capabilities.as_ref())
            .and_then(|c| c.add.as_ref())
            .into_iter()
            .flatten();
        for capability in added {
            if !BASELINE_CAPABILITIES.contains(&capability.as_str()) {
                breach(Baseline, "capabilities");
            } else if capability != "NET_BIND_SERVICE" {
                breach(Restricted, "capabilities");
            }
        }
        let ports = container.ports.iter().flatten();
        if ports
            .into_iter()
            .any(|p| p.host_port.is_some_and(|h| h != 0))
        {
            breach(Baseline, "hostPort");
        }
        let seccomp = context
            .and_then(|c| c.seccomp_profile.as_ref())
            .map(seccomp_type);
        if seccomp == Some("Unconfined") {
            breach(Baseline, "seccompProfile");
        }
    }
    for volume in spec.volumes.iter().flatten() {
        if let Some(field) = volume_breach(volume) {
            breach(field.breaks, field.field);
        }
    }
    let pod_non_root = pod.and_then(|p| p.run_as_non_root) == Some(true);
    let confined = |t: Option<&str>| matches!(t, Some("RuntimeDefault" | "Localhost"));
    if pod.and_then(|p| p.run_as_user) == Some(0) {
        breach(Restricted, "runAsUser");
    }
    for container in containers(spec) {
        let context = container.security_context.as_ref();
        if context.and_then(|c| c.allow_privilege_escalation) != Some(false) {
            breach(Restricted, "allowPrivilegeEscalation");
        }
        let non_root = context.and_then(|c| c.run_as_non_root);
        if non_root == Some(false) || (non_root.is_none() && !pod_non_root) {
            breach(Restricted, "runAsNonRoot");
        }
        if context.and_then(|c| c.run_as_user) == Some(0) {
            breach(Restricted, "runAsUser");
        }
        let seccomp = context
            .and_then(|c| c.seccomp_profile.as_ref())
            .map(seccomp_type);
        if !(confined(seccomp) || seccomp.is_none() && confined(pod_seccomp)) {
            breach(Restricted, "seccompProfile");
        }
        let drops_all = context
            .and_then(|c| c.capabilities.as_ref())
            .and_then(|c| c.drop.as_ref())
            .is_some_and(|drop| drop.iter().any(|d| d == "ALL"));
        if !drops_all {
            breach(Restricted, "capabilities");
        }
    }
    out
}

/// The strictest level `spec` meets.
#[must_use]
pub fn level(spec: &PodSpec) -> PodSecurityLevel {
    breaches(spec)
        .iter()
        .map(|b| match b.breaks {
            PodSecurityLevel::Restricted => PodSecurityLevel::Baseline,
            PodSecurityLevel::Baseline | PodSecurityLevel::Privileged => {
                PodSecurityLevel::Privileged
            }
        })
        .min()
        .unwrap_or(PodSecurityLevel::Restricted)
}

#[cfg(test)]
mod tests {
    use k8s_openapi::api::core::v1::PodSpec;
    use serde_json::json;
    use zeabur_model::PodSecurityLevel;

    use super::{breaches, level};

    fn spec(value: serde_json::Value) -> PodSpec {
        serde_json::from_value(value).unwrap()
    }

    fn restricted() -> serde_json::Value {
        json!({
            "securityContext": {"runAsNonRoot": true, "seccompProfile": {"type": "RuntimeDefault"}},
            "containers": [{"name": "app", "image": "app", "securityContext": {
                "allowPrivilegeEscalation": false, "capabilities": {"drop": ["ALL"]}}}],
            "volumes": [{"name": "data", "persistentVolumeClaim": {"claimName": "data"}}]
        })
    }

    #[test]
    fn a_hardened_template_meets_restricted_and_a_plain_one_baseline() {
        assert_eq!(level(&spec(restricted())), PodSecurityLevel::Restricted);
        let plain = spec(json!({"containers": [{"name": "nginx", "image": "nginx"}]}));
        assert_eq!(level(&plain), PodSecurityLevel::Baseline);
        let fields: Vec<&str> = breaches(&plain).iter().map(|b| b.field).collect();
        assert_eq!(
            fields,
            [
                "allowPrivilegeEscalation",
                "runAsNonRoot",
                "seccompProfile",
                "capabilities"
            ]
        );
    }

    #[test]
    fn each_container_may_carry_what_the_pod_leaves_out() {
        let mut value = restricted();
        value["securityContext"] = json!({});
        value["containers"][0]["securityContext"]["runAsNonRoot"] = json!(true);
        value["containers"][0]["securityContext"]["seccompProfile"] =
            json!({"type": "Localhost", "localhostProfile": "p.json"});
        assert_eq!(level(&spec(value.clone())), PodSecurityLevel::Restricted);
        value["initContainers"] = json!([{"name": "init", "image": "busybox"}]);
        assert_eq!(level(&spec(value)), PodSecurityLevel::Baseline);
    }

    #[test]
    fn root_and_extra_capabilities_break_restricted_and_host_access_breaks_baseline() {
        let mut root = restricted();
        root["containers"][0]["securityContext"]["runAsUser"] = json!(0);
        assert_eq!(level(&spec(root)), PodSecurityLevel::Baseline);
        let mut chown = restricted();
        chown["containers"][0]["securityContext"]["capabilities"]["add"] = json!(["CHOWN"]);
        assert_eq!(level(&spec(chown)), PodSecurityLevel::Baseline);
        let mut bind = restricted();
        bind["containers"][0]["securityContext"]["capabilities"]["add"] =
            json!(["NET_BIND_SERVICE"]);
        assert_eq!(level(&spec(bind)), PodSecurityLevel::Restricted);
        let mut host = restricted();
        host["hostNetwork"] = json!(true);
        let mut host_path = restricted();
        host_path["volumes"] = json!([{"name": "h", "hostPath": {"path": "/"}}]);
        let mut privileged = restricted();
        privileged["containers"][0]["securityContext"]["privileged"] = json!(true);
        let mut admin = restricted();
        admin["containers"][0]["securityContext"]["capabilities"]["add"] = json!(["SYS_ADMIN"]);
        let mut port = restricted();
        port["containers"][0]["ports"] = json!([{"containerPort": 80, "hostPort": 80}]);
        for value in [host, host_path, privileged, admin, port] {
            assert_eq!(
                level(&spec(value.clone())),
                PodSecurityLevel::Privileged,
                "{value}"
            );
        }
    }

    #[test]
    fn a_host_volume_breaks_baseline_and_another_kind_restricted() {
        let mut nfs = restricted();
        nfs["volumes"] = json!([{"name": "n", "nfs": {"server": "s", "path": "/"}}]);
        assert_eq!(level(&spec(nfs)), PodSecurityLevel::Baseline);
    }
}
