//! How to install a catalogue component yourself, in four forms: Helm commands, a k3s
//! `HelmChart`, Flux objects and an Argo CD `Application`.
//!
//! Zeabur recommends and never installs: these are texts to copy and run, or to commit to a Git
//! repository a GitOps tool watches. Each renders the component's releases in order, with the
//! catalogue's pinned versions and its values for the chosen profile (`single-node` or
//! `multi-node`): the release's values, with the profile's merged over them as Helm merges values
//! files, maps merged and lists replaced [helm-values].
//!
//! - **Helm** keeps the catalogue's two values files apart, comments and all, and passes both
//!   (`--values base --values profile`), so the reasons for each value stay readable. `helm upgrade
//!   --install` installs or upgrades in one command [helm-upgrade]; a chart from an HTTP repository
//!   is named with `--repo`.
//! - **k3s** writes a `HelmChart` for k3s's Helm controller, which installs it; the file can go in
//!   the server's auto-deploying manifests directory [k3s-helm]. Values are merged into
//!   `valuesContent`.
//! - **Flux** writes the chart's source (`OCIRepository` for an OCI chart, `HelmRepository` for an
//!   HTTP repository) and a `HelmRelease` [flux-helmrelease] [flux-ocirepository].
//! - **Argo CD** writes an `Application` with the values in `helm.valuesObject`; an OCI chart also
//!   needs a repository Secret with `enableOCI` [argocd-helm].
//!
//! A component k3s also packages (Traefik, metrics-server) cannot run beside k3s's copy: two
//! Traefiks compete for ports 80 and 443, two metrics-servers for the `v1beta1.metrics.k8s.io`
//! `APIService`, and the k3s form's `HelmChart kube-system/<name>` would overwrite k3s's own, which
//! k3s re-applies on its next restart. So every form, on k3s, begins with how to disable the
//! packaged one first (`disable` in `/etc/rancher/k3s/config.yaml` on every server, then a
//! restart), and what that removes [k3s-packaged]; the guide lists those add-ons for the interface
//! to warn about.
//!
//! Releases that are manifests (by URL or inline) are applied with `kubectl apply
//! --server-side` in the Helm form, and listed in the others for the person's own repository.
//! Inputs the catalogue asks for appear as `<name>` placeholders; generated ones are produced by
//! `openssl rand -hex 64` in the Helm form and marked in the others. Secrets an input is stored
//! in are created before the releases.
//!
//! [helm-values]: https://helm.sh/docs/chart_template_guide/values_files/
//! [helm-upgrade]: https://helm.sh/docs/helm/helm_upgrade/
//! [k3s-helm]: https://docs.k3s.io/helm
//! [flux-helmrelease]: https://fluxcd.io/flux/components/helm/helmreleases/
//! [flux-ocirepository]: https://fluxcd.io/flux/components/source/ocirepositories/
//! [argocd-helm]: https://argo-cd.readthedocs.io/en/stable/user-guide/helm/
//! [k3s-packaged]: https://docs.k3s.io/installation/packaged-components

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;

use crate::bundle;

/// The profiles the catalogue sizes components for.
pub const PROFILES: [&str; 2] = ["single-node", "multi-node"];

/// One way to install a component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum InstallFormat {
    /// Shell commands: `kubectl` and `helm upgrade --install`.
    Helm,
    /// A k3s `HelmChart` object.
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

/// What a component's RBAC and network access amount to, from the catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Security {
    /// What its RBAC lets it do.
    pub summary: String,
    /// It has cluster-scoped RBAC.
    #[serde(default)]
    pub cluster_wide: bool,
    /// It runs privileged.
    #[serde(default)]
    pub privileged: bool,
    /// Hosts it connects to.
    #[serde(default)]
    pub egress: Vec<String>,
    /// External credentials: `optional` or `required`; absent means none.
    #[serde(default)]
    pub credentials: Option<String>,
    /// What the credentials are for.
    #[serde(default)]
    pub credentials_note: Option<String>,
    /// Who may use its custom resources: `namespaced` or `admin-only`; absent means tenants never
    /// do.
    #[serde(default)]
    pub tenants: Option<String>,
    /// Known risks and how to contain them.
    #[serde(default)]
    pub risks: Vec<Risk>,
}

