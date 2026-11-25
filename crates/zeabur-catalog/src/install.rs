//! How to install a catalogue component, in four forms: Helm commands, a K3s/RKE2 `HelmChart`, Flux
//! objects and an Argo CD `Application`.
//!
//! Each is a text to copy, or to commit to a Git repository a GitOps tool watches
//! (`zeaburctl components export`). Each renders the component's releases in order, with the
//! catalogue's pinned versions and its values for the chosen profile (`single-node` or
//! `multi-node`): the release's values, with the profile's merged over them as Helm merges values
//! files, maps merged and lists replaced [helm-values].
//!
//! - **Helm** keeps the catalogue's two values files apart, comments and all, and passes both
//!   (`--values base --values profile`), so the reasons for each value stay readable. `helm upgrade
//!   --install` installs or upgrades in one command [helm-upgrade]; a chart from an HTTP repository
//!   is named with `--repo`.
//! - **K3s/RKE2** writes a `HelmChart` for the Helm controller both run, which installs it; the
//!   file can go in the server's auto-deploying manifests directory [k3s-helm] [rke2-helm]. Values
//!   are merged into `valuesContent`.
//! - **Flux** writes the chart's source (`OCIRepository` for an OCI chart, pinned to the catalog's
//!   digest, or `HelmRepository` for an HTTP repository) and a `HelmRelease` that waits for the
//!   component's chart release before it [flux-helmrelease] [flux-ocirepository]. Namespaces and
//!   Secrets are kept when Flux prunes, as Zeabur keeps them on removal.
//! - **Argo CD** writes an `Application` with the values in `helm.valuesObject`; an OCI chart also
//!   needs a repository Secret with `enableOCI` [argocd-helm].
//!
//! A component k3s also packages (Traefik, metrics-server) cannot run beside k3s's copy: two
//! Traefiks compete for ports 80 and 443, two metrics-servers for the `v1beta1.metrics.k8s.io`
//! `APIService`, and the k3s form's `HelmChart kube-system/<name>` would overwrite k3s's own, which
//! k3s re-applies on its next restart. So every form, on k3s, begins with how to disable the
//! packaged one first (`disable` in `/etc/rancher/k3s/config.yaml` on every server, then a
//! restart), and what that removes [k3s-packaged]. RKE2 packages its own copies, which the
//! catalog names per distribution, and chooses its ingress controller with `ingress-controller`
//! in `/etc/rancher/rke2/config.yaml` [rke2-packaged] [rke2-ingress], so the comment gives that
//! form too. The guide lists the k3s add-ons.
//!
//! Releases that are manifests (by URL or inline) are applied with `kubectl apply
//! --server-side` in the Helm form, and listed in the others for the person's own repository.
//! Inputs the catalogue asks for appear as `<name>` placeholders; generated ones are produced by
//! `openssl rand` in the Helm form and marked in the others. Secrets an input is stored
//! in are created before the releases.
//!
//! [helm-values]: https://helm.sh/docs/chart_template_guide/values_files/
//! [helm-upgrade]: https://helm.sh/docs/helm/helm_upgrade/
//! [k3s-helm]: https://docs.k3s.io/add-ons/helm
//! [flux-helmrelease]: https://fluxcd.io/flux/components/helm/helmreleases/
//! [flux-ocirepository]: https://fluxcd.io/flux/components/source/ocirepositories/
//! [argocd-helm]: https://argo-cd.readthedocs.io/en/stable/user-guide/helm/
//! [k3s-packaged]: https://docs.k3s.io/installation/packaged-components
//! [rke2-helm]: https://docs.rke2.io/helm
//! [rke2-packaged]: https://docs.rke2.io/advanced
//! [rke2-ingress]: https://docs.rke2.io/networking/networking_services

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;
use zeabur_catalog_format::common::{Distribution, Encoding, Generate, Profile, Reference};
use zeabur_catalog_format::component::{self, Component, Input, InputValue, ProfileValues};
use zeabur_kube::k8s_openapi::Resource;
use zeabur_model::PodSecurityLevel;
use zeabur_model::kernel::isolation::PLATFORM_LABEL;
use zeabur_model::kernel::names::is_system_namespace;

use crate::engine::{self, ChartRelease};
use crate::{bundle, zeabur};

/// The profiles the catalogue sizes components for.
pub const PROFILES: [&str; 2] = ["single-node", "multi-node"];

/// One way to install a component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum InstallFormat {
    /// Shell commands: `kubectl` and `helm upgrade --install`.
    Helm,
    /// A K3s/RKE2 `HelmChart` object.
    K3s,
    /// Flux source and `HelmRelease` objects.
    Flux,
    /// An Argo CD `Application`.
    ArgoCd,
}

/// One rendering of the install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallText {
    /// The form.
    pub format: InstallFormat,
    /// The text to copy: shell for Helm, YAML for the others.
    pub text: String,
}

/// How privileged a component is, from the catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Security {
    /// The Pod Security level its namespaces need: `restricted`, `baseline` or `privileged`.
    pub pod_security: String,
}

/// A value the install needs from the person, or generates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallInput {
    /// Its name, the `<name>` placeholder in the texts.
    pub name: String,
    /// What it is.
    pub description: String,
    /// It must be given.
    pub required: bool,
    /// It is generated at random, not asked for.
    pub generated: bool,
}

/// How to install one catalogue component, for one profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallGuide {
    /// Its id (`traefik`).
    pub component: String,
    /// The profile the texts are for.
    pub profile: String,
    /// Every profile the catalogue sizes it for.
    pub profiles: Vec<String>,
    /// What it needs installed first: a component id, or a capability that any of its
    /// components provides.
    pub requires: Vec<String>,
    /// Its RBAC and network access.
    pub security: Security,
    /// Values asked for or generated.
    pub inputs: Vec<InstallInput>,
    /// Other components whose releases it adds values to, by id; those values
    /// are not in these texts.
    pub contributes_to: Vec<String>,
    /// K3s packaged components of the same kind (`traefik`, `metrics-server`) that must be
    /// turned off on K3s, or RKE2's copies on RKE2, before installing this one; each text begins
    /// by saying how.
    pub replaces_k3s_packaged: Vec<String>,
    /// The texts, one per form.
    pub formats: Vec<InstallText>,
}

/// Why a guide could not be rendered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InstallError {
    /// No such component in the catalogue.
    #[error("Zeabur Catalog has no component {0}")]
    UnknownComponent(String),
    /// The component needs something this build does not know: a release type, a Pod Security
    /// level or a way to generate an input.
    #[error("Zeabur Catalog, {0}: this build cannot install it ({1})")]
    Unknown(String, String),
    /// No such profile.
    #[error("unknown profile {0}; use single-node or multi-node")]
    UnknownProfile(String),
    /// The catalogue file or its values do not parse; a test renders them all.
    #[error("Zeabur Catalog, {0}: {1}")]
    Catalogue(String, String),
}

