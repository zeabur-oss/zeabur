//! `apply`: every step checks what is already there, so running it again resumes or repairs.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use zeabur_installer_host::kubectl::{self, Kubectl};
use zeabur_installer_host::nodeprep::{self, Stage};
use zeabur_installer_host::preflight::{self, Status};
use zeabur_installer_host::{archive, chart, cmd, download, facts, files, systemd};
use zeabur_installer_plan::catalog::Encoding;
use zeabur_installer_plan::config::{AccessMode, Config};
use zeabur_installer_plan::plan::{
    self, ComponentPlan, Download, IDENTITY_NAMESPACE, IdentityPlan, LOCAL_ISSUER, MANAGER,
    NAMESPACE_MANAGER, Plan, ReleasePlan, Rke2Plan, SecretSource, Source, ZEABUR_NAMESPACE,
    ZeaburPlan,
};
use zeabur_installer_plan::rke2::{
    AUTHN_CONFIGMAP, AUTHN_FILE, ETCD_USER, HostFile, IMAGES_DIR, PREFIX, ZOT_CA_FILE,
};
use zeabur_installer_plan::storage::{self, NodePrep, Storage};

use crate::{InstallError, load, out, render};

/// Downloads and the local CA's copy.
pub(crate) const STATE_DIR: &str = "/var/lib/rancher/zeabur-installer";
/// The `ConfigMap` recording what was installed.
pub(crate) const STATE_CONFIGMAP: &str = "zeabur-installer";
pub(crate) const UNIT: &str = "rke2-server";
const RELOADS_METRIC: &str = "apiserver_authentication_config_controller_automatic_reloads_total";

pub(crate) fn run(config_path: &Path) -> Result<(), InstallError> {
    let config = load::config(config_path)?;
    let index = load::catalog(config.sources.catalog.as_deref(), STATE_DIR)?;
    let plan = plan::plan(&index, &config, &facts::read()?)?;
    out::line(render::plan(&plan).trim_end());

    check_host(
        UNIT,
        config.host.require_encrypted_disk,
        plan.node_prep.as_ref(),
    )?;
    prepare_node(plan.node_prep.as_ref())?;

    let kubectl = Kubectl::rke2();
    let started = install_rke2(&Rke2Install {
        rke2: &plan.rke2,
        files: &plan.host_files,
        image_archives: &config.sources.image_archives,
        unit: UNIT,
        server: true,
        changed: false,
    })?;
    kubectl.wait_ready(Duration::from_secs(600))?;
    if started.is_some() {
        let name = plan.facts.hostname.to_ascii_lowercase();
        kubectl::poll(
            Duration::from_mins(10),
            &format!("node {name} to be Ready"),
            || crate::join::node_ready(&kubectl, &name, started).unwrap_or(false),
        )?;
    }

    out::step("Namespaces and network policies");
    let nodes = node_addresses(&kubectl)?;
    for namespace in &plan.namespaces {
        kubectl.apply_as(
            NAMESPACE_MANAGER,
            plan::namespace_object(namespace).to_string().as_bytes(),
        )?;
        if !namespace.public {
            kubectl.apply_object(&plan::network_policy(&namespace.name, &nodes))?;
        }
    }
    kubectl.apply_object(&plan::servicelb_policy())?;

    storage(&kubectl, &plan.storage)?;

    for component in &plan.components {
        out::step(&format!("{} ({})", component.display_name, component.id));
        secrets(&kubectl, component)?;
        releases(&kubectl, component)?;
        for wait in &component.smoke {
            let mut args: Vec<&str> = wait.iter().map(String::as_str).collect();
            args.push("--timeout=5m");
            kubectl.wait(&args)?;
        }
        if component.id == plan::ZOT {
            kubectl.apply_object(&plan::zot_builder_policy())?;
        }
        if component.id == plan::ZOT && plan.zot_cluster_ip.is_some() {
            zot_ca(&kubectl, Path::new(ZOT_CA_FILE))?;
        }
    }

    let mut report = Vec::new();
    let mut link_issued = !first_install(&kubectl)?;
    if config.zeabur {
        fetch_pinned(&plan)?;
        if config.access.mode == AccessMode::Local {
            local_ca(&kubectl, &plan)?;
        }
        system_hosts(&kubectl)?;
        identity(&kubectl, &plan)?;
        zeabur(&kubectl, &plan, &config)?;
        if !link_issued {
            report.push(setup_link(&kubectl, &plan)?);
            link_issued = true;
        }
        report.extend(certificates(&kubectl, &plan));
    }
    record(&kubectl, &plan, link_issued)?;

    out::step("Done");
    if let (Some(identity), Some(zeabur)) = (&plan.identity, &plan.zeabur) {
        out::line(&format!("Zeabur:          {}", zeabur.url));
        out::line(&format!("Zeabur Identity: {}", identity.issuer));
    }
    if config.access.mode == AccessMode::Local && config.zeabur {
        out::line(&format!(
            "The local CA is in {STATE_DIR}/local-ca.crt: browsers trust these sites once it is imported."
        ));
    }
    for line in report {
        out::line(&line);
    }
    Ok(())
}

/// The preflight checks; a failed one stops `apply`.
pub(crate) fn check_host(
    unit: &str,
    require_encrypted_disk: bool,
    prep: Option<&NodePrep>,
) -> Result<(), InstallError> {
    if show_checks(unit, require_encrypted_disk, prep) {
        return Err(InstallError::Message(
            "the host is not ready; fix what failed and run apply again".to_owned(),
        ));
    }
    Ok(())
}

