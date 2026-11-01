//! The plan of a personal install: `KubeSolo`, the standalone helm-controller and the components.
//!
//! There is no Zeabur panel or Zeabur Identity. Zeabur Builder, with its registry, is an option.
//! The person runs `zeaburctl ui` on their own computer with the admin kubeconfig. Nothing is
//! isolated: `KubeSolo`'s bridge CNI enforces no `NetworkPolicy`, there is one admin and one node.
//! The plan makes no network policies, and every object it makes is one `kubectl apply` can make
//! and `kubectl delete` can remove without the installer.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::catalog::{Component, Index};
use crate::cluster::HostFile;
use crate::config::PersonalConfig;
use crate::pins::{HELM_CONTROLLER, KUBECTL, KUBESOLO};
use crate::plan::{
    Choices, ComponentPlan, Download, Facts, Memory, Namespace, RegistryPlan, Warning, components,
    idle_memory, registry, warning,
};
use crate::record::Changes;
use crate::remove::Removal;
use crate::select::{self, PERSONAL_DISTRIBUTION, PERSONAL_PROFILE};
use crate::storage::Storage;
use crate::{PlanError, kubesolo};

/// `KubeSolo` at idle with its own pods (`CoreDNS`, local-path-provisioner) and the
/// helm-controller: about 670 MiB resident on a 1 CPU / 1.5 GiB arm64 VM, rounded up.
pub const KUBESOLO_IDLE_MIB: u64 = 700;
/// The least memory a personal install is planned for: a host with 1.5 GiB, less what the kernel
/// and firmware keep.
pub const MIN_MEMORY_MIB: u64 = 1400;
/// Memory left for an app while a component works on top of its idle use.
pub const APP_ROOM_MIB: u64 = 600;

/// The whole plan of a personal install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalPlan {
    /// Always `personal`, so a reader of the JSON tells it from a team plan.
    #[serde(rename = "use")]
    pub use_: &'static str,
    /// Facts it was computed from.
    pub facts: Facts,
    /// The node's name in Kubernetes, written into `KubeSolo`'s configuration.
    pub node_name: String,
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
    /// The in-cluster registry, when a chosen component serves one: containerd pulls its images
    /// through its fixed `ClusterIP`.
    pub registry: Option<RegistryPlan>,
    /// The storage class: `KubeSolo`'s local-path.
    pub storage: Storage,
    /// Idle memory: `KubeSolo` plus the measured components.
    pub memory: Memory,
    /// What `apply` changes against the install record; `None` on a first install.
    pub changes: Option<Changes>,
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

/// A component's idle memory on `KubeSolo`, else on a single node.
fn idle_memory_mib(component: &Component, arch: &str) -> Option<u64> {
    component
        .idle_memory_mib(PERSONAL_DISTRIBUTION, arch)
        .or_else(|| component.idle_memory_mib(PERSONAL_PROFILE, arch))
}

/// The least host memory a personal install needs for a component that uses memory on top of its
/// idle use while working: `KubeSolo`, the component idle and at work, and room for an app.
#[must_use]
pub fn min_memory_mib(component: &Component, arch: &str) -> Option<u64> {
    let work = component.resources.work_memory_mib?;
    Some(KUBESOLO_IDLE_MIB + idle_memory_mib(component, arch).unwrap_or(0) + work + APP_ROOM_MIB)
}

/// Takes out of the selection the components that need more memory than the host has.
fn leave_out_short_of_memory(index: &Index, selection: &mut select::Selection, facts: &Facts) {
    let needs = |id: &str| {
        index
            .component(id)
            .and_then(|c| min_memory_mib(c, &facts.arch))
            .filter(|&mib| facts.memory_mib < mib)
    };
    let (kept, dropped): (Vec<_>, Vec<_>) = std::mem::take(&mut selection.components)
        .into_iter()
        .partition(|(id, _)| needs(id).is_none());
    selection.components = kept;
    for (id, _) in dropped {
        if let Some(mib) = needs(&id) {
            selection
                .left_out
                .insert(id, format!("needs {mib} MiB of memory"));
        }
    }
}