/// The shell command that makes a random value like the one Zeabur generates.
fn random(input: &Input) -> Option<String> {
    let Some(Generate::Random { bytes, encoding }) = &input.generate else {
        return None;
    };
    Some(if *encoding == Encoding::Base64url {
        format!("openssl rand -base64 {bytes} | tr '+/' '-_' | tr -d '='")
    } else {
        format!("openssl rand -hex {bytes}")
    })
}

/// Keeps an object when Flux prunes: a Namespace holds the volumes a component leaves, and a
/// Secret the inputs a later install reads back [flux-prune].
///
/// [flux-prune]: https://fluxcd.io/flux/components/kustomize/kustomizations/#prune
const FLUX_PRUNE: &str = "kustomize.toolkit.fluxcd.io/prune";

/// A release of the file, as the forms render it.
struct Release<'a> {
    name: &'a str,
    namespace: &'a str,
    /// The Helm release name; `name` for manifests.
    helm_name: &'a str,
    skip_if_present: &'a [String],
    source: Source<'a>,
    values: Option<&'a str>,
    profiles: Option<&'a ProfileValues>,
}

/// What one release installs.
enum Source<'a> {
    /// A chart from an OCI registry: `oci://host/path/name`.
    Oci {
        url: &'a str,
        version: &'a str,
        /// The chart's manifest digest.
        digest: &'a str,
    },
    /// A chart from an HTTP repository.
    Repo {
        repo: &'a str,
        chart: &'a str,
        version: &'a str,
    },
    /// Manifests at these URLs.
    Urls(Vec<String>),
    /// Manifests as written.
    Manifests(&'a str),
}

impl<'a> Release<'a> {
    fn new(release: &'a component::Release) -> Result<Self, String> {
        Ok(match release {
            component::Release::Helm(r) => Self {
                name: &r.name,
                namespace: &r.namespace,
                helm_name: r.helm_name(),
                skip_if_present: &r.skip_if_present,
                source: match &r.repo {
                    None if r.chart.starts_with("oci://") => Source::Oci {
                        url: &r.chart,
                        version: &r.version,
                        digest: &r.chart_digest,
                    },
                    Some(repo) => Source::Repo {
                        repo,
                        chart: &r.chart,
                        version: &r.version,
                    },
                    None => return Err(format!("release {}: chart has no repository", r.name)),
                },
                values: r.values.as_deref(),
                profiles: Some(&r.profiles),
            },
            component::Release::Manifest(r) => Self {
                name: &r.name,
                namespace: &r.namespace,
                helm_name: &r.name,
                skip_if_present: &r.skip_if_present,
                source: Source::Urls(r.resolved_urls()),
                values: None,
                profiles: None,
            },
            component::Release::Inline(r) => Self {
                name: &r.name,
                namespace: &r.namespace,
                helm_name: &r.name,
                skip_if_present: &r.skip_if_present,
                source: Source::Manifests(&r.manifests),
                values: None,
                profiles: None,
            },
            component::Release::Unknown => return Err("a release type it does not know".to_owned()),
        })
    }

    const fn is_chart(&self) -> bool {
        matches!(self.source, Source::Oci { .. } | Source::Repo { .. })
    }
}

/// The first thing `file` names that this build does not know: a release type, a Pod Security
/// level or a way to generate an input.
fn unknown(file: &Component) -> Option<String> {
    if file
        .releases
        .iter()
        .any(|r| matches!(r, component::Release::Unknown))
    {
        return Some("a release type".to_owned());
    }
    if engine::pod_security(file).is_none() {
        return Some("a Pod Security level".to_owned());
    }
    file.inputs
        .iter()
        .find(|i| matches!(i.generate, Some(Generate::Unknown)))
        .map(|i| format!("how to generate {}", i.name))
}

/// How to install the component `id` for `profile`.
///
/// Each text ends with the objects Zeabur adds to the component ([`zeabur::objects`]) for a
/// Zeabur that runs in `zeabur_namespace`. Apply them once the component is installed.
///
/// # Errors
///
/// An unknown component or profile, or a catalogue file that does not parse.
pub fn guide(
    component: &str,
    profile: &str,
    zeabur_namespace: &str,
) -> Result<InstallGuide, InstallError> {
    let Some(profile) = crate::api::profile(profile) else {
        return Err(InstallError::UnknownProfile(profile.to_owned()));
    };
    let text = bundle::FILES
        .iter()
        .find(|(id, _)| *id == component)
        .map(|(_, text)| *text)
        .ok_or_else(|| InstallError::UnknownComponent(component.to_owned()))?;
    let catalogue = |message: String| InstallError::Catalogue(component.to_owned(), message);
    let file: Component = serde_saphyr::from_str(text).map_err(|e| catalogue(e.to_string()))?;
    if let Some(unknown) = unknown(&file) {
        return Err(InstallError::Unknown(component.to_owned(), unknown));
    }
    let rendered = Rendered::new(component, &file, profile).map_err(catalogue)?;
    let zeaburs = zeabur::yaml(component, zeabur_namespace).map_err(catalogue)?;
    Ok(InstallGuide {
        component: component.to_owned(),
        profile: profile.as_str().to_owned(),
        profiles: Profile::ALL.iter().map(|p| p.as_str().to_owned()).collect(),
        requires: file
            .requires
            .iter()
            .map(|r| match r {
                Reference::Capability(name) | Reference::Component(name) => name.clone(),
            })
            .collect(),
        security: Security {
            pod_security: engine::pod_security(&file)
                .map_or_else(String::new, |level| level.as_str().to_owned()),
        },
        inputs: file
            .inputs
            .iter()
            .map(|i| InstallInput {
                name: i.name.clone(),
                description: i.description.trim().to_owned(),
                required: i.required,
                generated: random(i).is_some(),
            })
            .collect(),
        contributes_to: file
            .contributes
            .iter()
            .map(|c| c.target.component.clone())
            .collect(),
        replaces_k3s_packaged: file
            .detection
            .addons_on(Distribution::K3s)
            .map(str::to_owned)
            .collect(),
        formats: [
            InstallFormat::Helm,
            InstallFormat::K3s,
            InstallFormat::Flux,
            InstallFormat::ArgoCd,
        ]
        .into_iter()
        .map(|format| InstallText {
            format,
            text: rendered.text(format) + &zeaburs_text(component, format, &zeaburs),
        })
        .collect(),
    })
}