/// Runs and prints the preflight checks, which only read the host. Returns whether one failed.
pub(crate) fn show_checks(
    unit: &str,
    require_encrypted_disk: bool,
    prep: Option<&NodePrep>,
) -> bool {
    out::step("Checking the host");
    let mut checks = preflight::run(preflight::Options {
        cluster_running: systemd::is_active(unit),
        require_encrypted_disk,
        node: if unit == UNIT {
            preflight::Node::Server
        } else {
            preflight::Node::Agent
        },
    });
    if let Some(prep) = prep {
        checks.extend(nodeprep::check(prep, Stage::Before));
    }
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

/// Installs and starts what the planned components need on this node, before RKE2 runs their pods
/// here.
pub(crate) fn prepare_node(prep: Option<&NodePrep>) -> Result<(), InstallError> {
    let Some(prep) = prep else {
        return Ok(());
    };
    out::step(&format!("Preparing the node for {}", prep.component));
    let changed = nodeprep::prepare(prep)?;
    if changed.is_empty() {
        out::err("    already prepared");
    }
    for line in changed {
        out::err(&format!("    {line}"));
    }
    let broken: Vec<String> = nodeprep::check(prep, Stage::After)
        .into_iter()
        .filter(|c| c.status != Status::Pass)
        .map(|c| c.detail)
        .collect();
    if broken.is_empty() {
        Ok(())
    } else {
        Err(InstallError::Message(format!(
            "the node is still not ready for {}: {}",
            prep.component,
            broken.join("; ")
        )))
    }
}

/// What `install_rke2` installs: a first server, a joining server or an agent.
pub(crate) struct Rke2Install<'a> {
    pub(crate) rke2: &'a Rke2Plan,
    pub(crate) files: &'a [HostFile],
    pub(crate) image_archives: &'a [String],
    /// `rke2-server` or `rke2-agent`.
    pub(crate) unit: &'a str,
    /// A server runs etcd, which the cis profile runs as the `etcd` user.
    pub(crate) server: bool,
    /// Something the caller wrote already needs a restart.
    pub(crate) changed: bool,
}

/// Installs RKE2 and starts or restarts it as needed. Returns when it did, in seconds since the
/// Unix epoch, taken just before; a Ready condition older than that is the old kubelet's.
pub(crate) fn install_rke2(install: &Rke2Install<'_>) -> Result<Option<u64>, InstallError> {
    let unit = install.unit;
    out::step(&format!("RKE2 {} ({unit})", install.rke2.version));
    let mut changed = install.changed;
    for file in install.files {
        let path = Path::new(&file.path);
        // The issuer configuration belongs to a later step; only make sure one exists.
        if file.path == AUTHN_FILE && path.exists() {
            continue;
        }
        changed |= files::write(path, file.mode, file.content.as_bytes())?;
    }
    cmd::run(
        "sysctl",
        &["--quiet", "--load=/etc/sysctl.d/60-rke2-cis.conf"],
        None,
    )?;
    if install.server && !cmd::succeeds("id", &["etcd"]) {
        let (program, args) = ETCD_USER
            .split_first()
            .ok_or_else(|| InstallError::Message("ETCD_USER".to_owned()))?;
        cmd::run(program, args, None)?;
    }

    let installed =
        cmd::run(Path::new(PREFIX).join("bin/rke2"), &["--version"], None).unwrap_or_default();
    if !installed.contains(&install.rke2.version) {
        let agent = download::agent();
        let dir = PathBuf::from(STATE_DIR)
            .join("downloads")
            .join(&install.rke2.version);
        let mut list = None;
        for d in &install.rke2.downloads {
            let dest = if d.file.contains("-images-") {
                Path::new(IMAGES_DIR).join(&d.file)
            } else {
                dir.join(&d.file)
            };
            out::err(&format!("    {}", d.url));
            let actual = download::fetch(&agent, &d.url, &dest, d.sha256.as_deref())?;
            if d.file.starts_with("sha256sum-") {
                list = Some(
                    fs::read_to_string(&dest).map_err(|e| InstallError::Message(e.to_string()))?,
                );
            } else if let (None, Some(list)) = (&d.sha256, &list) {
                download::check_against_list(list, &dest, &actual)?;
            }
        }
        let tarball = install
            .rke2
            .downloads
            .iter()
            .find(|d| d.file.ends_with(".tar.gz"))
            .ok_or_else(|| InstallError::Message("no RKE2 tarball in the plan".to_owned()))?;
        archive::unpack_tar_gz(&dir.join(&tarball.file), Path::new(PREFIX))?;
        systemd::daemon_reload()?;
        changed = true;
    }
    changed |= image_archives(install.image_archives)?;

    let now = || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    };
    if !systemd::is_active(unit) {
        out::err(&format!(
            "    starting {unit}; the first start takes a few minutes"
        ));
        let started = now();
        systemd::enable_now(unit)?;
        Ok(Some(started))
    } else if changed {
        out::err(&format!("    restarting {unit} for the new configuration"));
        let started = now();
        systemd::restart(unit)?;
        Ok(Some(started))
    } else {
        Ok(None)
    }
}

/// The name prefix of the archives the installer copies into RKE2's image directory, so it can
/// tell its own from RKE2's and from those put there by hand.
const ARCHIVE_PREFIX: &str = "zeabur-installer-";

