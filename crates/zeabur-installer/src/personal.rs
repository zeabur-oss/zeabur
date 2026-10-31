//! A personal install: `KubeSolo`, the standalone helm-controller and the components. Each step
//! checks what is already there, so `apply` runs again safely.

use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use serde_json::{Value, json};
use zeabur_installer_host::kubectl::{self, Kubectl};
use zeabur_installer_host::preflight::{self, Node, Status};
use zeabur_installer_host::{archive, cmd, download, facts, files, systemd};
use zeabur_installer_plan::catalog::Index;
use zeabur_installer_plan::config::PersonalConfig;
use zeabur_installer_plan::kubesolo::{
    ADMIN_KUBECONFIG, BINARY, DATA_DIR, KUBECTL, PERSONAL_KUBECONFIG, STATE_DIR, UNIT,
    WEBHOOK_SCRIPT, helm_controller_manifest, job_image_mismatches, personal_kubeconfig,
    registry_ca_file,
};
use zeabur_installer_plan::personal::{self, PersonalPlan};
use zeabur_installer_plan::pins::HELM_CONTROLLER;
use zeabur_installer_plan::plan::{
    self, ComponentPlan, MANAGER, NAMESPACE_MANAGER, RegistryPlan, Warning,
};
use zeabur_installer_plan::remove::Removal;
use zeabur_installer_plan::select::PERSONAL_PROFILE;
use zeabur_telemetry::Step;

use crate::apply::STATE_CONFIGMAP;
use crate::telemetry::{self, Reporter};
use crate::{InstallError, doctor, load, out, remove, render};

/// The plan, with the removals read from the cluster when it runs.
pub(crate) fn make_plan(config: &PersonalConfig) -> Result<PersonalPlan, InstallError> {
    let index = load::catalog(config.sources.catalog.as_deref(), STATE_DIR)?;
    let running = systemd::is_active(UNIT);
    let mut facts = facts::read()?;
    facts.installer = crate::apply::kept_installer(Path::new(STATE_DIR));
    if running {
        facts.record = crate::apply::read_record(&kubectl())?;
    }
    let mut plan = personal::plan(&index, config, &facts)?;
    if let Some(changes) = &mut plan.changes {
        changes.set_host_files(crate::apply::changed_files(plan.host_files.iter()));
        changes.restart |= binary_outdated(Path::new(BINARY), &plan.kubesolo.version);
    }
    plan.removals = removals(
        running,
        &index,
        &plan.components,
        &config.allow_data_loss,
        &mut plan.warnings,
    )?;
    Ok(plan)
}

fn removals(
    running: bool,
    index: &Index,
    planned: &[ComponentPlan],
    allow_data_loss: &[String],
    warnings: &mut Vec<Warning>,
) -> Result<Vec<Removal>, InstallError> {
    let kubectl = kubectl();
    remove::plan(
        running.then_some(&kubectl),
        index,
        planned,
        PERSONAL_PROFILE,
        allow_data_loss,
        warnings,
    )
}

pub(crate) fn show_plan(config: &PersonalConfig, json: bool) -> Result<(), InstallError> {
    let plan = make_plan(config)?;
    if json {
        let text = serde_json::to_string_pretty(&plan)
            .map_err(|e| InstallError::Message(e.to_string()))?;
        out::line(&text);
    } else {
        out::line(render::personal_plan(&plan).trim_end());
        if show_checks(config) {
            out::line("apply stops until what failed is fixed.");
        }
    }
    Ok(())
}

/// Runs and prints the preflight checks. Returns whether one failed.
pub(crate) fn show_checks(config: &PersonalConfig) -> bool {
    out::step("Checking the host");
    let checks = preflight::run(preflight::Options {
        cluster_running: systemd::is_active(UNIT),
        require_encrypted_disk: config.host.require_encrypted_disk,
        node: Node::KubeSolo,
    });
    for check in &checks {
        let mark = match check.status {
            Status::Pass => "ok  ",
            Status::Warn => "warn",
            Status::Fail => "FAIL",
        };
        out::err(&format!(
            "    {mark} {:<18} {}",
            check.name.label(),
            check.detail
        ));
    }
    checks.iter().any(|c| c.status == Status::Fail)
}

fn kubectl() -> Kubectl {
    Kubectl::new(KUBECTL, ADMIN_KUBECONFIG)
}

pub(crate) fn apply(
    config: &PersonalConfig,
    config_path: &Path,
    no_telemetry: bool,
) -> Result<(), InstallError> {
    let report = Reporter::start(
        config.telemetry && !no_telemetry,
        config_path,
        telemetry::personal(),
    );
    report.end(install(config, &report))
}