/// A known risk of a component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Risk {
    /// The risk.
    pub risk: String,
    /// How to contain it.
    pub mitigation: String,
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
    /// `<capability>/<id>`.
    pub component: String,
    /// The profile the texts are for.
    pub profile: String,
    /// Every profile the catalogue sizes it for.
    pub profiles: Vec<String>,
    /// What it needs installed first: `<capability>` (any provider) or `<capability>/<id>`.
    pub depends_on: Vec<String>,
    /// Its RBAC and network access.
    pub security: Security,
    /// Values asked for or generated.
    pub inputs: Vec<InstallInput>,
    /// Other components whose releases it adds values to (`<capability>/<id>`); those values
    /// are not in these texts.
    pub contributes_to: Vec<String>,
    /// k3s packaged components of the same kind (`traefik`, `metrics-server`) that must be
    /// disabled on k3s before installing this one; each text begins by saying how.
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
    /// No such profile.
    #[error("unknown profile {0}; use single-node or multi-node")]
    UnknownProfile(String),
    /// The catalogue file or its values do not parse; a test renders them all.
    #[error("Zeabur Catalog, {0}: {1}")]
    Catalogue(String, String),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct File {
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    inputs: Vec<Input>,
    releases: Vec<Release>,
    #[serde(default)]
    contributes: Vec<Contribution>,
    security: Security,
    #[serde(default)]
    detection: Detection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Detection {
    #[serde(default)]
    k3s_addons: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Contribution {
    target: String,
}

#[derive(Debug, Deserialize)]
struct Input {
    name: String,
    description: String,
    #[serde(default)]
    release: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    secret: Option<SecretRef>,
    #[serde(default)]
    generate: Option<String>,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    default: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct SecretRef {
    name: String,
    key: String,
    namespace: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Release {
    name: String,
    namespace: String,
    #[serde(default, rename = "releaseName")]
    helm_name: Option<String>,
    #[serde(default)]
    skip_if_present: Vec<String>,
    #[serde(default)]
    chart: Option<String>,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    urls: Vec<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    manifests: Option<String>,
    #[serde(default)]
    values: Option<String>,
    #[serde(default)]
    profiles: Map<String, Value>,
}

/// What one release installs.
enum Source<'a> {
    /// A chart from an OCI registry: `oci://host/path/name`.
    Oci { url: &'a str, version: &'a str },
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

impl Release {
    #[expect(
        clippy::literal_string_with_formatting_args,
        reason = "`{version}` is the catalogue's URL placeholder, not a format argument"
    )]
    fn source(&self) -> Option<Source<'_>> {
        let version = self.version.as_deref();
        match (&self.chart, &self.repo, &self.manifests) {
            (Some(chart), None, _) if chart.starts_with("oci://") => Some(Source::Oci {
                url: chart,
                version: version?,
            }),
            (Some(chart), Some(repo), _) => Some(Source::Repo {
                repo,
                chart,
                version: version?,
            }),
            (None, _, Some(manifests)) => Some(Source::Manifests(manifests)),
            (None, _, None) if !self.urls.is_empty() => Some(Source::Urls(
                self.urls
                    .iter()
                    .map(|u| u.replace("{version}", version.unwrap_or_default()))
                    .collect(),
            )),
            _ => None,
        }
    }

    fn release_name(&self) -> &str {
        self.helm_name.as_deref().unwrap_or(&self.name)
    }

    const fn is_chart(&self) -> bool {
        self.chart.is_some()
    }
}

/// How to install `component` (`<capability>/<id>`) for `profile`.
///
/// # Errors
///
/// An unknown component or profile, or a catalogue file that does not parse.
pub fn guide(component: &str, profile: &str) -> Result<InstallGuide, InstallError> {
    if !PROFILES.contains(&profile) {
        return Err(InstallError::UnknownProfile(profile.to_owned()));
    }
    let (capability, id) = component
        .split_once('/')
        .ok_or_else(|| InstallError::UnknownComponent(component.to_owned()))?;
    let text = bundle::FILES
        .iter()
        .find(|(c, i, _)| *c == capability && *i == id)
        .map(|(_, _, text)| *text)
        .ok_or_else(|| InstallError::UnknownComponent(component.to_owned()))?;
    let catalogue = |message: String| InstallError::Catalogue(component.to_owned(), message);
    let file: File = serde_saphyr::from_str(text).map_err(|e| catalogue(e.to_string()))?;
    let rendered = Rendered::new(&file, profile).map_err(catalogue)?;
    Ok(InstallGuide {
        component: component.to_owned(),
        profile: profile.to_owned(),
        profiles: PROFILES.iter().map(|p| (*p).to_owned()).collect(),
        depends_on: file.depends_on.clone(),
        security: file.security.clone(),
        inputs: file
            .inputs
            .iter()
            .map(|i| InstallInput {
                name: i.name.clone(),
                description: i.description.trim().to_owned(),
                required: i.required,
                generated: i.generate.is_some(),
            })
            .collect(),
        contributes_to: file.contributes.iter().map(|c| c.target.clone()).collect(),
        replaces_k3s_packaged: file.detection.k3s_addons.clone(),
        formats: vec![
            InstallText {
                format: InstallFormat::Helm,
                text: rendered.helm(),
            },
            InstallText {
                format: InstallFormat::K3s,
                text: rendered.k3s(),
            },
            InstallText {
                format: InstallFormat::Flux,
                text: rendered.flux(),
            },
            InstallText {
                format: InstallFormat::ArgoCd,
                text: rendered.argo_cd(),
            },
        ],
    })
}

/// A Secret inputs are stored in: `(namespace, name)`, and each input with its key.
type SecretInputs<'a> = ((&'a str, &'a str), Vec<(&'a Input, &'a str)>);

/// The releases of a file, with their values worked out for one profile.
struct Rendered<'a> {
    file: &'a File,
    profile: &'a str,
    /// Per release: the merged values (inputs set at their paths), as YAML; empty when none.
    merged: Vec<String>,
}