/// Copies `sources.imageArchives` into RKE2's image directory and removes the ones it copied that
/// are no longer listed. Returns whether an archive was copied.
///
/// RKE2 imports every archive there each time it starts, replacing the tags it holds, so an
/// archive left behind would bring an old image back over a newer one. With `.cache.json` in the
/// directory it imports an archive again only when the file changes (k3s
/// `pkg/agent/containerd/watcher.go`); a changed one is imported at once, without a restart.
fn image_archives(sources: &[String]) -> Result<bool, InstallError> {
    let dir = Path::new(IMAGES_DIR);
    let io = |path: &Path| {
        let path = path.display().to_string();
        move |e: std::io::Error| InstallError::Message(format!("{path}: {e}"))
    };
    fs::create_dir_all(dir).map_err(io(dir))?;
    let cache = dir.join(".cache.json");
    if !cache.exists() {
        files::write(&cache, 0o644, b"")?;
    }
    let mut changed = false;
    let mut names = Vec::new();
    for archive in sources {
        let source = Path::new(archive);
        let name = source
            .file_name()
            .ok_or_else(|| InstallError::Message(format!("{archive}: not a file")))?
            .to_string_lossy()
            .into_owned();
        let dest = dir.join(format!("{ARCHIVE_PREFIX}{name}"));
        if download::sha256_file(source)? != download::sha256_file(&dest).unwrap_or_default() {
            // RKE2 does not import a `.partial` file, so it never reads half an archive.
            let partial = dir.join(format!(".{ARCHIVE_PREFIX}{name}.partial"));
            fs::copy(source, &partial).map_err(io(source))?;
            fs::rename(&partial, &dest).map_err(io(&dest))?;
            out::err(&format!("    {archive} copied to {}", dest.display()));
            changed = true;
        }
        names.push(name);
    }
    let present: Vec<String> = fs::read_dir(dir)
        .map_err(io(dir))?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    for name in stale_archives(&present, &names) {
        let path = dir.join(&name);
        fs::remove_file(&path).map_err(io(&path))?;
        out::err(&format!(
            "    removed {}: no longer in sources.imageArchives",
            path.display()
        ));
    }
    for name in present.iter().filter(|n| !n.starts_with('.')) {
        let path = dir.join(name);
        whole_second_mtime(&path).map_err(io(&path))?;
    }
    Ok(changed)
}

/// Drops the fraction of a second from a file's modification time. RKE2's cache keeps whole
/// seconds and imports a file again when its time is later, so a fraction makes it import every
/// archive on every start.
fn whole_second_mtime(path: &Path) -> std::io::Result<()> {
    let Ok(file) = fs::File::options().write(true).open(path) else {
        return Ok(());
    };
    let modified = file.metadata()?.modified()?;
    let since = modified.duration_since(UNIX_EPOCH).unwrap_or_default();
    if since.subsec_nanos() != 0 {
        file.set_modified(UNIX_EPOCH + Duration::from_secs(since.as_secs()))?;
    }
    Ok(())
}

/// The archives in the image directory to remove, given the file names of `sources.imageArchives`:
/// the installer's copies of archives no longer listed. Files it did not copy stay.
fn stale_archives(present: &[String], sources: &[String]) -> Vec<String> {
    present
        .iter()
        .filter(|name| {
            name.strip_prefix(ARCHIVE_PREFIX)
                .is_some_and(|source| !sources.iter().any(|s| s == source))
        })
        .cloned()
        .collect()
}

/// local-path, the default class unless the plan makes Longhorn's the default. A default class the
/// installer did not make is kept, and then local-path is not added; with Longhorn as the default
/// it is refused rather than left to chance. Longhorn's own class follows its chart values.
fn storage(kubectl: &Kubectl, plan: &Storage) -> Result<(), InstallError> {
    out::step("Storage");
    let classes = kubectl.run(&["get", "storageclass", "--output=json"], None)?;
    let classes: Value = serde_json::from_str(&classes)
        .map_err(|e| InstallError::Message(format!("reading storage classes: {e}")))?;
    let defaults = storage::default_classes(&classes);
    let ours = [storage::LOCAL_PATH_CLASS, storage::LONGHORN_CLASS];
    let local_default = plan.default_class == storage::LOCAL_PATH_CLASS;
    if let Some(other) = defaults.iter().find(|c| !ours.contains(&c.as_str())) {
        if !local_default {
            return Err(InstallError::Message(format!(
                "{other} is the default storage class, and the plan makes {} the default too. \
                 Take the default off {other} (kubectl annotate storageclass {other} \
                 {}-) or disable {}, then run apply again",
                plan.default_class,
                storage::DEFAULT_CLASS_ANNOTATION,
                storage::LONGHORN
            )));
        }
        out::err(&format!(
            "    {other} is already the default storage class, kept"
        ));
        return Ok(());
    }
    kubectl.apply(local_path_manifest(local_default).as_bytes())?;
    if !local_default && defaults.iter().any(|c| c == storage::LOCAL_PATH_CLASS) {
        out::err(&format!(
            "    {} is no longer the default storage class; {} will be. Volumes already on {} \
             stay there",
            storage::LOCAL_PATH_CLASS,
            plan.default_class,
            storage::LOCAL_PATH_CLASS
        ));
    }
    kubectl.wait(&[
        "--namespace=local-path-storage",
        "deployment/local-path-provisioner",
        "--for=condition=Available",
        "--timeout=5m",
    ])?;
    Ok(())
}

/// rancher's local-path-provisioner, its class the default or not.
fn local_path_manifest(default: bool) -> String {
    let manifest = include_str!("../manifests/local-path-storage.yaml");
    if default {
        manifest.to_owned()
    } else {
        manifest.replace(
            &format!("{}: \"true\"", storage::DEFAULT_CLASS_ANNOTATION),
            &format!("{}: \"false\"", storage::DEFAULT_CLASS_ANNOTATION),
        )
    }
}