fn install(config: &PersonalConfig, report: &Reporter) -> Result<(), InstallError> {
    let mut plan = make_plan(config)?;
    let running = systemd::is_active(UNIT);
    out::line(render::personal_plan(&plan).trim_end());
    remove::check(&plan.removals)?;
    report.at(Step::Preflight);
    if show_checks(config) {
        return Err(InstallError::Message(
            "the host is not ready; fix what failed and run apply again".to_owned(),
        ));
    }

    report.at(Step::Cluster);
    install_kubesolo(&plan)?;
    let kubectl = kubectl();
    kubectl.wait_ready(Duration::from_mins(10))?;
    kubectl::poll(Duration::from_mins(10), "the node to be Ready", || {
        kubectl
            .wait(&["node", "--all", "--for=condition=Ready", "--timeout=10s"])
            .is_ok()
    })?;
    for (namespace, deployment) in [
        ("kube-system", "coredns"),
        ("local-path-storage", "local-path-provisioner"),
    ] {
        wait_available(&kubectl, namespace, deployment)?;
    }

    helm_controller(&kubectl, &plan)?;
    if !running && crate::apply::read_record(&kubectl)?.is_some() {
        plan = make_plan(config)?;
        out::line(render::changes(plan.changes.as_ref(), UNIT).trim_end());
        out::line(render::removals(&plan.removals).trim_end());
        remove::check(&plan.removals)?;
    } else if !running {
        let index = load::catalog(config.sources.catalog.as_deref(), STATE_DIR)?;
        plan.removals = removals(
            true,
            &index,
            &plan.components,
            &config.allow_data_loss,
            &mut Vec::new(),
        )?;
        out::line(render::removals(&plan.removals).trim_end());
        remove::check(&plan.removals)?;
    }
    remove::run(&kubectl, &plan.removals)?;

    report.at(Step::ClusterSetup);
    out::step("Namespaces");
    for namespace in &plan.namespaces {
        kubectl.apply_as(
            NAMESPACE_MANAGER,
            plan::namespace_object(namespace).to_string().as_bytes(),
        )?;
    }
    report.at(Step::Components);
    for component in &plan.components {
        out::step(&format!("{} ({})", component.display_name, component.id));
        crate::apply::secrets(&kubectl, component)?;
        crate::apply::releases(&kubectl, component)?;
        for wait in &component.smoke {
            let mut args: Vec<&str> = wait.iter().map(String::as_str).collect();
            args.push("--timeout=5m");
            kubectl.wait(&args)?;
        }
        if let Some(registry) = plan
            .registry
            .as_ref()
            .filter(|r| r.component == component.id && r.cluster_ip.is_some())
        {
            let path = registry_ca_file(&registry.host);
            crate::apply::registry_ca(&kubectl, registry, Path::new(&path))?;
        }
    }
    if let Some(registry) = &plan.registry {
        registry_info(&kubectl, registry)?;
    }
    let wrong = job_images(&kubectl, &plan)?;
    if !wrong.is_empty() {
        return Err(InstallError::Message(format!(
            "the install Jobs ran an image other than {}@{}:\n  {}",
            plan.helm_controller.job_image,
            plan.helm_controller.job_image_digest,
            wrong.join("\n  ")
        )));
    }
    report.at(Step::Record);
    record(&kubectl, &plan)?;
    let name = plan.facts.hostname.to_ascii_lowercase();
    write_kubeconfig(&name)?;

    out::step("Done");
    out::line(&done_text(&name, &facts::addresses()));
    Ok(())
}

/// What `zeaburctl up` reads to push to the registry: its address and the ConfigMap with its CA.
fn registry_info(kubectl: &Kubectl, registry: &RegistryPlan) -> Result<(), InstallError> {
    let labels = json!({"app.kubernetes.io/managed-by": MANAGER});
    kubectl.apply_object(&json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {"name": plan::REGISTRY_CA_CONFIGMAP, "namespace": "kube-public", "labels": labels},
        "data": {"ca.crt": crate::apply::registry_ca_pem(kubectl, registry)?},
    }))?;
    kubectl.apply_object(&json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {"name": "zeabur-info", "namespace": "kube-public", "labels": labels},
        "data": {
            "registry": registry.host,
            "registryCa": format!("kube-public/{}/ca.crt", plan::REGISTRY_CA_CONFIGMAP),
        },
    }))?;
    Ok(())
}