impl<'a> Rendered<'a> {
    fn new(file: &'a File, profile: &'a str) -> Result<Self, String> {
        let first_chart = file
            .releases
            .iter()
            .find(|r| r.is_chart())
            .map(|r| r.name.as_str());
        let mut merged = Vec::new();
        for release in &file.releases {
            let mut values = parse_values(release.values.as_deref())?;
            if let Some(profile_values) = release.profiles.get(profile).and_then(Value::as_str) {
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
            merged.push(if values.as_object().is_some_and(Map::is_empty) {
                String::new()
            } else {
                serde_saphyr::to_string(&values).map_err(|e| e.to_string())?
            });
        }
        Ok(Self {
            file,
            profile,
            merged,
        })
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

    /// Shell commands for `kubectl` and Helm.
    fn helm(&self) -> String {
        let mut out = self.k3s_packaged();
        push(&mut out, format!("# Profile: {}\n", self.profile));
        let secrets = self.secrets();
        let mut namespaces: Vec<&str> = secrets.iter().map(|((ns, _), _)| *ns).collect();
        namespaces.dedup();
        for ns in namespaces {
            push(
                &mut out,
                format!(
                    "\nkubectl create namespace {ns} --dry-run=client -o yaml | kubectl apply -f -\n"
                ),
            );
        }
        for ((ns, name), inputs) in &secrets {
            for (input, _) in inputs {
                push(&mut out, format!("# {}\n", input.description.trim()));
            }
            let literals: Vec<String> = inputs
                .iter()
                .map(|(input, key)| {
                    let value = if input.generate.is_some() {
                        "\"$(openssl rand -hex 64)\"".to_owned()
                    } else {
                        format!("'<{}>'", input.name)
                    };
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
        for release in &self.file.releases {
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
            match release.source() {
                Some(Source::Urls(urls)) => {
                    for url in urls {
                        push(
                            &mut out,
                            format!("{indent}kubectl apply --server-side -f {url}\n"),
                        );
                    }
                }
                Some(Source::Manifests(manifests)) => {
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
                Some(Source::Oci { url, version }) => {
                    self.helm_chart(&mut out, indent, release, url, None, version);
                }
                Some(Source::Repo {
                    repo,
                    chart,
                    version,
                }) => {
                    self.helm_chart(&mut out, indent, release, chart, Some(repo), version);
                }
                None => {}
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
        release: &Release,
        chart: &str,
        repo: Option<&str>,
        version: &str,
    ) {
        let profile_values = release.profiles.get(self.profile).and_then(Value::as_str);
        let mut files = Vec::new();
        for (values, file) in [
            (
                release.values.as_deref(),
                format!("{}-values.yaml", release.name),
            ),
            (
                profile_values,
                format!("{}-{}.yaml", release.name, self.profile),
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
        let mut args = vec![format!(
            "{indent}helm upgrade --install {} {chart}",
            release.release_name()
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
            if input.release.as_deref().unwrap_or(first_chart) != release.name {
                continue;
            }
            let value = if input.generate.is_some() {
                "\"$(openssl rand -hex 64)\"".to_owned()
            } else {
                format!("'<{}>'", input.name)
            };
            args.push(format!("--set {path}={value}"));
        }
        out.push_str(&args.join(&format!(" \\\n{indent}  ")));
        out.push('\n');
    }

    fn first_chart(&self) -> &str {
        self.file
            .releases
            .iter()
            .find(|r| r.is_chart())
            .map_or("", |r| r.name.as_str())
    }

    /// For each k3s packaged component this one replaces, a comment on disabling it first and
    /// what that removes, at the head of every form; empty for any other component.
    fn k3s_packaged(&self) -> String {
        let mut out = String::new();
        for addon in &self.file.detection.k3s_addons {
            push(
                &mut out,
                format!(
                    "# On k3s, first disable the packaged {addon}: add it to `disable` in\n# /etc/rancher/k3s/config.yaml on every server, then restart k3s.\n"
                ),
            );
            out.push_str(match addon.as_str() {
                "traefik" => {
                    "# That deletes the Traefik CRDs and any IngressRoute or Middleware objects;\n# Ingress traffic stops until this one runs.\n"
                }
                "metrics-server" => "# The metrics API is unavailable until this one runs.\n",
                _ => "# Its workloads stop until this one runs.\n",
            });
        }
        if !out.is_empty() {
            out.push_str("# https://docs.k3s.io/installation/packaged-components\n");
        }
        out
    }

    /// Secret objects for inputs stored in Secrets, for the YAML forms.
    fn secret_objects(&self) -> String {
        let mut out = String::new();
        for ((ns, name), inputs) in self.secrets() {
            out.push_str(
                "# Keep these values out of Git (SOPS, Sealed Secrets or a secret manager).\n",
            );
            for (input, key) in &inputs {
                push(&mut out, format!("# {key}: {}\n", input.description.trim()));
            }
            push(
                &mut out,
                format!(
                    "apiVersion: v1\nkind: Secret\nmetadata:\n  name: {name}\n  namespace: {ns}\nstringData:\n"
                ),
            );
            for (input, key) in &inputs {
                let value = if input.generate.is_some() {
                    "<random: openssl rand -hex 64>".to_owned()
                } else {
                    format!("<{}>", input.name)
                };
                push(&mut out, format!("  {key}: \"{value}\"\n"));
            }
            out.push_str("---\n");
        }
        out
    }

    /// Manifest releases, for the YAML forms: listed as they would be applied.
    fn manifest_release(release: &Release) -> Option<String> {
        match release.source()? {
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

    /// k3s `HelmChart` objects.
    fn k3s(&self) -> String {
        let mut out = self.k3s_packaged();
        push(
            &mut out,
            format!(
                "# Profile: {}. Apply with kubectl, or save in /var/lib/rancher/k3s/server/manifests/.\n",
                self.profile
            ),
        );
        out.push_str(&self.secret_objects());
        for (release, merged) in self.file.releases.iter().zip(&self.merged) {
            if let Some(text) = Self::manifest_release(release) {
                out.push_str(&text);
                continue;
            }
            let (chart, repo, version) = match release.source() {
                Some(Source::Oci { url, version }) => (url, None, version),
                Some(Source::Repo {
                    repo,
                    chart,
                    version,
                }) => (chart, Some(repo), version),
                _ => continue,
            };
            push(
                &mut out,
                format!(
                    "apiVersion: helm.cattle.io/v1\nkind: HelmChart\nmetadata:\n  name: {}\n  namespace: kube-system\nspec:\n  chart: {chart}\n",
                    release.release_name()
                ),
            );
            if let Some(repo) = repo {
                push(&mut out, format!("  repo: {repo}\n"));
            }
            push(
                &mut out,
                format!(
                    "  version: {version}\n  targetNamespace: {}\n  createNamespace: true\n",
                    release.namespace
                ),
            );
            if !merged.is_empty() {
                out.push_str("  valuesContent: |-\n");
                out.push_str(&indent_block(merged, 4));
            }
            out.push_str("---\n");
        }
        trim_separator(out)
    }

    /// Flux sources and `HelmRelease`s.
    fn flux(&self) -> String {
        let mut out = self.k3s_packaged();
        push(
            &mut out,
            format!(
                "# Profile: {}. Commit to the repository Flux reconciles.\n",
                self.profile
            ),
        );
        out.push_str(&self.secret_objects());
        for (release, merged) in self.file.releases.iter().zip(&self.merged) {
            if let Some(text) = Self::manifest_release(release) {
                out.push_str(&text);
                continue;
            }
            let name = release.release_name();
            let chart_ref = match release.source() {
                Some(Source::Oci { url, version }) => {
                    push(
                        &mut out,
                        format!(
                            "apiVersion: source.toolkit.fluxcd.io/v1\nkind: OCIRepository\nmetadata:\n  name: {name}\n  namespace: flux-system\nspec:\n  interval: 1h\n  url: {url}\n  ref:\n    tag: {version}\n  layerSelector:\n    mediaType: application/vnd.cncf.helm.chart.content.v1.tar+gzip\n    operation: copy\n---\n"
                        ),
                    );
                    format!("  chartRef:\n    kind: OCIRepository\n    name: {name}\n")
                }
                Some(Source::Repo {
                    repo,
                    chart,
                    version,
                }) => {
                    push(
                        &mut out,
                        format!(
                            "apiVersion: source.toolkit.fluxcd.io/v1\nkind: HelmRepository\nmetadata:\n  name: {name}\n  namespace: flux-system\nspec:\n  interval: 1h\n  url: {repo}\n---\n"
                        ),
                    );
                    format!(
                        "  chart:\n    spec:\n      chart: {chart}\n      version: {version}\n      sourceRef:\n        kind: HelmRepository\n        name: {name}\n"
                    )
                }
                _ => continue,
            };
            push(
                &mut out,
                format!(
                    "apiVersion: helm.toolkit.fluxcd.io/v2\nkind: HelmRelease\nmetadata:\n  name: {name}\n  namespace: flux-system\nspec:\n  interval: 1h\n  releaseName: {name}\n  targetNamespace: {}\n  install:\n    createNamespace: true\n{chart_ref}",
                    release.namespace
                ),
            );
            if !merged.is_empty() {
                out.push_str("  values:\n");
                out.push_str(&indent_block(merged, 4));
            }
            out.push_str("---\n");
        }
        trim_separator(out)
    }

    /// Argo CD `Application`s, and a repository Secret per OCI registry path.
    fn argo_cd(&self) -> String {
        let mut out = self.k3s_packaged();
        push(
            &mut out,
            format!(
                "# Profile: {}. Apply in the namespace Argo CD runs in (argocd).\n",
                self.profile
            ),
        );
        out.push_str(&self.secret_objects());
        for (release, merged) in self.file.releases.iter().zip(&self.merged) {
            if let Some(text) = Self::manifest_release(release) {
                out.push_str(&text);
                continue;
            }
            let name = release.release_name();
            let (repo_url, chart, version) = match release.source() {
                Some(Source::Oci { url, version }) => {
                    let path = url.trim_start_matches("oci://");
                    let (registry_path, chart) = path.rsplit_once('/').unwrap_or((path, path));
                    push(
                        &mut out,
                        format!(
                            "# Argo CD reads OCI Helm charts through a repository with enableOCI.\napiVersion: v1\nkind: Secret\nmetadata:\n  name: {name}-oci\n  namespace: argocd\n  labels:\n    argocd.argoproj.io/secret-type: repository\nstringData:\n  type: helm\n  name: {name}-oci\n  url: {registry_path}\n  enableOCI: \"true\"\n---\n"
                        ),
                    );
                    (registry_path.to_owned(), chart.to_owned(), version)
                }
                Some(Source::Repo {
                    repo,
                    chart,
                    version,
                }) => (repo.to_owned(), chart.to_owned(), version),
                _ => continue,
            };
            push(
                &mut out,
                format!(
                    "apiVersion: argoproj.io/v1alpha1\nkind: Application\nmetadata:\n  name: {name}\n  namespace: argocd\nspec:\n  project: default\n  source:\n    repoURL: {repo_url}\n    chart: {chart}\n    targetRevision: {version}\n    helm:\n      releaseName: {name}\n"
                ),
            );
            if !merged.is_empty() {
                out.push_str("      valuesObject:\n");
                out.push_str(&indent_block(merged, 8));
            }
            push(
                &mut out,
                format!(
                    "  destination:\n    server: https://kubernetes.default.svc\n    namespace: {}\n  syncPolicy:\n    syncOptions:\n      - CreateNamespace=true\n      - ServerSideApply=true\n---\n",
                    release.namespace
                ),
            );
        }
        trim_separator(out)
    }
}

fn placeholder(input: &Input) -> Value {
    if input.generate.is_some() {
        Value::String("<random: openssl rand -hex 64>".to_owned())
    } else {
        input
            .default
            .clone()
            .unwrap_or_else(|| Value::String(format!("<{}>", input.name)))
    }
}

fn parse_values(text: Option<&str>) -> Result<Value, String> {
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
fn merge(base: &mut Value, over: Value) {
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

/// Sets `value` at a dotted path; numeric segments index lists, which grow as needed.
fn set_path(root: &mut Value, path: &str, value: Value) {
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
        for (capability, id, _) in bundle::FILES {
            for profile in PROFILES {
                let component = format!("{capability}/{id}");
                let guide = guide(&component, profile)
                    .unwrap_or_else(|e| panic!("{component} {profile}: {e}"));
                assert_eq!(guide.formats.len(), 4);
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

    #[test]
    fn components_k3s_also_packages_say_to_disable_its_copy_first() {
        for (capability, id, _) in bundle::FILES {
            let component = format!("{capability}/{id}");
            let guide = guide(&component, "single-node").unwrap();
            let expected: &[&str] = match component.as_str() {
                "ingress/traefik" => &["traefik"],
                "metrics/metrics-server" => &["metrics-server"],
                _ => &[],
            };
            assert_eq!(guide.replaces_k3s_packaged, expected, "{component}");
            for text in &guide.formats {
                assert_eq!(
                    text.text
                        .starts_with("# On k3s, first disable the packaged "),
                    !expected.is_empty(),
                    "{component} {:?}",
                    text.format
                );
            }
        }
    }

    /// Representative cases: an OCI chart with a profile (cert-manager), an HTTP repository with
    /// asked inputs (external-dns), generated inputs stored in Secrets (crowdsec), a manifest
    /// release before a chart (traefik), and the k3s packaged notes (traefik, metrics-server).
    const GOLDEN: [(&str, &str); 6] = [
        ("certificates/cert-manager", "single-node"),
        ("certificates/cert-manager", "multi-node"),
        ("dns/external-dns", "single-node"),
        ("intrusion-prevention/crowdsec", "single-node"),
        ("ingress/traefik", "multi-node"),
        ("metrics/metrics-server", "single-node"),
    ];

    #[test]
    fn install_texts_match_their_golden_files() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
        let update = std::env::var_os("ZEABUR_UPDATE_GOLDEN").is_some();
        let mut stale = Vec::new();
        for (component, profile) in GOLDEN {
            for text in guide(component, profile).unwrap().formats {
                let format = match text.format {
                    InstallFormat::Helm => "helm.sh",
                    InstallFormat::K3s => "k3s.yaml",
                    InstallFormat::Flux => "flux.yaml",
                    InstallFormat::ArgoCd => "argocd.yaml",
                };
                let path = dir.join(format!(
                    "{}-{profile}-{format}",
                    component.replace('/', "-")
                ));
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