/// The policy that keeps the domains of `zeabur` and `zeabur-identity` to them, and its bindings.
/// A binding that exists stays as it is, so one an admin set to `Warn` stays so.
fn system_hosts(kubectl: &Kubectl) -> Result<(), InstallError> {
    for (binding, document) in system_hosts_documents()? {
        if let Some(name) = binding
            && kubectl
                .get("validatingadmissionpolicybinding", &name, None)?
                .is_some()
        {
            continue;
        }
        kubectl.apply(document.as_bytes())?;
    }
    Ok(())
}

/// The documents of `manifests/system-hosts.yaml`, each with its name when it is a binding.
fn system_hosts_documents() -> Result<Vec<(Option<String>, &'static str)>, InstallError> {
    include_str!("../manifests/system-hosts.yaml")
        .split("\n---\n")
        .map(|document| {
            let object: Value = serde_saphyr::from_str(document)
                .map_err(|e| InstallError::Message(format!("reading system-hosts.yaml: {e}")))?;
            let binding = (object.get("kind").and_then(Value::as_str)
                == Some("ValidatingAdmissionPolicyBinding"))
            .then(|| object.pointer("/metadata/name").and_then(Value::as_str))
            .flatten()
            .map(str::to_owned);
            Ok((binding, document))
        })
        .collect()
}

/// Where traffic from the nodes comes from: the first address of each node's pod range, which a
/// node's own traffic to pods on other nodes leaves from (Canal's VXLAN interface).
pub(crate) fn node_addresses(kubectl: &Kubectl) -> Result<Vec<String>, InstallError> {
    let nodes = kubectl.run(&["get", "nodes", "--output=json"], None)?;
    let nodes: Value = serde_json::from_str(&nodes)
        .map_err(|e| InstallError::Message(format!("reading nodes: {e}")))?;
    Ok(plan::node_sources(&nodes))
}

/// Creates the component's Secrets; generated values are made once and kept.
pub(crate) fn secrets(kubectl: &Kubectl, component: &ComponentPlan) -> Result<(), InstallError> {
    let mut by_secret: BTreeMap<(&str, &str), Vec<_>> = BTreeMap::new();
    for s in &component.secrets {
        by_secret
            .entry((s.secret.namespace.as_str(), s.secret.name.as_str()))
            .or_default()
            .push(s);
    }
    for ((namespace, name), keys) in by_secret {
        let existing = kubectl.get("secret", name, Some(namespace))?;
        let mut data = BTreeMap::new();
        for s in keys {
            let value = match &s.source {
                SecretSource::File(path) => files::read_secret(Path::new(path))?,
                SecretSource::Generate { bytes, encoding } => match existing
                    .as_ref()
                    .and_then(|e| kubectl::secret_key(e, &s.secret.key))
                {
                    Some(value) => value,
                    None => encode(&files::random_bytes(*bytes)?, *encoding).into_bytes(),
                },
            };
            data.insert(s.secret.key.clone(), value);
        }
        kubectl.apply_object(&kubectl::secret(namespace, name, &data))?;
    }
    Ok(())
}

fn encode(bytes: &[u8], encoding: Encoding) -> String {
    match encoding {
        Encoding::Hex => hex::encode(bytes),
        Encoding::Base64url => URL_SAFE_NO_PAD.encode(bytes),
    }
}

pub(crate) fn releases(kubectl: &Kubectl, component: &ComponentPlan) -> Result<(), InstallError> {
    for release in &component.releases {
        match release {
            ReleasePlan::HelmChart {
                name,
                object,
                digest,
            } => match digest {
                Some(digest) => helm_chart(kubectl, name, &checked_chart(object, digest)?)?,
                None => helm_chart(kubectl, name, object)?,
            },
            ReleasePlan::Manifests {
                namespace, yaml, ..
            } => {
                kubectl.apply_in(namespace, yaml.as_bytes())?;
            }
            ReleasePlan::Urls {
                name,
                namespace,
                urls,
                digests,
                skip_if_present,
            } => {
                let mut present = false;
                for crd in skip_if_present {
                    present |= kubectl
                        .get("customresourcedefinition", crd, None)?
                        .is_some();
                }
                if present {
                    out::err(&format!(
                        "    {name}: already installed by something else, skipped"
                    ));
                    continue;
                }
                if !digests.is_empty() && digests.len() != urls.len() {
                    return Err(InstallError::Message(format!(
                        "{name}: the catalog has {} URLs but {} digests",
                        urls.len(),
                        digests.len()
                    )));
                }
                let agent = download::agent();
                for (i, url) in urls.iter().enumerate() {
                    let text =
                        download::fetch_text(&agent, url, digests.get(i).map(String::as_str))?;
                    kubectl.apply_in(namespace, text.as_bytes())?;
                }
            }
        }
    }
    Ok(())
}

/// `object` with the chart downloaded and checked against `digest` in `chartContent`, which
/// helm-controller installs instead of fetching `chart` itself [helm-controller].
///
/// [helm-controller]: https://docs.rke2.io/helm#helmchart-field-definitions
fn checked_chart(object: &Value, digest: &str) -> Result<Value, InstallError> {
    let field = |key: &str| {
        object
            .pointer(&format!("/spec/{key}"))
            .and_then(Value::as_str)
    };
    let name = field("chart")
        .ok_or_else(|| InstallError::Message("a HelmChart without a chart".to_owned()))?;
    let chart = chart::fetch(
        &download::agent(),
        field("repo"),
        name,
        field("version").unwrap_or_default(),
        digest,
    )?;
    let mut object = object.clone();
    if let Some(spec) = object.get_mut("spec").and_then(Value::as_object_mut) {
        for key in ["chart", "repo", "version"] {
            spec.remove(key);
        }
        spec.insert("chartContent".to_owned(), json!(STANDARD.encode(chart)));
    }
    Ok(object)
}