/// The install Job Pods that ran another image than the pinned digest.
fn job_images(kubectl: &Kubectl, plan: &PersonalPlan) -> Result<Vec<String>, InstallError> {
    let text = kubectl.run(
        &[
            "get",
            "pods",
            "--all-namespaces",
            "--selector=helmcharts.helm.cattle.io/chart",
            "--output=json",
        ],
        None,
    )?;
    let pods: Value =
        serde_json::from_str(&text).map_err(|e| InstallError::Message(e.to_string()))?;
    Ok(job_image_mismatches(
        &pods,
        &plan.helm_controller.job_image_digest,
    ))
}

/// What to run on the person's own computer.
fn done_text(name: &str, addresses: &[String]) -> String {
    let host = addresses.first().map_or("<this server>", String::as_str);
    format!(
        "This server runs KubeSolo and the components, for one person. On your own computer:\n\
         \n  ssh <you>@{host} sudo cat {PERSONAL_KUBECONFIG} > ~/.kube/{name}.yaml\n  \
         KUBECONFIG=~/.kube/{name}.yaml zeaburctl ui --context {name}\n\n\
         Whoever has this file is cluster admin; keep it like an SSH key. If your computer \
         reaches the server at another address, change `server:` in the file and add the \
         address to host.tlsSan."
    )
}

fn wait_available(kubectl: &Kubectl, namespace: &str, name: &str) -> Result<(), InstallError> {
    let target = format!("deployment/{name}");
    kubectl::poll(
        Duration::from_mins(10),
        &format!("{namespace}/{name}"),
        || {
            kubectl
                .wait(&[
                    "--namespace",
                    namespace,
                    &target,
                    "--for=condition=Available",
                    "--timeout=10s",
                ])
                .is_ok()
        },
    )?;
    Ok(())
}

/// Downloads `KubeSolo` and kubectl, writes the files and starts or restarts `KubeSolo`.
fn install_kubesolo(plan: &PersonalPlan) -> Result<(), InstallError> {
    out::step(&format!("KubeSolo {} ({UNIT})", plan.kubesolo.version));
    let agent = download::agent();
    let kubectl = Path::new(KUBECTL);
    out::err(&format!("    {}", plan.kubectl.url));
    download::fetch(
        &agent,
        &plan.kubectl.url,
        kubectl,
        plan.kubectl.sha256.as_deref(),
    )?;
    fs::set_permissions(kubectl, Permissions::from_mode(0o755)).map_err(io(kubectl))?;

    let mut changed = false;
    for file in &plan.host_files {
        changed |= files::write(Path::new(&file.path), file.mode, file.content.as_bytes())?;
    }
    if binary_outdated(Path::new(BINARY), &plan.kubesolo.version) {
        let download = &plan.kubesolo.download;
        let dir = PathBuf::from(STATE_DIR)
            .join("downloads")
            .join(&plan.kubesolo.version);
        out::err(&format!("    {}", download.url));
        download::fetch(
            &agent,
            &download.url,
            &dir.join(&download.file),
            download.sha256.as_deref(),
        )?;
        let unpacked = dir.join("unpacked");
        archive::unpack_tar_gz(&dir.join(&download.file), &unpacked)?;
        let source = unpacked.join("kubesolo");
        let temp = Path::new(BINARY).with_extension("zeabur-installer");
        fs::copy(&source, &temp).map_err(io(&temp))?;
        fs::set_permissions(&temp, Permissions::from_mode(0o755)).map_err(io(&temp))?;
        fs::rename(&temp, BINARY).map_err(io(Path::new(BINARY)))?;
        fs::remove_dir_all(&unpacked).map_err(io(&unpacked))?;
        changed = true;
    }
    if changed {
        systemd::daemon_reload()?;
    }
    if !systemd::is_active(UNIT) {
        out::err(&format!(
            "    starting {UNIT}; the first start takes a few minutes"
        ));
        systemd::enable_now(UNIT)?;
    } else if changed {
        out::err(&format!("    restarting {UNIT} for the new configuration"));
        systemd::restart(UNIT)?;
    }
    Ok(())
}

