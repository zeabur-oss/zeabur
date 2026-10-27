//! The plan of a personal install: `KubeSolo`, the standalone helm-controller and the components.
//!
//! There is no Zeabur panel or Zeabur Identity. Zeabur Builder, with its zot, is an option. The
//! person runs `zeaburctl ui` on their own computer with the admin kubeconfig. Nothing is isolated:
//! `KubeSolo`'s bridge CNI enforces no `NetworkPolicy`, there is one admin and one node. The plan
//! makes no network policies, and every object it makes is one `kubectl apply` can make and
//! `kubectl delete` can remove without the installer.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::catalog::Index;
use crate::cluster::HostFile;
use crate::config::PersonalConfig;
use crate::pins::{HELM_CONTROLLER, KUBECTL, KUBESOLO};
use crate::plan::{
    Choices, ComponentPlan, Download, Facts, Memory, Namespace, Warning, components, idle_memory,
    warning, zot_cluster_ip,
};
use crate::remove::Removal;
use crate::select::{self, BUILD_CAPABILITIES, PERSONAL_DISTRIBUTION, PERSONAL_PROFILE};
use crate::storage::Storage;
use crate::{PlanError, kubesolo};

/// `KubeSolo` at idle with its own pods (`CoreDNS`, local-path-provisioner) and the
/// helm-controller: about 670 MiB resident on a 1 CPU / 1.5 GiB arm64 VM, rounded up.
pub const KUBESOLO_IDLE_MIB: u64 = 700;
/// The least memory a personal install is planned for: a host with 1.5 GiB, less what the kernel
/// and firmware keep.
pub const MIN_MEMORY_MIB: u64 = 1400;
/// The least memory for Zeabur Builder and zot: idle, and one small build. A 1.5 GiB host built
/// one with 221 MiB to spare, so a 2 GiB host leaves room for an app next to it.
pub const BUILDS_MIN_MEMORY_MIB: u64 = 1800;

/// The whole plan of a personal install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalPlan {
    /// Always `personal`, so a reader of the JSON tells it from a team plan.
    #[serde(rename = "use")]
    pub use_: &'static str,
    /// Facts it was computed from.
    pub facts: Facts,
    /// The `KubeSolo` release.
    pub kubesolo: KubeSoloPlan,
    /// The standalone helm-controller.
    pub helm_controller: HelmControllerPlan,
    /// The kubectl the installer uses; `KubeSolo` ships none.
    pub kubectl: Download,
    /// Files written before `KubeSolo` starts.
    #[serde(skip)]
    pub host_files: Vec<HostFile>,
    /// Their paths, for the JSON form.
    pub host_file_paths: Vec<String>,
    /// Namespaces created before anything installs into them.
    pub namespaces: Vec<Namespace>,
    /// Catalog components in install order.
    pub components: Vec<ComponentPlan>,
    /// Catalog components named but left out, with the reason.
    pub left_out: BTreeMap<String, String>,
    /// Installed components no longer planned; `apply` fills it from the cluster
    /// ([`crate::remove`]).
    pub removals: Vec<Removal>,
    /// zot's fixed `ClusterIP`, when zot is installed: containerd pulls its images through it.
    pub zot_cluster_ip: Option<String>,
    /// The storage class: `KubeSolo`'s local-path.
    pub storage: Storage,
    /// Idle memory: `KubeSolo` plus the measured components.
    pub memory: Memory,
    /// Things to read before applying.
    pub warnings: Vec<Warning>,
}

/// The `KubeSolo` release and its download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KubeSoloPlan {
    /// Release tag.
    pub version: String,
    /// The Kubernetes minor it ships.
    pub kubernetes: String,
    /// The offline archive.
    pub download: Download,
}

/// The helm-controller release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelmControllerPlan {
    /// Release tag.
    pub version: String,
    /// Its manifest.
    pub manifest: Download,
    /// The images the manifest names, as the installer pins them: `ref@digest`.
    pub images: Vec<String>,
    /// The install Jobs' image by tag, and the digest it must resolve to.
    pub job_image: String,
    /// The digest `apply` and `doctor` compare with what the Jobs ran.
    pub job_image_digest: String,
}

fn helm_controller() -> HelmControllerPlan {
    HelmControllerPlan {
        version: HELM_CONTROLLER.version.to_owned(),
        manifest: HELM_CONTROLLER.manifest().into(),
        images: HELM_CONTROLLER
            .images
            .iter()
            .map(|(reference, digest)| format!("{reference}@{digest}"))
            .collect(),
        job_image: HELM_CONTROLLER.job_image.reference.to_owned(),
        job_image_digest: HELM_CONTROLLER.job_image.digest.to_owned(),
    }
}

/// Takes Zeabur Builder, with its zot, out of the selection on a host with too little memory.
fn leave_out_builds(index: &Index, selection: &mut select::Selection, memory_mib: u64) {
    if memory_mib >= BUILDS_MIN_MEMORY_MIB {
        return;
    }
    let (kept, dropped): (Vec<_>, Vec<_>) = std::mem::take(&mut selection.components)
        .into_iter()
        .partition(|(id, _)| {
            !index
                .component(id)
                .is_some_and(|c| BUILD_CAPABILITIES.contains(&c.capability.as_str()))
        });
    selection.components = kept;
    for (id, _) in dropped {
        selection
            .left_out
            .insert(id, format!("needs {BUILDS_MIN_MEMORY_MIB} MiB of memory"));
    }
}