/// Applies a `HelmChart` and waits for helm-controller's `helm upgrade --install` job, so running
/// again is safe. helm-controller replaces the job within seconds when the chart changed and keeps
/// the finished one when it did not; which of the two happened is only known by watching.
fn helm_chart(kubectl: &Kubectl, name: &str, object: &Value) -> Result<(), InstallError> {
    const SETTLE: Duration = Duration::from_secs(5);
    let job_name = format!("helm-install-{name}");
    let old_uid = kubectl
        .get("job", &job_name, Some("kube-system"))?
        .and_then(|j| j.pointer("/metadata/uid").cloned());
    kubectl.apply_object(object)?;
    let applied = Instant::now();

    let mut outcome = None;
    kubectl::poll(
        Duration::from_mins(15),
        &format!("the {name} chart to install"),
        || {
            let Ok(job) = kubectl.get("job", &job_name, Some("kube-system")) else {
                return false;
            };
            let settled = applied.elapsed() >= SETTLE;
            let Some(job) = job else {
                // helm-controller deletes finished jobs on its own schedule.
                if settled && old_uid.is_some() {
                    outcome = Some(true);
                }
                return outcome.is_some();
            };
            if job.pointer("/metadata/uid") == old_uid.as_ref() && !settled {
                return false;
            }
            if job
                .pointer("/status/succeeded")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                > 0
            {
                outcome = Some(true);
            } else if job
                .pointer("/status/failed")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                > 0
                && conditions(&job).any(|(kind, status)| kind == "Failed" && status == "True")
            {
                outcome = Some(false);
            }
            outcome.is_some()
        },
    )?;
    if outcome == Some(false) {
        let job = format!("job/{job_name}");
        let logs = kubectl
            .run(
                &["logs", "--namespace=kube-system", &job, "--tail=30"],
                None,
            )
            .unwrap_or_default();
        return Err(InstallError::Message(format!(
            "the {name} chart failed to install:\n{logs}"
        )));
    }
    Ok(())
}

fn conditions(object: &Value) -> impl Iterator<Item = (&str, &str)> {
    object
        .pointer("/status/conditions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|c| {
            (
                c.get("type").and_then(Value::as_str).unwrap_or_default(),
                c.get("status").and_then(Value::as_str).unwrap_or_default(),
            )
        })
}

fn cert_manager_namespace(plan: &Plan) -> Result<String, InstallError> {
    plan.components
        .iter()
        .filter(|c| c.id == "certificates/cert-manager")
        .flat_map(|c| &c.releases)
        .find_map(|r| match r {
            ReleasePlan::HelmChart { object, .. } => object
                .pointer("/spec/targetNamespace")
                .and_then(Value::as_str)
                .map(str::to_owned),
            ReleasePlan::Manifests { .. } | ReleasePlan::Urls { .. } => None,
        })
        .ok_or_else(|| InstallError::Message("the plan has no cert-manager chart".to_owned()))
}

fn local_ca(kubectl: &Kubectl, plan: &Plan) -> Result<(), InstallError> {
    out::step("Local certificate authority");
    let namespace = cert_manager_namespace(plan)?;
    for object in plan::local_ca_objects(&namespace, &plan.facts.hostname) {
        kubectl.apply_object(&object)?;
    }
    kubectl.wait(&[
        "--namespace",
        &namespace,
        &format!("certificate/{LOCAL_ISSUER}"),
        "--for=condition=Ready",
        "--timeout=2m",
    ])?;
    let pem = local_ca_pem(kubectl, &namespace)?;
    files::write(
        &Path::new(STATE_DIR).join("local-ca.crt"),
        0o644,
        pem.as_bytes(),
    )?;
    Ok(())
}

fn local_ca_pem(kubectl: &Kubectl, namespace: &str) -> Result<String, InstallError> {
    kubectl
        .get("secret", LOCAL_ISSUER, Some(namespace))?
        .as_ref()
        .and_then(|s| kubectl::secret_key(s, "ca.crt"))
        .and_then(|b| String::from_utf8(b).ok())
        .ok_or_else(|| {
            InstallError::Message(format!("secret {namespace}/{LOCAL_ISSUER} has no ca.crt"))
        })
}

/// The CA of zot's certificate.
pub(crate) fn zot_ca_pem(kubectl: &Kubectl) -> Result<String, InstallError> {
    kubectl
        .get("secret", "zot-tls", Some("zot"))?
        .as_ref()
        .and_then(|s| kubectl::secret_key(s, "ca.crt"))
        .and_then(|b| String::from_utf8(b).ok())
        .ok_or_else(|| InstallError::Message("secret zot/zot-tls has no ca.crt".to_owned()))
}

/// Copies the CA of zot's certificate to the node, where the registry mirror checks zot against it.
pub(crate) fn zot_ca(kubectl: &Kubectl, path: &Path) -> Result<(), InstallError> {
    files::write(path, 0o644, zot_ca_pem(kubectl)?.as_bytes())?;
    Ok(())
}

/// Where `apply` keeps a file of a pinned release.
fn pinned_path(download: &Download) -> PathBuf {
    Path::new(STATE_DIR).join("downloads").join(&download.file)
}