/// What a personal install warns about whatever is chosen.
fn warnings(index: &Index) -> Vec<Warning> {
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
    warnings
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
    facts.check_installer()?;
    let node_name = crate::node_name::choose(config.host.node_name.as_deref(), facts, &[])?;
    let arch = facts.arch.as_str();
    let no_release = || PlanError::Arch(arch.to_owned());
    let archive = KUBESOLO.archive(arch).ok_or_else(no_release)?;
    let kubectl = KUBECTL.binary(arch).ok_or_else(no_release)?;
    let mut warnings = warnings(index);
    let mut selection = select::select_personal(index, config)?;
    warnings.extend(selection.warnings.clone());
    leave_out_short_of_memory(index, &mut selection, facts);
    let choices = Choices {
        inputs: &config.inputs,
        secret_files: &config.secret_files,
    };
    let (mut components, namespaces) =
        components(index, &selection, &choices, PERSONAL_PROFILE, arch)?;
    for plan in &mut components {
        plan.network_policies.clear();
        plan.idle_memory_mib = index
            .component(&plan.id)
            .and_then(|c| idle_memory_mib(c, arch));
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
    let registry = registry(index, &config.inputs, &chosen);
    let mut host_files = kubesolo::host_files(&kubesolo::Options {
        node_name: node_name.clone(),
        node_ip: facts.node_ips.first().cloned(),
        tls_san: config.host.tls_san.clone(),
    });
    host_files.extend(registry.as_ref().and_then(|r| {
        Some(kubesolo::registry_hosts_file(
            &r.host,
            r.cluster_ip.as_deref()?,
        ))
    }));
    let changes = facts
        .record
        .as_ref()
        .map(|r| Changes::new(r, index, &components, KUBESOLO.version));
    warnings.extend(
        facts
            .record
            .as_ref()
            .and_then(|r| crate::record::builder_deletes_images(r, &components)),
    );
    Ok(PersonalPlan {
        use_: "personal",
        facts: facts.clone(),
        node_name,
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
        registry,
        storage: Storage {
            default_class: kubesolo::LOCAL_PATH_CLASS.to_owned(),
            other_classes: Vec::new(),
        },
        memory,
        changes,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Default;
    use crate::plan::ReleasePlan;
    const INDEX: &str = include_str!(concat!(env!("ZEABUR_CATALOG_RELEASE"), "/index.json"));

    fn facts(memory_mib: u64) -> Facts {
        Facts {
            arch: "arm64".to_owned(),
            hostname: "solo".to_owned(),
            node_name: None,
            host_hex: "a1b2c3".to_owned(),
            memory_mib,
            node_ips: vec!["10.0.0.5".to_owned()],
            record: None,
            installer: None,
        }
    }

    fn config(extra: &str) -> PersonalConfig {
        PersonalConfig::parse(&format!("use: personal\n{extra}")).unwrap()
    }

    fn ids(plan: &PersonalPlan) -> Vec<&str> {
        plan.components.iter().map(|c| c.id.as_str()).collect()
    }

    #[test]
    fn a_stopped_kubesolo_last_applied_by_a_newer_installer_is_left_alone() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let stopped = Facts {
            installer: Some("99.0.0".to_owned()),
            ..facts(8192)
        };
        assert!(matches!(
            plan(&index, &config(""), &stopped),
            Err(PlanError::NewerInstaller { recorded, .. }) if recorded == "99.0.0"
        ));
    }

    #[test]
    fn kubesolo_is_told_the_node_name() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let name = |extra: &str, facts: &Facts| {
            let plan = plan(&index, &config(extra), facts).unwrap();
            let file = plan
                .host_files
                .iter()
                .find(|f| f.path == kubesolo::CONFIG_FILE)
                .unwrap();
            let value: serde_json::Value = serde_saphyr::from_str(&file.content).unwrap();
            value["kubernetes"]["nodeName"].as_str().unwrap().to_owned()
        };
        assert_eq!(name("", &facts(4096)), "solo");
        assert_eq!(name("host: {nodeName: laptop}\n", &facts(4096)), "laptop");
        let mut kept = facts(4096);
        kept.hostname = "10-0-0-5".to_owned();
        kept.node_name = Some("solo".to_owned());
        assert_eq!(name("", &kept), "solo");
    }

    #[test]
    fn a_personal_install_has_ingress_and_certificates_and_no_panel() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let plan = plan(&index, &config(""), &facts(1500)).unwrap();
        let ids = ids(&plan);
        assert!(ids.contains(&"ingress/traefik"), "{ids:?}");
        assert!(ids.contains(&"certificates/cert-manager"), "{ids:?}");
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
    fn a_registry_is_an_option_that_needs_the_memory() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let off = plan(&index, &config(""), &facts(4096)).unwrap();
        assert!(off.registry.is_none());

        let on = plan(&index, &config("enable: [builds]\n"), &facts(4096)).unwrap();
        let registry = on.registry.as_ref().unwrap();
        assert!(ids(&on).contains(&registry.component.as_str()));
        let ip = registry.cluster_ip.as_deref().unwrap();
        let hosts = kubesolo::registry_hosts_file(&registry.host, ip);
        assert!(on.host_files.contains(&hosts));
        assert!(hosts.content.contains(&format!("https://{ip}:")));
        assert!(on.components.iter().all(|c| c.network_policies.is_empty()));

        let builder = index.component(&registry.component).unwrap();
        let needs = min_memory_mib(builder, "arm64").unwrap();
        let small = plan(&index, &config("enable: [builds]\n"), &facts(needs - 1)).unwrap();
        assert!(!ids(&small).contains(&registry.component.as_str()));
        assert!(small.left_out.contains_key(&registry.component));
        assert!(small.registry.is_none());
    }

    #[test]
    fn a_component_not_tested_on_kubesolo_is_installed_when_asked_with_a_warning() {
        let mut index = Index::parse(INDEX.as_bytes()).unwrap();
        for component in &mut index.components {
            if component.id == "logs/victoria-logs" {
                component.tested_on.retain(|d| d != PERSONAL_DISTRIBUTION);
            }
        }
        let asked = plan(&index, &config("enable: [logs]\n"), &facts(4096)).unwrap();
        assert!(ids(&asked).contains(&"logs/victoria-logs"));
        assert!(asked.warnings.iter().any(|w| w.code == "config.untested"
            && w.values == ["logs/victoria-logs", PERSONAL_DISTRIBUTION]));
        let default = plan(&index, &config(""), &facts(4096)).unwrap();
        assert!(!ids(&default).contains(&"logs/victoria-logs"));
    }

    #[test]
    fn the_defaults_are_what_the_catalog_recommends_and_tested_on_kubesolo() {
        let mut index = Index::parse(INDEX.as_bytes()).unwrap();
        for component in &mut index.components {
            if component.id == "ingress/traefik" {
                component.tested_on.retain(|d| d != PERSONAL_DISTRIBUTION);
            }
            if component.id == "config-reload/reloader" {
                component.tested_on.push(PERSONAL_DISTRIBUTION.to_owned());
                component
                    .defaults
                    .insert(PERSONAL_PROFILE.to_owned(), Default::Recommended);
            }
        }
        let plan = plan(&index, &config("disable: [certificates]\n"), &facts(4096)).unwrap();
        assert!(!ids(&plan).contains(&"ingress/traefik"));
        assert!(ids(&plan).contains(&"config-reload/reloader"));
    }

    #[test]
    fn databases_can_be_added_to_a_personal_install() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let plan = plan(&index, &config("enable: [databases]\n"), &facts(4096)).unwrap();
        assert!(ids(&plan).contains(&"databases/cloudnative-pg"));
    }

    #[test]
    fn components_that_leave_out_kubesolo_are_left_out() {
        let mut index = Index::parse(INDEX.as_bytes()).unwrap();
        for component in &mut index.components {
            if component.id == "metrics/metrics-server" {
                component.distributions = vec!["rke2".to_owned()];
            }
        }
        let plan = plan(&index, &config(""), &facts(4096)).unwrap();
        assert!(!ids(&plan).contains(&"metrics/metrics-server"));
        assert!(plan.left_out["metrics/metrics-server"].contains("rke2"));
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