/// `yaml`, the objects Zeabur adds to `component`, in `format`; empty when there are none.
fn zeaburs_text(component: &str, format: InstallFormat, yaml: &str) -> String {
    if yaml.is_empty() {
        return String::new();
    }
    let comment = format!("# Zeabur's objects for {component}: apply them once it is installed.\n");
    if format == InstallFormat::Helm {
        format!(
            "\n{comment}kubectl apply --server-side --field-manager zeabur -f - <<'EOF'\n{yaml}EOF\n"
        )
    } else {
        format!("---\n{comment}{yaml}")
    }
}

/// A Secret inputs are stored in: `(namespace, name)`, and each input with its key.
type SecretInputs<'a> = ((&'a str, &'a str), Vec<(&'a Input, &'a str)>);

/// The releases of a file, with their values worked out for one profile.
struct Rendered<'a> {
    file: &'a Component,
    releases: Vec<Release<'a>>,
    profile: Profile,
    /// Per release: the merged values (inputs set at their paths), as YAML; empty when none.
    merged: Vec<String>,
    /// The Namespace of each release, as Zeabur's installer writes it, as YAML documents.
    namespaces: String,
    /// The same, kept when Flux prunes.
    flux_namespaces: String,
    /// Per release: its `HelmChart` as Zeabur's installer writes it, as YAML; `None` for
    /// manifests.
    charts: Vec<Option<String>>,
}

impl<'a> Rendered<'a> {
    fn new(id: &str, file: &'a Component, profile: Profile) -> Result<Self, String> {
        let level = engine::pod_security(file).ok_or_else(|| "a Pod Security level".to_owned())?;
        let releases = file
            .releases
            .iter()
            .map(Release::new)
            .collect::<Result<Vec<_>, _>>()?;
        let mut namespaces = String::new();
        let mut flux_namespaces = String::new();
        let mut seen = Vec::new();
        for release in &releases {
            if !seen.contains(&release.namespace)
                && let Some(namespace) = engine::namespace(release.namespace, level)
            {
                seen.push(release.namespace);
                namespaces.push_str(&yaml(&namespace)?);
                namespaces.push_str("---\n");
                let mut kept = namespace;
                kept.metadata
                    .annotations
                    .get_or_insert_default()
                    .insert(FLUX_PRUNE.to_owned(), "disabled".to_owned());
                flux_namespaces.push_str(&yaml(&kept)?);
                flux_namespaces.push_str("---\n");
            }
        }
        let first_chart = releases.iter().find(|r| r.is_chart()).map(|r| r.name);
        let mut merged = Vec::new();
        let mut charts = Vec::new();
        for release in &releases {
            let mut values = parse_values(release.values)?;
            if let Some(profile_values) = release.profiles.and_then(|p| p.get(profile)) {
                merge(&mut values, parse_values(Some(profile_values))?);
            }
            for input in &file.inputs {
                let target = input.release.as_deref().or(first_chart);
                if let (Some(path), Some(target)) = (&input.path, target)
                    && target == release.name
                {
                    set_path(&mut values, path, placeholder(input));
                }
            }
            charts.push(Self::chart(id, release, &values)?);
            merged.push(if values.as_object().is_some_and(Map::is_empty) {
                String::new()
            } else {
                serde_saphyr::to_string(&values).map_err(|e| e.to_string())?
            });
        }
        Ok(Self {
            file,
            releases,
            profile,
            merged,
            namespaces,
            flux_namespaces,
            charts,
        })
    }

    /// A chart release's `HelmChart`, as YAML; `None` for manifests.
    fn chart(id: &str, release: &Release<'_>, values: &Value) -> Result<Option<String>, String> {
        let (chart, repo, version) = match release.source {
            Source::Oci { url, version, .. } => (url, None, version),
            Source::Repo {
                repo,
                chart,
                version,
            } => (chart, Some(repo), version),
            Source::Urls(_) | Source::Manifests(_) => return Ok(None),
        };
        let mut object = engine::chart_object(
            &ChartRelease {
                name: release.helm_name,
                chart,
                repo,
                version,
                namespace: release.namespace,
            },
            values,
        )?;
        engine::label_release(&mut object.metadata, id, release.name);
        yaml(&object).map(Some)
    }