/// The `zeabur-identityctl` that `apply` runs.
fn identityctl_path(identity: &IdentityPlan) -> PathBuf {
    match &identity.identityctl {
        Source::Local(path) => PathBuf::from(path),
        Source::Pinned(archive) => {
            let dir = archive.file.trim_end_matches(".tar.gz");
            Path::new(STATE_DIR)
                .join("downloads")
                .join(dir)
                .join("zeabur-identityctl")
        }
    }
}

/// Downloads the planned files of pinned releases and checks their digests.
fn fetch_pinned(plan: &Plan) -> Result<(), InstallError> {
    let agent = download::agent();
    let fetch = |d: &Download| -> Result<PathBuf, InstallError> {
        let dest = pinned_path(d);
        out::err(&format!("    {}", d.url));
        download::fetch(&agent, &d.url, &dest, d.sha256.as_deref())?;
        Ok(dest)
    };
    if let Some(identity) = &plan.identity
        && let Source::Pinned(d) = &identity.identityctl
    {
        let archive = fetch(d)?;
        if let Some(dir) = identityctl_path(identity).parent() {
            archive::unpack_tar_gz(&archive, dir)?;
        }
    }
    if let Some(zeabur) = &plan.zeabur
        && let Source::Pinned(d) = &zeabur.chart
    {
        fetch(d)?;
    }
    Ok(())
}

fn identityctl(
    kubectl: &Kubectl,
    plan: &Plan,
    args: &[&str],
    stdin: Option<&[u8]>,
) -> Result<cmd::Output, InstallError> {
    let identity = plan
        .identity
        .as_ref()
        .ok_or_else(|| InstallError::Message("no identity in the plan".to_owned()))?;
    let kubeconfig = kubectl.kubeconfig().display().to_string();
    Ok(cmd::run_with(
        identityctl_path(identity),
        args,
        &[
            ("KUBECONFIG", &kubeconfig),
            ("ZEABUR_IDENTITY_ISSUER", &identity.issuer),
        ],
        stdin,
    )?)
}

fn identity(kubectl: &Kubectl, plan: &Plan) -> Result<(), InstallError> {
    let Some(identity) = &plan.identity else {
        return Ok(());
    };
    out::step("Zeabur Identity");
    kubectl.apply(
        identityctl(kubectl, plan, &["crds"], None)?
            .stdout
            .as_bytes(),
    )?;
    let mut args = vec!["manifests", "--image", &identity.image];
    if identity.invitations {
        args.push("--serve-arg=--invitations");
    }
    let manifests = identityctl(kubectl, plan, &args, None)?;
    kubectl.apply(manifests.stdout.as_bytes())?;
    kubectl.apply_object(&identity.ingress)?;
    kubectl.run(
        &[
            "rollout",
            "status",
            "--namespace",
            IDENTITY_NAMESPACE,
            "deployment/zeabur-identity",
            "--timeout=5m",
        ],
        None,
    )?;

    // kube-apiserver reaches Zeabur Identity through its Service (internal discovery), so neither
    // DNS nor the public certificate is needed for this step.
    let reloads = |k: &Kubectl| -> u64 {
        k.run(&["get", "--raw=/metrics"], None)
            .map_or(0, |m| successful_reloads(&m))
    };
    let before = reloads(kubectl);
    // RKE2 runs k3s's server, so its file is k3s's (see `EMPTY_AUTHN`).
    let config = identityctl(kubectl, plan, &["kube-config", "--distro", "k3s"], None)?.stdout;
    if trust(kubectl, &config)? {
        out::err("    waiting for kube-apiserver to trust Zeabur Identity (up to a few minutes)");
        kubectl::poll(
            Duration::from_secs(300),
            "kube-apiserver to load Zeabur Identity's configuration",
            || reloads(kubectl) > before,
        )?;
    }
    Ok(())
}

/// Writes kube-apiserver's authentication configuration on this server and in the cluster, where
/// the other servers copy it from (`join::sync_authn`). Returns whether this server's file changed.
pub(crate) fn trust(kubectl: &Kubectl, config: &str) -> Result<bool, InstallError> {
    kubectl.apply_object(&json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {
            "name": AUTHN_CONFIGMAP,
            "namespace": "kube-system",
            "labels": {"app.kubernetes.io/managed-by": MANAGER},
        },
        "data": {"authn.yaml": config},
    }))?;
    Ok(files::write(
        Path::new(AUTHN_FILE),
        0o600,
        config.as_bytes(),
    )?)
}

/// The sum of kube-apiserver's successful authentication-configuration reloads.
fn successful_reloads(metrics: &str) -> u64 {
    metrics
        .lines()
        .filter(|l| l.starts_with(RELOADS_METRIC) && l.contains("status=\"success\""))
        // A counter, printed as a whole number.
        .filter_map(|l| l.rsplit(' ').next()?.parse::<u64>().ok())
        .sum()
}