/// Whether the binary at `path` is missing or does not report `version`.
fn binary_outdated(path: &Path, version: &str) -> bool {
    !cmd::run(path, &["--version"], None)
        .unwrap_or_default()
        .contains(version)
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> InstallError {
    let path = path.display().to_string();
    move |e| InstallError::Message(format!("{path}: {e}"))
}

/// The standalone helm-controller, its images pinned by digest.
fn helm_controller(kubectl: &Kubectl, plan: &PersonalPlan) -> Result<(), InstallError> {
    out::step(&format!("helm-controller {}", plan.helm_controller.version));
    let manifest = &plan.helm_controller.manifest;
    let text = download::fetch_text(
        &download::agent(),
        &manifest.url,
        manifest.sha256.as_deref(),
    )?;
    kubectl.apply(helm_controller_manifest(&text, &HELM_CONTROLLER)?.as_bytes())?;
    wait_available(kubectl, "helm-controller", "helm-controller")
}

fn record(kubectl: &Kubectl, plan: &PersonalPlan) -> Result<(), InstallError> {
    let components: Vec<&str> = plan.components.iter().map(|c| c.id.as_str()).collect();
    let summary = serde_json::to_string(plan).map_err(|e| InstallError::Message(e.to_string()))?;
    kubectl.apply_object(&json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {
            "name": STATE_CONFIGMAP,
            "namespace": "kube-system",
            "labels": {"app.kubernetes.io/managed-by": MANAGER},
        },
        "data": {
            "installer": env!("CARGO_PKG_VERSION"),
            "use": "personal",
            "kubesolo": plan.kubesolo.version,
            "components": components.join(","),
            "plan": summary,
        },
    }))?;
    crate::apply::keep_installer(Path::new(STATE_DIR))
}

/// Writes the kubeconfig for the person's computer, readable by root only.
fn write_kubeconfig(name: &str) -> Result<(), InstallError> {
    let path = Path::new(ADMIN_KUBECONFIG);
    let text = fs::read_to_string(path).map_err(io(path))?;
    let admin: Value = serde_saphyr::from_str(&text)
        .map_err(|e| InstallError::Message(format!("{ADMIN_KUBECONFIG}: {e}")))?;
    let config = personal_kubeconfig(&admin, name, None)?;
    let yaml =
        serde_saphyr::to_string(&config).map_err(|e| InstallError::Message(e.to_string()))?;
    files::write(Path::new(PERSONAL_KUBECONFIG), 0o600, yaml.as_bytes())?;
    Ok(())
}

pub(crate) fn doctor(config: &PersonalConfig) -> ExitCode {
    if let Some(code) = doctor::cannot_run() {
        return code;
    }
    if !systemd::is_active(UNIT) {
        out::line(&format!(
            "CRITICAL {UNIT} is not running: journalctl -u {UNIT}"
        ));
        return ExitCode::from(2);
    }
    let kubectl = kubectl();
    if kubectl.wait_ready(Duration::from_secs(30)).is_err() {
        out::line("CRITICAL the Kubernetes API does not answer /readyz");
        return ExitCode::from(2);
    }
    let mut results = vec![(
        Status::Pass,
        format!("{UNIT} is running and the API is ready"),
    )];
    if let Ok(plan) = make_plan(config) {
        for file in &plan.host_files {
            if fs::read_to_string(&file.path).ok().as_deref() != Some(file.content.as_str()) {
                results.push((
                    Status::Warn,
                    format!(
                        "{} differs from the plan: zeabur-installer apply",
                        file.path
                    ),
                ));
            }
        }
        match job_images(&kubectl, &plan) {
            Ok(wrong) if wrong.is_empty() => results.push((
                Status::Pass,
                "the install Jobs ran the pinned klipper-helm image".to_owned(),
            )),
            Ok(wrong) => results.push((
                Status::Fail,
                format!(
                    "the install Jobs ran an image other than the pinned one: {}",
                    wrong.join(", ")
                ),
            )),
            Err(e) => results.push((Status::Warn, format!("reading the install Jobs: {e}"))),
        }
    }
    results.push(webhook_check(&kubectl));
    results.push(doctor::deployment(
        &kubectl,
        "helm-controller",
        "helm-controller",
    ));
    match kubectl.get("configmap", STATE_CONFIGMAP, Some("kube-system")) {
        Ok(Some(state)) => {
            let plan = state
                .pointer("/data/plan")
                .and_then(Value::as_str)
                .and_then(|p| serde_json::from_str::<Value>(p).ok())
                .unwrap_or_default();
            results.extend(doctor::components(&kubectl, &plan));
            results.push(doctor::default_class(&kubectl, &plan));
            results.extend(registry_checks(&kubectl, &plan));
        }
        Ok(None) => results.push((
            Status::Fail,
            "no record of an install: run zeabur-installer apply".to_owned(),
        )),
        Err(e) => results.push((Status::Fail, format!("reading the install record: {e}"))),
    }
    results.push((
        Status::Warn,
        "a personal install isolates nothing: one admin, one node, and no NetworkPolicy is \
         enforced"
            .to_owned(),
    ));
    doctor::report(&results)
}