    /// The Secrets inputs are stored in, each with its inputs: several inputs may share one
    /// Secret under different keys, so each Secret is written once with every key.
    fn secrets(&self) -> Vec<SecretInputs<'_>> {
        let mut secrets: Vec<SecretInputs<'_>> = Vec::new();
        for input in &self.file.inputs {
            let Some(secret) = &input.secret else {
                continue;
            };
            let id = (secret.namespace.as_str(), secret.name.as_str());
            let entry = (input, secret.key.as_str());
            match secrets.iter_mut().find(|(s, _)| *s == id) {
                Some((_, inputs)) => inputs.push(entry),
                None => secrets.push((id, vec![entry])),
            }
        }
        secrets
    }

    /// `kubectl` commands that make the namespaces of the releases and the secrets, label the
    /// releases' namespaces as the platform's, and give a privileged component its Pod Security
    /// level.
    fn namespace_commands(&self, secrets: &[SecretInputs<'_>]) -> String {
        let mut out = String::new();
        let exempt = self.exempt_namespaces();
        let releases: Vec<&str> = self
            .releases
            .iter()
            .map(|r| r.namespace)
            .filter(|ns| !is_system_namespace(ns))
            .collect();
        let mut namespaces: Vec<&str> = Vec::new();
        let wanted = exempt.iter().map(|(ns, _)| *ns).chain(releases.iter().copied());
        for ns in wanted.chain(secrets.iter().map(|((ns, _), _)| *ns)) {
            if !namespaces.contains(&ns) {
                namespaces.push(ns);
            }
        }
        for ns in namespaces {
            push(
                &mut out,
                format!(
                    "\nkubectl create namespace {ns} --dry-run=client -o yaml | kubectl apply -f -\n"
                ),
            );
            let mut labels = Vec::new();
            if releases.contains(&ns) {
                labels.push(format!("{PLATFORM_LABEL}=true"));
            }
            if let Some((_, level)) = exempt.iter().find(|(name, _)| *name == ns) {
                labels.extend(
                    engine::POD_SECURITY_MODES
                        .iter()
                        .map(|mode| format!("{mode}={level}")),
                );
            }
            if !labels.is_empty() {
                push(
                    &mut out,
                    format!(
                        "kubectl label namespace {ns} --overwrite \\\n  {}\n",
                        labels.join(" \\\n  ")
                    ),
                );
            }
        }
        out
    }

    /// Shell commands for `kubectl` and Helm.
    fn helm(&self) -> String {
        let mut out = self.k3s_packaged();
        push(&mut out, format!("# Profile: {}\n", self.profile.as_str()));
        let secrets = self.secrets();
        out.push_str(&self.namespace_commands(&secrets));
        for ((ns, name), inputs) in &secrets {
            for (input, _) in inputs {
                push(&mut out, format!("# {}\n", input.description.trim()));
            }
            let literals: Vec<String> = inputs
                .iter()
                .map(|(input, key)| {
                    let value = random(input).map_or_else(
                        || format!("'<{}>'", input.name),
                        |random| format!("\"$({random})\""),
                    );
                    format!("--from-literal={key}={value}")
                })
                .collect();
            push(
                &mut out,
                format!(
                    "kubectl -n {ns} create secret generic {name} \\\n  {} \\\n  --dry-run=client -o yaml | kubectl apply -f -\n",
                    literals.join(" \\\n  ")
                ),
            );
        }
        for release in &self.releases {
            out.push('\n');
            push(&mut out, format!("# {}\n", release.name));
            let indent = if release.skip_if_present.is_empty() {
                ""
            } else {
                let checks: Vec<String> = release
                    .skip_if_present
                    .iter()
                    .map(|crd| format!("kubectl get crd {crd} >/dev/null 2>&1"))
                    .collect();
                push(&mut out, format!("if ! {}; then\n", checks.join(" && ")));
                "  "
            };
            match &release.source {
                Source::Urls(urls) => {
                    for url in urls {
                        push(
                            &mut out,
                            format!("{indent}kubectl apply --server-side -f {url}\n"),
                        );
                    }
                }
                Source::Manifests(manifests) => {
                    push(
                        &mut out,
                        format!(
                            "{indent}kubectl create namespace {ns} --dry-run=client -o yaml | kubectl apply -f -\n\
                         {indent}kubectl -n {ns} apply --server-side -f - <<'ZEABUR_MANIFESTS'\n{}ZEABUR_MANIFESTS\n",
                            ensure_newline(manifests),
                            ns = release.namespace,
                        ),
                    );
                }
                Source::Oci { url, version, .. } => {
                    self.helm_chart(&mut out, indent, release, url, None, version);
                }
                Source::Repo {
                    repo,
                    chart,
                    version,
                } => {
                    self.helm_chart(&mut out, indent, release, chart, Some(repo), version);
                }
            }
            if !release.skip_if_present.is_empty() {
                out.push_str("fi\n");
            }
        }
        out
    }

    /// `helm upgrade --install` for one chart release, after its values files.
    fn helm_chart(
        &self,
        out: &mut String,
        indent: &str,
        release: &Release<'_>,
        chart: &str,
        repo: Option<&str>,
        version: &str,
    ) {
        let profile_values = release.profiles.and_then(|p| p.get(self.profile));
        let mut files = Vec::new();
        for (values, file) in [
            (release.values, format!("{}-values.yaml", release.name)),
            (
                profile_values,
                format!("{}-{}.yaml", release.name, self.profile.as_str()),
            ),
        ] {
            if let Some(values) = values.filter(|v| !v.trim().is_empty()) {
                push(
                    out,
                    format!(
                        "{indent}cat > {file} <<'ZEABUR_VALUES'\n{}ZEABUR_VALUES\n",
                        ensure_newline(values)
                    ),
                );
                files.push(file);
            }
        }
        if let Some(lists) = self.input_lists(release) {
            let file = format!("{}-inputs.yaml", release.name);
            push(
                out,
                format!(
                    "{indent}# Helm replaces a list as a whole, so this file repeats each list an input is in.\n\
                     {indent}cat > {file} <<'ZEABUR_VALUES'\n{}ZEABUR_VALUES\n",
                    ensure_newline(&lists)
                ),
            );
            files.push(file);
        }
        let mut args = vec![format!(
            "{indent}helm upgrade --install {} {chart}",
            release.helm_name
        )];
        if let Some(repo) = repo {
            args.push(format!("--repo {repo}"));
        }
        args.push(format!("--version {version}"));
        args.push(format!(
            "--namespace {} --create-namespace",
            release.namespace
        ));
        args.extend(files.iter().map(|file| format!("--values {file}")));
        let first_chart = self.first_chart();
        for input in &self.file.inputs {
            let Some(path) = input.path.as_deref() else {
                continue;
            };
            if input.release.as_deref().unwrap_or(first_chart) != release.name
                || list_of(path).is_some()
            {
                continue;
            }
            let value = random(input).map_or_else(
                || format!("'<{}>'", input.name),
                |random| format!("\"$({random})\""),
            );
            args.push(format!("--set {path}={value}"));
        }
        out.push_str(&args.join(&format!(" \\\n{indent}  ")));
        out.push('\n');
    }

    /// The lists of `release`'s merged values that hold an input, as YAML: `helm --set` with an
    /// index replaces the whole list item, so these inputs go in a values file. `None` when no
    /// input of the release is in a list.
    fn input_lists(&self, release: &Release<'_>) -> Option<String> {
        let first_chart = self.first_chart();
        let index = self
            .releases
            .iter()
            .position(|r| std::ptr::eq(r, release))?;
        let merged = parse_values(self.merged.get(index).map(String::as_str)).ok()?;
        let mut lists = Value::Object(Map::new());
        for input in &self.file.inputs {
            if input.release.as_deref().unwrap_or(first_chart) != release.name {
                continue;
            }
            let Some(list) = input.path.as_deref().and_then(list_of) else {
                continue;
            };
            let pointer = format!("/{}", list.replace('.', "/"));
            if let Some(value) = merged.pointer(&pointer) {
                set_path(&mut lists, &list, value.clone());
            }
        }
        if lists.as_object().is_some_and(Map::is_empty) {
            return None;
        }
        serde_saphyr::to_string(&lists).ok()
    }

    fn first_chart(&self) -> &str {
        self.releases
            .iter()
            .find(|r| r.is_chart())
            .map_or("", |r| r.name)
    }

    /// For each k3s packaged component this one replaces, a comment on disabling it first and
    /// what that removes, at the head of every form; empty for any other component.
    fn k3s_packaged(&self) -> String {
        let detection = &self.file.detection;
        let mut out = String::new();
        for addon in detection.addons_on(Distribution::K3s) {
            push(
                &mut out,
                format!(
                    "# On K3s, first turn off the packaged {addon}: add it to `disable` in\n# /etc/rancher/k3s/config.yaml on every server, then restart k3s.\n"
                ),
            );
        }
        for addon in detection.addons_on(Distribution::Rke2) {
            if addon == "rke2-traefik" {
                out.push_str("# On RKE2, set `ingress-controller: none` in /etc/rancher/rke2/config.yaml on\n# every server, then restart rke2-server.\n");
            } else {
                push(
                    &mut out,
                    format!(
                        "# On RKE2, add {addon} to `disable` in /etc/rancher/rke2/config.yaml on\n# every server, then restart rke2-server.\n"
                    ),
                );
            }
        }
        let names: Vec<&str> = detection.addons.iter().map(|a| a.name.as_str()).collect();
        if names
            .iter()
            .any(|n| ["traefik", "rke2-traefik"].contains(n))
        {
            out.push_str("# That deletes the Traefik CRDs and any IngressRoute or Middleware objects;\n# Ingress traffic stops until this one runs.\n");
        } else if names
            .iter()
            .any(|n| ["metrics-server", "rke2-metrics-server"].contains(n))
        {
            out.push_str("# The metrics API is unavailable until this one runs.\n");
        } else if !out.is_empty() {
            out.push_str("# Its workloads stop until this one runs.\n");
        }
        if !out.is_empty() {
            out.push_str("# https://docs.k3s.io/installation/packaged-components\n");
            out.push_str("# https://docs.rke2.io/advanced\n");
        }
        out
    }

    /// Secret objects for inputs stored in Secrets, for the YAML forms.
    /// The namespaces of its releases with the Pod Security level they need when it is not
    /// `restricted`; in release order.
    fn exempt_namespaces(&self) -> Vec<(&str, &'static str)> {
        let level = match engine::pod_security(self.file) {
            Some(level @ (PodSecurityLevel::Privileged | PodSecurityLevel::Baseline)) => {
                level.as_str()
            }
            Some(PodSecurityLevel::Restricted) | None => return Vec::new(),
        };
        let mut namespaces: Vec<(&str, &'static str)> = Vec::new();
        for release in &self.releases {
            if !namespaces.iter().any(|(ns, _)| *ns == release.namespace) {
                namespaces.push((release.namespace, level));
            }
        }
        namespaces
    }

    fn secret_objects(&self, format: InstallFormat) -> String {
        let mut out = String::new();
        for ((ns, name), inputs) in self.secrets() {
            out.push_str(
                "# Keep these values out of Git (SOPS, Sealed Secrets or a secret manager).\n",
            );
            out.extend(
                inputs
                    .iter()
                    .map(|(input, key)| format!("# {key}: {}\n", input.description.trim())),
            );
            push(
                &mut out,
                format!(
                    "apiVersion: v1\nkind: Secret\nmetadata:\n  name: {name}\n  namespace: {ns}\n"
                ),
            );
            if format == InstallFormat::Flux {
                push(
                    &mut out,
                    format!("  annotations:\n    {FLUX_PRUNE}: disabled\n"),
                );
            }
            out.push_str("stringData:\n");
            out.extend(inputs.iter().map(|(input, key)| {
                let value = random(input).map_or_else(
                    || format!("<{}>", input.name),
                    |random| format!("<random: {random}>"),
                );
                format!("  {key}: \"{value}\"\n")
            }));
            out.push_str("---\n");
        }
        out
    }

    /// Manifest releases, for the YAML forms: listed as they would be applied.
    fn manifest_release(release: &Release<'_>) -> Option<String> {
        match &release.source {
            Source::Urls(urls) => Some(format!(
                "# {}: apply with `kubectl apply --server-side -f`, or vendor into your repository:\n{}",
                release.name,
                urls.iter().fold(String::new(), |mut list, url| {
                    list.push_str("#   ");
                    list.push_str(url);
                    list.push('\n');
                    list
                })
            )),
            Source::Manifests(manifests) => Some(format!(
                "# {} (namespace {})\n{}---\n",
                release.name,
                release.namespace,
                ensure_newline(manifests)
            )),
            Source::Oci { .. } | Source::Repo { .. } => None,
        }
    }

    /// K3s/RKE2 `HelmChart` objects.
    fn k3s(&self) -> String {
        let mut out = self.k3s_packaged();
        push(
            &mut out,
            format!(
                "# Profile: {}. Apply with kubectl, or save in /var/lib/rancher/k3s/server/manifests/\n# (K3s) or /var/lib/rancher/rke2/server/manifests/ (RKE2).\n",
                self.profile.as_str()
            ),
        );
        self.append_releases(&mut out, InstallFormat::K3s);
        trim_separator(out)
    }

    /// Secrets, then each release: a manifest release as listed, a chart as its objects.
    fn append_releases(&self, out: &mut String, format: InstallFormat) {
        out.push_str(if format == InstallFormat::Flux {
            &self.flux_namespaces
        } else {
            &self.namespaces
        });
        out.push_str(&self.secret_objects(format));
        let mut after = None;
        for ((release, merged), chart) in self.releases.iter().zip(&self.merged).zip(&self.charts) {
            if let Some(text) = Self::manifest_release(release) {
                out.push_str(&text);
                continue;
            }
            for object in Self::release_objects(format, release, merged, chart.as_deref(), after) {
                out.push_str(&object);
                out.push_str("---\n");
            }
            after = Some(release.helm_name);
        }
    }

    /// A chart release's objects in one form; empty for manifests and for Helm.
    fn release_objects(
        format: InstallFormat,
        release: &Release<'_>,
        merged: &str,
        chart: Option<&str>,
        after: Option<&str>,
    ) -> Vec<String> {
        match format {
            InstallFormat::Helm => Vec::new(),
            InstallFormat::K3s => chart.map(str::to_owned).into_iter().collect(),
            InstallFormat::Flux => Self::flux_objects(release, merged, after),
            InstallFormat::ArgoCd => Self::argo_cd_objects(release, merged),
        }
    }

    /// The text of one form.
    fn text(&self, format: InstallFormat) -> String {
        match format {
            InstallFormat::Helm => self.helm(),
            InstallFormat::K3s => self.k3s(),
            InstallFormat::Flux => self.flux(),
            InstallFormat::ArgoCd => self.argo_cd(),
        }
    }

    /// Flux sources and `HelmRelease`s.
    fn flux(&self) -> String {
        let mut out = self.k3s_packaged();
        push(
            &mut out,
            format!(
                "# Profile: {}. Commit to the repository Flux reconciles.\n",
                self.profile.as_str()
            ),
        );
        self.append_releases(&mut out, InstallFormat::Flux);
        trim_separator(out)
    }

    /// A chart release's Flux source and `HelmRelease` in `flux-system`, after the chart release
    /// `after` of the same component.
    fn flux_objects(release: &Release<'_>, merged: &str, after: Option<&str>) -> Vec<String> {
        let name = release.helm_name;
        let (source, chart_ref) = match release.source {
            Source::Oci {
                url,
                version,
                digest,
            } => (
                format!(
                    "apiVersion: source.toolkit.fluxcd.io/v1\nkind: OCIRepository\nmetadata:\n  name: {name}\n  namespace: flux-system\nspec:\n  interval: 1h\n  url: {url}\n  ref:\n    tag: {version}\n    digest: {digest}\n  layerSelector:\n    mediaType: application/vnd.cncf.helm.chart.content.v1.tar+gzip\n    operation: copy\n"
                ),
                format!("  chartRef:\n    kind: OCIRepository\n    name: {name}\n"),
            ),
            Source::Repo {
                repo,
                chart,
                version,
            } => (
                format!(
                    "apiVersion: source.toolkit.fluxcd.io/v1\nkind: HelmRepository\nmetadata:\n  name: {name}\n  namespace: flux-system\nspec:\n  interval: 1h\n  url: {repo}\n"
                ),
                format!(
                    "  chart:\n    spec:\n      chart: {chart}\n      version: {version}\n      sourceRef:\n        kind: HelmRepository\n        name: {name}\n"
                ),
            ),
            Source::Urls(_) | Source::Manifests(_) => return Vec::new(),
        };
        let mut helm_release = format!(
            "apiVersion: helm.toolkit.fluxcd.io/v2\nkind: HelmRelease\nmetadata:\n  name: {name}\n  namespace: flux-system\nspec:\n  interval: 1h\n  releaseName: {name}\n  targetNamespace: {}\n  install:\n    createNamespace: true\n{chart_ref}",
            release.namespace
        );
        if let Some(after) = after {
            push(
                &mut helm_release,
                format!("  dependsOn:\n    - name: {after}\n"),
            );
        }
        if !merged.is_empty() {
            helm_release.push_str("  values:\n");
            helm_release.push_str(&indent_block(merged, 4));
        }
        vec![source, helm_release]
    }

    /// Argo CD `Application`s, and a repository Secret per OCI registry path.
    fn argo_cd(&self) -> String {
        let mut out = self.k3s_packaged();
        push(
            &mut out,
            format!(
                "# Profile: {}. Apply in the namespace Argo CD runs in (argocd).\n",
                self.profile.as_str()
            ),
        );
        self.append_releases(&mut out, InstallFormat::ArgoCd);
        trim_separator(out)
    }

    /// A chart release's Argo CD `Application` in `argocd`, after the repository Secret an OCI
    /// chart needs.
    fn argo_cd_objects(release: &Release<'_>, merged: &str) -> Vec<String> {
        let name = release.helm_name;
        let mut objects = Vec::new();
        let (repo_url, chart, version) = match release.source {
            Source::Oci { url, version, .. } => {
                let path = url.trim_start_matches("oci://");
                let (registry_path, chart) = path.rsplit_once('/').unwrap_or((path, path));
                objects.push(format!(
                    "# Argo CD reads OCI Helm charts through a repository with enableOCI.\napiVersion: v1\nkind: Secret\nmetadata:\n  name: {name}-oci\n  namespace: argocd\n  labels:\n    argocd.argoproj.io/secret-type: repository\nstringData:\n  type: helm\n  name: {name}-oci\n  url: {registry_path}\n  enableOCI: \"true\"\n"
                ));
                (registry_path.to_owned(), chart.to_owned(), version)
            }
            Source::Repo {
                repo,
                chart,
                version,
            } => (repo.to_owned(), chart.to_owned(), version),
            Source::Urls(_) | Source::Manifests(_) => return Vec::new(),
        };
        let mut application = format!(
            "apiVersion: argoproj.io/v1alpha1\nkind: Application\nmetadata:\n  name: {name}\n  namespace: argocd\nspec:\n  project: default\n  source:\n    repoURL: {repo_url}\n    chart: {chart}\n    targetRevision: {version}\n    helm:\n      releaseName: {name}\n"
        );
        if !merged.is_empty() {
            application.push_str("      valuesObject:\n");
            application.push_str(&indent_block(merged, 8));
        }
        push(
            &mut application,
            format!(
                "  destination:\n    server: https://kubernetes.default.svc\n    namespace: {}\n  syncPolicy:\n    syncOptions:\n      - CreateNamespace=true\n      - ServerSideApply=true\n",
                release.namespace
            ),
        );
        objects.push(application);
        objects
    }
}

fn placeholder(input: &Input) -> Value {
    random(input).map_or_else(
        || {
            input.default.as_ref().map_or_else(
                || Value::String(format!("<{}>", input.name)),
                InputValue::to_json,
            )
        },
        |random| Value::String(format!("<random: {random}>")),
    )
}

pub(crate) fn parse_values(text: Option<&str>) -> Result<Value, String> {
    match text.map(str::trim).filter(|t| !t.is_empty()) {
        None => Ok(Value::Object(Map::new())),
        Some(text) => {
            let value: Value = serde_saphyr::from_str(text).map_err(|e| e.to_string())?;
            Ok(if value.is_null() {
                Value::Object(Map::new())
            } else {
                value
            })
        }
    }
}

/// Merges `over` into `base` as Helm merges values files: maps merge key by key, anything else
/// (lists included) replaces.
pub(crate) fn merge(base: &mut Value, over: Value) {
    match (base, over) {
        (Value::Object(base), Value::Object(over)) => {
            for (key, value) in over {
                match base.get_mut(&key) {
                    Some(existing) => merge(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, over) => *base = over,
    }
}

/// The path of the list a dotted path indexes into (`a.b` for `a.b.0.c`); `None` when it has no
/// index.
fn list_of(path: &str) -> Option<String> {
    let segments: Vec<&str> = path.split('.').collect();
    let at = segments.iter().position(|s| s.parse::<usize>().is_ok())?;
    Some(segments.get(..at)?.join("."))
}

/// Sets `value` at a dotted path; numeric segments index lists, which grow as needed.
pub(crate) fn set_path(root: &mut Value, path: &str, value: Value) {
    let mut node = root;
    let segments: Vec<&str> = path.split('.').collect();
    for (i, segment) in segments.iter().enumerate() {
        let last = i == segments.len() - 1;
        if let Ok(index) = segment.parse::<usize>() {
            if !node.is_array() {
                *node = Value::Array(Vec::new());
            }
            let Value::Array(items) = node else { return };
            while items.len() <= index {
                items.push(Value::Null);
            }
            let Some(slot) = items.get_mut(index) else {
                return;
            };
            if last {
                *slot = value;
                return;
            }
            node = slot;
        } else {
            if !node.is_object() {
                *node = Value::Object(Map::new());
            }
            let Value::Object(map) = node else { return };
            if last {
                map.insert((*segment).to_owned(), value);
                return;
            }
            node = map
                .entry((*segment).to_owned())
                .or_insert_with(|| Value::Object(Map::new()));
        }
    }
}

/// Appends a formatted piece of text.
/// `object` as a YAML document, with its `apiVersion` and `kind`.
fn yaml<K: Resource + Serialize>(object: &K) -> Result<String, String> {
    serde_saphyr::to_string(&engine::dynamic(object)?).map_err(|e| e.to_string())
}

fn push(out: &mut String, text: impl AsRef<str>) {
    out.push_str(text.as_ref());
}

fn ensure_newline(text: &str) -> String {
    if text.ends_with('\n') {
        text.to_owned()
    } else {
        format!("{text}\n")
    }
}

fn indent_block(text: &str, depth: usize) -> String {
    let pad = " ".repeat(depth);
    text.lines()
        .map(|line| {
            if line.is_empty() {
                "\n".to_owned()
            } else {
                format!("{pad}{line}\n")
            }
        })
        .collect()
}

fn trim_separator(mut out: String) -> String {
    if out.ends_with("---\n") {
        out.truncate(out.len() - 4);
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The values written for a chart read back the same, whatever the strings look like.
    #[test]
    fn written_values_read_back_the_same() {
        let strings = [
            "yes",
            "on",
            "no",
            "off",
            "0123",
            "1e3",
            "1:20",
            ":",
            "#",
            "a #b",
            "a: b",
            "true",
            "null",
            "~",
            "",
            "- x",
            "2024-01-01",
            " lead",
            "trail ",
            "multi\nline\n",
            "\"q\"",
            "'q'",
        ];
        let values = json!({
            "list": strings,
            "map": strings.iter().map(|s| ((*s).to_owned(), json!(s))).collect::<Map<_, _>>(),
            "number": 123, "float": 1.5, "flag": true, "none": null,
        });
        let yaml = serde_saphyr::to_string(&values).unwrap();
        let back: Value = serde_saphyr::from_str(&yaml).unwrap();
        assert_eq!(back, values, "{yaml}");
    }

    #[test]
    fn values_merge_like_helm_maps_merge_and_lists_replace() {
        let mut base = json!({"a": {"b": 1, "c": [1, 2]}, "d": "x"});
        merge(&mut base, json!({"a": {"c": [3], "e": true}}));
        assert_eq!(base, json!({"a": {"b": 1, "c": [3], "e": true}, "d": "x"}));
    }

    #[test]
    fn a_dotted_path_with_an_index_builds_maps_and_lists() {
        let mut root = json!({});
        set_path(&mut root, "provider.name", json!("<provider>"));
        set_path(&mut root, "extraArgs.1", json!("x"));
        assert_eq!(
            root,
            json!({"provider": {"name": "<provider>"}, "extraArgs": [null, "x"]})
        );
    }

    #[test]
    fn every_component_renders_in_every_form_and_profile_as_valid_yaml() {
        for (component, _) in bundle::FILES {
            for profile in PROFILES {
                let guide = guide(component, profile, "zeabur")
                    .unwrap_or_else(|e| panic!("{component} {profile}: {e}"));
                for text in &guide.formats {
                    assert!(
                        !text.text.trim().is_empty(),
                        "{component} {:?}",
                        text.format
                    );
                    if text.format != InstallFormat::Helm {
                        for document in text.text.split("\n---\n") {
                            let parsed: Result<Value, _> = serde_saphyr::from_str(document);
                            assert!(
                                parsed.is_ok(),
                                "{component} {profile} {:?} does not parse: {:?}\n{document}",
                                text.format,
                                parsed.err()
                            );
                        }
                    }
                }
            }
        }
    }

    /// The `HelmChart` and Namespace objects of the k3s form are the ones Zeabur's installer
    /// writes, without the annotations of the catalog release it installs from.
    #[test]
    fn the_k3s_form_holds_the_objects_the_installer_writes() {
        let key = |o: &Value| format!("{}/{}", o["kind"], o["metadata"]["name"]);
        for (id, text) in bundle::FILES {
            let declaration: Component = serde_saphyr::from_str(text).unwrap();
            if !declaration.inputs.is_empty() {
                continue;
            }
            let component = crate::api::ComponentFile {
                id: (*id).to_owned(),
                source_digest: String::new(),
                declaration,
                render: zeabur_catalog_format::api::Render {
                    kubernetes_version: String::new(),
                    profiles: std::collections::BTreeMap::new(),
                },
                measurements: Vec::new(),
                readmes: Vec::new(),
            };
            // The files of manifest releases hold neither kind.
            let manifests: engine::Manifests = engine::downloads(&component)
                .unwrap()
                .into_iter()
                .map(|d| (d.url, "".into()))
                .collect();
            for profile in PROFILES {
                let mut written: Vec<Value> = engine::steps_of(
                    &component,
                    "",
                    profile,
                    &manifests,
                    &[],
                    &engine::Contributions::default(),
                    "zeabur",
                )
                .unwrap()
                .into_iter()
                .flat_map(|step| step.objects)
                .map(|desired| {
                    let mut object = desired.object;
                    object.metadata.annotations = None;
                    serde_json::to_value(object).unwrap()
                })
                .filter(|o| o["kind"] == "HelmChart" || o["kind"] == "Namespace")
                .collect();
                let k3s = guide(id, profile, "zeabur")
                    .unwrap()
                    .formats
                    .into_iter()
                    .find(|f| f.format == InstallFormat::K3s)
                    .unwrap()
                    .text;
                let mut exported: Vec<Value> = serde_saphyr::from_multiple::<Value>(&k3s)
                    .unwrap()
                    .into_iter()
                    .filter(|o| o["kind"] == "HelmChart" || o["kind"] == "Namespace")
                    .collect();
                written.sort_by_key(key);
                exported.sort_by_key(key);
                assert_eq!(exported, written, "{id} {profile}");
            }
        }
    }

    #[test]
    fn a_privileged_component_gets_a_privileged_namespace() {
        let (_, text) = bundle::FILES
            .iter()
            .find(|(id, _)| *id == "cloudnative-pg")
            .unwrap();
        let mut file: Component = serde_saphyr::from_str(text).unwrap();
        file.security.pod_security = zeabur_catalog_format::common::PodSecurity::Privileged;
        let rendered = Rendered::new("x", &file, Profile::SingleNode).unwrap();
        let k3s = rendered.k3s();
        let namespace = k3s.find("kind: Namespace").unwrap();
        assert!(namespace < k3s.find("kind: HelmChart").unwrap());
        for (id, _) in bundle::FILES {
            let guide = guide(id, "multi-node", "zeabur").unwrap();
            if guide.security.pod_security != "privileged" {
                continue;
            }
            for text in &guide.formats {
                let level = if text.format == InstallFormat::Helm {
                    "pod-security.kubernetes.io/enforce=privileged"
                } else {
                    "pod-security.kubernetes.io/enforce: privileged"
                };
                assert!(text.text.contains(level), "{id} {:?}", text.format);
            }
        }
    }

    #[test]
    fn a_baseline_component_gets_a_baseline_namespace_in_every_form() {
        let guide = guide("k8up", "single-node", "zeabur").unwrap();
        assert_eq!(guide.security.pod_security, "baseline");
        for text in &guide.formats {
            let label = if text.format == InstallFormat::Helm {
                "pod-security.kubernetes.io/enforce=baseline"
            } else {
                "pod-security.kubernetes.io/enforce: baseline"
            };
            assert!(text.text.contains(label), "{:?}", text.format);
        }
    }

    /// The Flux form's objects of `component`, parsed.
    fn flux_documents(component: &str) -> Vec<Value> {
        let text = guide(component, "single-node", "zeabur")
            .unwrap()
            .formats
            .into_iter()
            .find(|t| t.format == InstallFormat::Flux)
            .unwrap()
            .text;
        serde_saphyr::from_multiple::<Value>(&text)
            .unwrap()
            .into_iter()
            .filter(|document| document.get("kind").is_some())
            .collect()
    }

    #[test]
    fn a_flux_helm_release_waits_for_the_chart_release_before_it() {
        let releases: Vec<(String, Option<Value>)> = flux_documents("cloudnative-pg")
            .into_iter()
            .filter(|d| d["kind"] == "HelmRelease")
            .map(|d| {
                (
                    d["metadata"]["name"].as_str().unwrap().to_owned(),
                    d["spec"].get("dependsOn").cloned(),
                )
            })
            .collect();
        assert_eq!(
            releases,
            [
                ("cnpg".to_owned(), None),
                (
                    "plugin-barman-cloud".to_owned(),
                    Some(json!([{"name": "cnpg"}]))
                ),
            ]
        );
    }

    #[test]
    fn flux_keeps_the_namespaces_and_input_secrets_when_it_prunes() {
        let kept: Vec<(String, bool)> = flux_documents("crowdsec")
            .into_iter()
            .filter(|d| d["kind"] == "Namespace" || d["kind"] == "Secret")
            .map(|d| {
                (
                    d["kind"].as_str().unwrap().to_owned(),
                    d["metadata"]["annotations"][FLUX_PRUNE] == "disabled",
                )
            })
            .collect();
        assert!(kept.iter().any(|(kind, _)| kind == "Namespace"));
        assert!(kept.iter().any(|(kind, _)| kind == "Secret"));
        assert!(kept.iter().all(|(_, pruned_never)| *pruned_never));
    }

    #[test]
    fn a_flux_oci_source_pins_the_catalogs_chart_digest() {
        let source = flux_documents("crowdsec")
            .into_iter()
            .find(|d| d["kind"] == "OCIRepository")
            .unwrap();
        let (_, text) = bundle::FILES
            .iter()
            .find(|(id, _)| *id == "crowdsec")
            .unwrap();
        let file: Component = serde_saphyr::from_str(text).unwrap();
        let chart = file
            .releases
            .iter()
            .find_map(component::Release::helm)
            .unwrap();
        assert_eq!(
            source["spec"]["ref"]["digest"].as_str(),
            Some(chart.chart_digest.as_str())
        );
    }

    #[test]
    fn an_install_is_found_in_every_form_by_its_chart_label() {
        // Argo CD leaves no Helm release Secret, and a release named otherwise than its chart
        // (`cnpg`) hides the Secret k3s or Flux leaves: the chart label is what every form leaves.
        let components = crate::catalogue().unwrap();
        for (id, text) in bundle::FILES {
            let file: Component = serde_saphyr::from_str(text).unwrap();
            let Some(charts) = file
                .releases
                .iter()
                .map(component::Release::helm)
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let mut facts = zeabur_kube::ClusterFacts::default();
            for release in charts {
                let chart = release.chart.rsplit('/').next().unwrap_or(&release.chart);
                let version = &release.version;
                facts.workload_labels.push((
                    "Deployment".to_owned(),
                    std::collections::BTreeMap::from([(
                        "helm.sh/chart".to_owned(),
                        format!("{chart}-{version}"),
                    )]),
                ));
            }
            let found = crate::detect(&components, &facts);
            assert!(
                found
                    .iter()
                    .any(|c| c.installations.iter().any(|i| i.component == *id)),
                "{id}"
            );
        }
    }

    #[test]
    fn components_k3s_also_packages_say_to_disable_its_copy_first() {
        for (component, _) in bundle::FILES {
            let guide = guide(component, "single-node", "zeabur").unwrap();
            let expected: &[&str] = match *component {
                "traefik" => &["traefik"],
                "metrics-server" => &["metrics-server"],
                _ => &[],
            };
            assert_eq!(guide.replaces_k3s_packaged, expected, "{component}");
            for text in &guide.formats {
                assert_eq!(
                    text.text
                        .starts_with("# On K3s, first turn off the packaged "),
                    !expected.is_empty(),
                    "{component} {:?}",
                    text.format
                );
            }
        }
    }

    /// Representative cases: an OCI chart with a profile (cert-manager), an HTTP repository with
    /// asked inputs (velero), generated inputs stored in Secrets (crowdsec), a manifest
    /// release before a chart (traefik), and the k3s packaged notes (traefik, metrics-server).
    const GOLDEN: [(&str, &str); 7] = [
        ("cert-manager", "single-node"),
        ("cert-manager", "multi-node"),
        ("velero", "single-node"),
        ("crowdsec", "single-node"),
        ("traefik", "multi-node"),
        ("metrics-server", "single-node"),
        ("victoria-logs", "single-node"),
    ];

    #[test]
    fn install_texts_match_their_golden_files() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
        let update = std::env::var_os("ZEABUR_UPDATE_GOLDEN").is_some();
        let mut stale = Vec::new();
        for (component, profile) in GOLDEN {
            for text in guide(component, profile, "zeabur").unwrap().formats {
                let format = match text.format {
                    InstallFormat::Helm => "helm.sh",
                    InstallFormat::K3s => "k3s.yaml",
                    InstallFormat::Flux => "flux.yaml",
                    InstallFormat::ArgoCd => "argocd.yaml",
                };
                let path = dir.join(format!("{component}-{profile}-{format}"));
                if update {
                    std::fs::create_dir_all(&dir).unwrap();
                    std::fs::write(&path, &text.text).unwrap();
                } else if std::fs::read_to_string(&path).ok().as_deref() != Some(&text.text) {
                    stale.push(path.display().to_string());
                }
            }
        }
        assert!(
            stale.is_empty(),
            "stale golden files (regenerate with ZEABUR_UPDATE_GOLDEN=1): {stale:?}"
        );
    }
}