fn zeabur(kubectl: &Kubectl, plan: &Plan, config: &Config) -> Result<(), InstallError> {
    let Some(zeabur) = &plan.zeabur else {
        return Ok(());
    };
    out::step("Zeabur");
    oauth_client(kubectl, plan, &zeabur.url)?;
    let plan_key = kubectl
        .get("secret", "zeabur-plan-key", Some(ZEABUR_NAMESPACE))?
        .and_then(|secret| kubectl::secret_key(&secret, "plan-key"));
    if !plan_key.as_deref().is_some_and(is_plan_key) {
        let key: String = files::random_hex()?.chars().take(64).collect();
        let data = BTreeMap::from([("plan-key".to_owned(), key.into_bytes())]);
        kubectl.apply_object(&kubectl::secret(ZEABUR_NAMESPACE, "zeabur-plan-key", &data))?;
    }
    if config.access.mode == AccessMode::Local {
        let pem = local_ca_pem(kubectl, &cert_manager_namespace(plan)?)?;
        kubectl.apply_object(&json!({
            "apiVersion": "v1",
            "kind": "ConfigMap",
            "metadata": {
                "name": "zeabur-identity-ca",
                "namespace": ZEABUR_NAMESPACE,
                "labels": {"app.kubernetes.io/managed-by": MANAGER},
            },
            "data": {"ca.crt": pem},
        }))?;
    }

    if plan.components.iter().any(|c| c.id == plan::ZOT) {
        kubectl.apply_object(&json!({
            "apiVersion": "v1",
            "kind": "ConfigMap",
            "metadata": {
                "name": plan::REGISTRY_CA_CONFIGMAP,
                "namespace": ZEABUR_NAMESPACE,
                "labels": {"app.kubernetes.io/managed-by": MANAGER},
            },
            "data": {"ca.crt": zot_ca_pem(kubectl)?},
        }))?;
    }

    helm_chart(kubectl, "zeabur", &zeabur_helm_chart(zeabur)?)?;
    kubectl.wait(&[
        "--namespace",
        ZEABUR_NAMESPACE,
        "deployment/zeabur",
        "--for=condition=Available",
        "--timeout=5m",
    ])?;
    Ok(())
}

/// The `HelmChart` that installs Zeabur's chart with the planned values.
pub(crate) fn zeabur_helm_chart(zeabur: &ZeaburPlan) -> Result<Value, InstallError> {
    let chart = match &zeabur.chart {
        Source::Local(path) => chart_content(path)?,
        Source::Pinned(d) => {
            let path = pinned_path(d);
            fs::read(&path)
                .map_err(|e| InstallError::Message(format!("{}: {e}", path.display())))?
        }
    };
    Ok(json!({
        "apiVersion": "helm.cattle.io/v1",
        "kind": "HelmChart",
        "metadata": {
            "name": "zeabur",
            "namespace": "kube-system",
            "labels": {"app.kubernetes.io/managed-by": MANAGER},
        },
        "spec": {
            "chartContent": STANDARD.encode(chart),
            "targetNamespace": ZEABUR_NAMESPACE,
            "createNamespace": false,
            "valuesContent": zeabur.values.to_string(),
            "failurePolicy": "abort",
            "timeout": "10m0s",
        },
    }))
}

/// Zeabur's OAuth client and the Secret holding its secret.
fn oauth_client(kubectl: &Kubectl, plan: &Plan, url: &str) -> Result<(), InstallError> {
    let client = kubectl.get(
        "oauthclients.identity.zeabur.com",
        "zeabur",
        Some(IDENTITY_NAMESPACE),
    )?;
    let secret = kubectl.get("secret", "zeabur-oidc", Some(ZEABUR_NAMESPACE))?;
    let home = format!("{url}/");
    if client.is_none() || secret.is_none() {
        let redirect = format!("{url}/auth/callback");
        let created = identityctl(
            kubectl,
            plan,
            &[
                "client",
                "create",
                "zeabur",
                "--type",
                "confidential",
                "--audience",
                "kubernetes",
                "--display-name",
                "Zeabur",
                "--home-url",
                &home,
                "--redirect-uri",
                &redirect,
            ],
            None,
        )?;
        let client_secret = created.stderr.lines().last().ok_or_else(|| {
            InstallError::Message("zeabur-identityctl printed no client secret".to_owned())
        })?;
        kubectl.apply(created.stdout.as_bytes())?;
        let data = BTreeMap::from([(
            "client-secret".to_owned(),
            client_secret.trim().as_bytes().to_vec(),
        )]);
        kubectl.apply_object(&kubectl::secret(ZEABUR_NAMESPACE, "zeabur-oidc", &data))?;
    } else if client
        .as_ref()
        .and_then(|c| c.pointer("/spec/homeUrl"))
        .and_then(Value::as_str)
        != Some(home.as_str())
    {
        // Clients made before Zeabur Identity knew home URLs are left out of its list of apps.
        kubectl.run(
            &[
                "patch",
                "oauthclients.identity.zeabur.com",
                "zeabur",
                "--namespace",
                IDENTITY_NAMESPACE,
                "--type=merge",
                "--patch",
                &json!({ "spec": { "homeUrl": home } }).to_string(),
            ],
            None,
        )?;
    }
    Ok(())
}

/// A packaged chart, or a chart directory packed the way `helm package` lays it out.
/// Zeabur reads its plan-signing key as 32 bytes written in 64 hexadecimal digits.
fn is_plan_key(key: &[u8]) -> bool {
    key.len() == 64 && key.iter().all(u8::is_ascii_hexdigit)
}

fn chart_content(source: &str) -> Result<Vec<u8>, InstallError> {
    let path = Path::new(source);
    if path.is_dir() {
        let name = path
            .file_name()
            .map_or_else(|| "zeabur".to_owned(), |n| n.to_string_lossy().into_owned());
        return Ok(archive::pack_chart(path, &name)?);
    }
    fs::read(path).map_err(|e| {
        InstallError::Message(format!(
            "{source}: {e}; OCI chart references are not supported yet"
        ))
    })
}