/// The registry, when the plan has one with a fixed address: containerd trusts the CA it serves
/// with.
fn registry_checks(kubectl: &Kubectl, plan: &Value) -> Vec<(Status, String)> {
    let Some(registry) = plan
        .get("registry")
        .and_then(|r| serde_json::from_value::<RegistryPlan>(r.clone()).ok())
        .filter(|r| r.cluster_ip.is_some())
    else {
        return Vec::new();
    };
    let ca = &registry.ca;
    let file = registry_ca_file(&registry.host);
    let served = crate::apply::registry_ca_pem(kubectl, &registry).ok();
    let on_host = fs::read_to_string(&file).ok();
    vec![match (served, on_host) {
        (Some(served), Some(on_host)) if served == on_host => (
            Status::Pass,
            format!("containerd trusts the CA {} serves with", registry.host),
        ),
        (None, _) => (
            Status::Fail,
            format!(
                "secret {}/{} has no {}: kubectl describe -n {} certificates",
                ca.namespace, ca.name, ca.key, ca.namespace
            ),
        ),
        _ => (
            Status::Fail,
            format!(
                "{file} is not the CA {} serves with, so nodes cannot pull images built here: \
                 zeabur-installer apply",
                registry.host
            ),
        ),
    }]
}

/// Whether `KubeSolo`'s webhook still changes Jobs, which stops helm-controller (`kubesolo.rs`).
fn webhook_check(kubectl: &Kubectl) -> (Status, String) {
    let config = kubectl.get("mutatingwebhookconfiguration", "webhook.kubesolo.io", None);
    let jobs = config.ok().flatten().is_some_and(|c| {
        c.pointer("/webhooks/0/rules/0/resources")
            .and_then(Value::as_array)
            .is_some_and(|r| r.iter().any(|v| v == "jobs"))
    });
    if jobs {
        (
            Status::Warn,
            format!(
                "KubeSolo's webhook still changes Jobs, so charts do not install: \
                 sudo {WEBHOOK_SCRIPT}, or journalctl -u {UNIT}"
            ),
        )
    } else {
        (
            Status::Pass,
            "KubeSolo's webhook leaves Jobs alone".to_owned(),
        )
    }
}

/// How to remove a personal install. Nothing in it depends on the installer, so it only prints
/// the commands.
pub(crate) fn uninstall_text() -> String {
    format!(
        "A personal install is KubeSolo and what runs on it. The components keep running without \
         the installer; manage them with kubectl (kubectl get helmcharts -n kube-system).\n\n\
         To remove everything, apps and volumes included:\n\n  \
         sudo systemctl disable --now {UNIT}\n  \
         sudo rm -f /etc/systemd/system/{UNIT}.service {BINARY} {WEBHOOK_SCRIPT}\n  \
         sudo systemctl daemon-reload\n  \
         sudo reboot    # the containers and their mounts outlive the service\n  \
         sudo rm -rf {DATA_DIR} /etc/kubesolo {STATE_DIR} /etc/zeabur-installer\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_binary(dir: &Path, reports: &str) -> PathBuf {
        let path = dir.join("kubesolo");
        fs::write(&path, format!("#!/bin/sh\necho {reports}\n")).unwrap();
        fs::set_permissions(&path, Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn a_missing_or_other_release_of_the_binary_is_outdated() {
        let dir = tempfile::tempdir().unwrap();
        assert!(binary_outdated(&dir.path().join("kubesolo"), "v1.2.3"));
        let old = fake_binary(dir.path(), "kubesolo v1.2.2");
        assert!(binary_outdated(&old, "v1.2.3"));
        let current = fake_binary(dir.path(), "kubesolo v1.2.3");
        assert!(!binary_outdated(&current, "v1.2.3"));
    }

    #[test]
    fn the_last_words_say_how_to_reach_the_cluster() {
        let text = done_text("solo", &["203.0.113.7".to_owned()]);
        assert!(text.contains(&format!(
            "ssh <you>@203.0.113.7 sudo cat {PERSONAL_KUBECONFIG}"
        )));
        assert!(text.contains("zeaburctl ui --context solo"));
        assert!(done_text("solo", &[]).contains("<this server>"));
    }

    #[test]
    fn uninstall_removes_kubesolo_with_plain_commands() {
        let text = uninstall_text();
        assert!(text.contains(&format!("systemctl disable --now {UNIT}")));
        let reboot = text.find("reboot").unwrap();
        let remove = text.find(&format!("rm -rf {DATA_DIR}")).unwrap();
        assert!(
            reboot < remove,
            "the data is busy until the containers are gone"
        );
        assert!(!text.contains("zeabur-installer uninstall"));
    }
}