/// Computes the plan of a personal install.
///
/// # Errors
///
/// When the architecture has no release, the selection fails or a required input is missing.
pub fn plan(
    index: &Index,
    config: &PersonalConfig,
    facts: &Facts,
) -> Result<PersonalPlan, PlanError> {
    let arch = facts.arch.as_str();
    let archive = KUBESOLO
        .archive(arch)
        .ok_or_else(|| PlanError::Arch(arch.to_owned()))?;
    let kubectl = KUBECTL
        .binary(arch)
        .ok_or_else(|| PlanError::Arch(arch.to_owned()))?;
    let mut warnings = vec![warning(
        "personal.isolation",
        &[],
        "A personal install is for one person: whoever has the kubeconfig is cluster admin, \
         and nothing isolates apps from each other (KubeSolo enforces no NetworkPolicy). For a \
         team, install with K3s or RKE2 on a server with 4 GiB."
            .to_owned(),
    )];
    if !index
        .kubernetes
        .iter()
        .any(|v| v.starts_with(&format!("{}.", KUBESOLO.kubernetes)))
    {
        let tested = index.kubernetes.join(", ");
        warnings.push(warning(
            "kubernetes.untested",
            &[KUBESOLO.kubernetes, &tested],
            format!(
                "The catalog was not tested on Kubernetes {}; it lists {tested}.",
                KUBESOLO.kubernetes
            ),
        ));
    }

    let mut selection = select::select_personal(index, config)?;
    leave_out_builds(index, &mut selection, facts.memory_mib);
    let choices = Choices {
        inputs: &config.inputs,
        secret_files: &config.secret_files,
        facts,
    };
    let (mut components, namespaces) =
        components(index, &selection, &choices, PERSONAL_PROFILE, arch)?;
    for plan in &mut components {
        if let Some(mib) = index
            .component(&plan.id)
            .and_then(|c| c.idle_memory_mib(PERSONAL_DISTRIBUTION, arch))
        {
            plan.idle_memory_mib = Some(mib);
        }
    }

    let memory = idle_memory(KUBESOLO_IDLE_MIB, &components, facts.memory_mib);
    let idle = memory.cluster_mib + memory.components_mib;
    if facts.memory_mib < MIN_MEMORY_MIB {
        warnings.push(warning(
            "memory.personal.low",
            &[&facts.memory_mib.to_string()],
            format!(
                "A personal install needs at least 1.5 GiB of memory; this host has {} MiB.",
                facts.memory_mib
            ),
        ));
    } else if idle > facts.memory_mib * 3 / 4 {
        warnings.push(warning(
            "memory.personal.tight",
            &[&idle.to_string(), &facts.memory_mib.to_string()],
            format!(
                "At idle, KubeSolo and the components use about {idle} MiB of this host's {} MiB.",
                facts.memory_mib
            ),
        ));
    }

    let chosen = components.iter().map(|c| c.id.as_str()).collect();
    let zot_cluster_ip = zot_cluster_ip(index, &config.inputs, &chosen);
    let mut host_files = kubesolo::host_files(&kubesolo::Options {
        node_ip: facts.node_ips.first().cloned(),
        tls_san: config.host.tls_san.clone(),
    });
    host_files.extend(zot_cluster_ip.as_deref().map(kubesolo::zot_hosts_file));
    Ok(PersonalPlan {
        use_: "personal",
        facts: facts.clone(),
        kubesolo: KubeSoloPlan {
            version: KUBESOLO.version.to_owned(),
            kubernetes: KUBESOLO.kubernetes.to_owned(),
            download: archive.into(),
        },
        helm_controller: helm_controller(),
        kubectl: kubectl.into(),
        host_file_paths: host_files.iter().map(|f| f.path.clone()).collect(),
        host_files,
        namespaces: namespaces.into_values().collect(),
        components,
        left_out: selection.left_out,
        removals: Vec::new(),
        zot_cluster_ip,
        storage: Storage {
            default_class: kubesolo::LOCAL_PATH_CLASS.to_owned(),
            other_classes: Vec::new(),
        },
        memory,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::ReleasePlan;

    const INDEX: &str = include_str!("../fixtures/index.json");

    fn facts(memory_mib: u64) -> Facts {
        Facts {
            arch: "arm64".to_owned(),
            hostname: "solo".to_owned(),
            memory_mib,
            node_ips: vec!["10.0.0.5".to_owned()],
            node_prefix: None,
        }
    }

    fn config(extra: &str) -> PersonalConfig {
        PersonalConfig::parse(&format!("use: personal\n{extra}")).unwrap()
    }

    fn ids(plan: &PersonalPlan) -> Vec<&str> {
        plan.components.iter().map(|c| c.id.as_str()).collect()
    }

    #[test]
    fn a_personal_install_has_ingress_and_certificates_and_no_panel() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let plan = plan(&index, &config(""), &facts(1500)).unwrap();
        let ids = ids(&plan);
        assert!(ids.contains(&"ingress/traefik"), "{ids:?}");
        assert!(ids.contains(&"certificates/cert-manager"), "{ids:?}");
        assert!(!ids.contains(&"metrics/victoria-metrics"), "{ids:?}");
        let json = serde_json::to_value(&plan).unwrap();
        assert_eq!(json["use"], "personal");
        assert!(json.get("identity").is_none() && json.get("zeabur").is_none());
        assert!(plan.warnings.iter().any(|w| w.code == "personal.isolation"));
        assert_eq!(plan.storage.default_class, "local-path");
        for component in &plan.components {
            for release in &component.releases {
                if let ReleasePlan::HelmChart { object, .. } = release {
                    assert_eq!(object["apiVersion"], "helm.cattle.io/v1");
                    assert!(object["metadata"].get("finalizers").is_none());
                }
            }
        }
    }

    #[test]
    fn builder_with_its_zot_is_an_option_that_needs_the_memory() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let off = plan(&index, &config(""), &facts(4096)).unwrap();
        assert!(!ids(&off).contains(&crate::plan::BUILDER) && off.zot_cluster_ip.is_none());

        let on = plan(&index, &config("enable: [builds]\n"), &facts(4096)).unwrap();
        let chosen = ids(&on);
        assert!(chosen.contains(&crate::plan::BUILDER), "{chosen:?}");
        assert_eq!(on.zot_cluster_ip.as_deref(), Some("10.43.0.50"));
        let hosts = on
            .host_files
            .iter()
            .find(|f| f.path == kubesolo::ZOT_HOSTS_FILE)
            .unwrap();
        assert!(hosts.content.contains("https://10.43.0.50:5000"));

        let small = plan(
            &index,
            &config("enable: [builds]\n"),
            &facts(BUILDS_MIN_MEMORY_MIB - 1),
        )
        .unwrap();
        assert!(!ids(&small).contains(&crate::plan::BUILDER));
        assert!(small.left_out.contains_key(crate::plan::BUILDER));
        assert!(small.zot_cluster_ip.is_none());
    }

    #[test]
    fn only_the_capabilities_tested_on_kubesolo_can_be_added() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let plan = plan(&index, &config("enable: [logs]\n"), &facts(4096)).unwrap();
        assert!(!ids(&plan).contains(&"logs/victoria-logs"));
        assert!(plan.left_out.contains_key("logs/victoria-logs"));
        let plan = super::plan(&index, &config("disable: [certificates]\n"), &facts(4096)).unwrap();
        assert!(!ids(&plan).contains(&"certificates/cert-manager"));
    }

    #[test]
    fn components_that_leave_out_kubesolo_are_left_out() {
        let mut index = Index::parse(INDEX.as_bytes()).unwrap();
        for component in &mut index.components {
            if component.id == "ingress/traefik" {
                component.distributions = vec!["rke2".to_owned()];
            }
        }
        let plan = plan(&index, &config("disable: [certificates]\n"), &facts(4096)).unwrap();
        assert!(!ids(&plan).contains(&"ingress/traefik"));
        assert!(plan.left_out["ingress/traefik"].contains("rke2"));
    }

    #[test]
    fn the_downloads_and_images_are_pinned() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let plan = plan(&index, &config(""), &facts(4096)).unwrap();
        for download in [
            &plan.kubesolo.download,
            &plan.helm_controller.manifest,
            &plan.kubectl,
        ] {
            assert!(download.url.starts_with("https://"), "{}", download.url);
            assert_eq!(download.sha256.as_ref().map(String::len), Some(64));
        }
        assert!(
            plan.kubesolo
                .download
                .file
                .ends_with("-linux-arm64-offline.tar.gz")
        );
        assert!(
            plan.helm_controller
                .images
                .iter()
                .all(|i| i.contains("@sha256:"))
        );
        assert!(plan.helm_controller.job_image_digest.starts_with("sha256:"));
        let mut riscv = facts(4096);
        riscv.arch = "riscv64".to_owned();
        assert!(matches!(
            super::plan(&index, &config(""), &riscv),
            Err(PlanError::Arch(_))
        ));
    }

    #[test]
    fn a_host_under_one_and_a_half_gib_is_warned() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        for (mib, low) in [(MIN_MEMORY_MIB, false), (MIN_MEMORY_MIB - 1, true)] {
            let plan = plan(&index, &config(""), &facts(mib)).unwrap();
            assert_eq!(
                plan.warnings
                    .iter()
                    .any(|w| w.code == "memory.personal.low"),
                low,
                "{mib} MiB"
            );
        }
    }

    #[test]
    fn the_config_file_names_the_node_address() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let plan = plan(
            &index,
            &config("host: {tlsSan: [solo.example.com]}\n"),
            &facts(4096),
        )
        .unwrap();
        let file = plan
            .host_files
            .iter()
            .find(|f| f.path == kubesolo::CONFIG_FILE)
            .unwrap();
        assert!(file.content.contains("10.0.0.5"));
        assert!(file.content.contains("solo.example.com"));
    }
}