/// No setup link was issued by an earlier `apply`.
fn first_install(kubectl: &Kubectl) -> Result<bool, InstallError> {
    let state = kubectl.get("configmap", STATE_CONFIGMAP, Some("kube-system"))?;
    Ok(state.and_then(|s| s.pointer("/data/setupLinkIssued").cloned()) != Some(json!("true")))
}

fn setup_link(kubectl: &Kubectl, plan: &Plan) -> Result<String, InstallError> {
    let link = identityctl(
        kubectl,
        plan,
        &["setup-link", "--username", "admin", "--valid-for", "24h"],
        None,
    )?;
    let program = plan
        .identity
        .as_ref()
        .map_or_else(PathBuf::new, identityctl_path);
    Ok(format!(
        "\nEnroll the first admin's passkey (works once, for 24 hours):\n  {}\nA new link: {} setup-link --username admin",
        link.stdout.trim(),
        program.display()
    ))
}

/// Waits briefly for the two certificates; a slow ACME order is reported, not fatal.
fn certificates(kubectl: &Kubectl, plan: &Plan) -> Vec<String> {
    let mut report = Vec::new();
    for (namespace, name, host) in [
        (
            IDENTITY_NAMESPACE,
            "zeabur-identity-tls",
            plan.identity.as_ref().map(|i| i.host.clone()),
        ),
        (
            ZEABUR_NAMESPACE,
            "zeabur-tls",
            plan.zeabur
                .as_ref()
                .map(|z| z.url.trim_start_matches("https://").to_owned()),
        ),
    ] {
        let certificate = format!("certificate/{name}");
        if kubectl
            .wait(&[
                "--namespace",
                namespace,
                &certificate,
                "--for=condition=Ready",
                "--timeout=3m",
            ])
            .is_err()
        {
            report.push(format!(
                "warning: the certificate for {} is not ready yet. Check that its DNS name points at this \
                 server and port 80 is reachable: kubectl describe --namespace {namespace} {certificate}",
                host.unwrap_or_default()
            ));
        }
    }
    report
}

/// Records what was installed, for `doctor` and the next `apply`.
fn record(kubectl: &Kubectl, plan: &Plan, link_issued: bool) -> Result<(), InstallError> {
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
            "rke2": plan.rke2.version,
            "components": components.join(","),
            "plan": summary,
            "setupLinkIssued": if link_issued { "true" } else { "false" },
        },
    }))?;
    Ok(())
}

#[cfg(test)]
mod tests {

    #[test]
    fn system_hosts_bind_zeabur_and_identity() {
        let mut protected = Vec::new();
        for (binding, document) in system_hosts_documents().unwrap() {
            let object: Value = serde_saphyr::from_str(document).unwrap();
            if binding.is_some() {
                assert_eq!(object["spec"]["policyName"], "zeabur-system-hosts");
                protected.push(object["spec"]["paramRef"]["namespace"].clone());
            }
        }
        protected.sort_by_key(ToString::to_string);
        assert_eq!(protected, [ZEABUR_NAMESPACE, IDENTITY_NAMESPACE]);
    }

    #[test]
    fn generated_values_use_the_encoding() {
        assert_eq!(encode(&[0xFB, 0xFF], Encoding::Hex), "fbff");
        assert_eq!(encode(&[0xFB, 0xFF], Encoding::Base64url), "-_8");
        assert_eq!(encode(&[0; 24], Encoding::Base64url).len(), 32);
    }

    #[test]
    fn whole_second_mtime_drops_the_fraction() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.tar");
        let file = fs::File::create(&path).unwrap();
        file.set_modified(UNIX_EPOCH + Duration::new(1_700_000_000, 250_000_000))
            .unwrap();
        whole_second_mtime(&path).unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(modified, UNIX_EPOCH + Duration::from_secs(1_700_000_000));
    }
    use super::*;

    #[test]
    fn local_path_is_the_default_unless_longhorn_is() {
        let default = format!("{}: \"true\"", storage::DEFAULT_CLASS_ANNOTATION);
        assert!(local_path_manifest(true).contains(&default));
        let plain = local_path_manifest(false);
        assert!(!plain.contains(&default));
        assert!(plain.contains(&format!("{}: \"false\"", storage::DEFAULT_CLASS_ANNOTATION)));
    }

    #[test]
    fn plan_keys_are_32_bytes_in_hex() {
        assert!(is_plan_key(&[b'a'; 64]));
        assert!(!is_plan_key(&[b'a'; 128]));
        assert!(!is_plan_key(&[b'g'; 64]));
    }

    #[test]
    fn archives_no_longer_listed_go_and_others_stay() {
        let present: Vec<String> = [
            "rke2-images-core.linux-arm64.tar.zst",
            "zeabur.tar",
            "zeabur-installer-zeabur.tar",
            "zeabur-installer-old.tar",
            "mine.tar",
            ".cache.json",
        ]
        .map(str::to_owned)
        .to_vec();
        let sources = ["zeabur.tar".to_owned()];
        assert_eq!(
            stale_archives(&present, &sources),
            ["zeabur-installer-old.tar"]
        );
        assert_eq!(
            stale_archives(&present, &[]),
            ["zeabur-installer-zeabur.tar", "zeabur-installer-old.tar"]
        );
    }

    #[test]
    fn reload_metric() {
        let metrics = "# HELP x\n\
            apiserver_authentication_config_controller_automatic_reloads_total{apiserver_id_hash=\"a\",status=\"success\"} 2\n\
            apiserver_authentication_config_controller_automatic_reloads_total{apiserver_id_hash=\"a\",status=\"failure\"} 5\n";
        assert_eq!(successful_reloads(metrics), 2);
        assert_eq!(successful_reloads(""), 0);
    }
}
