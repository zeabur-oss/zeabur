//! What installing a catalog component writes, and whether what it wrote is ready.
//!
//! An install is a list of [`Step`]s, one per release, in the catalog's install order
//! [installers]: the components the asked one needs and does not find on the cluster, then it.
//! A step holds the objects to server-side apply: the release's namespace with its Pod Security
//! label, then a K3s/RKE2 `HelmChart` for a chart [k3s-helm], or the objects of an inline
//! release or of a manifest release's files. A step waits for the one before it, because a
//! release often uses the CRDs of an earlier one.
//!
//! The engine does no I/O: the caller downloads the files of each [`Download`], checks them with
//! [`verify`] and passes them in as [`Manifests`].
//!
//! A chart release's values are its own, then its profile's, then the `contributes` of every
//! component Zeabur wrote or installs that targets it, merged in id order [releases], then its
//! inputs. Every plan
//! computes them, so upgrading a release keeps what others add to it. Installing a contributor
//! adds a step that writes its target again: the `HelmChart` Zeabur wrote, or, for a chart the
//! distribution packages, a `HelmChartConfig` of the same name [k3s-config]. Removing one writes
//! the target without it, before its own objects go: a `HelmChartConfig` left with no values
//! is written empty, so the removal waits until helm-controller installs the chart without
//! them, and is deleted after the component's own objects. A
//! contribution to a component that is not installed waits until Zeabur installs it.
//!
//! Readiness follows kstatus [kstatus] through `zeabur_model::readiness`. A `HelmChart` has no
//! `observedGeneration`, and helm-controller replaces its Job whenever the chart changes, so a
//! chart is ready when the Job its status names was created at or after Zeabur's last write of
//! the chart and has completed [helm-controller].
//!
//! [installers]: https://github.com/zeabur-oss/catalog/blob/main/docs/reference/installers.mdx
//! [k3s-helm]: https://docs.k3s.io/add-ons/helm
//! [kstatus]: https://github.com/kubernetes-sigs/cli-utils/blob/v0.37.2/pkg/kstatus/README.md
//! [helm-controller]: https://github.com/k3s-io/helm-controller/blob/v0.16.27/pkg/controllers/chart/chart.go
//! [releases]: https://github.com/zeabur-oss/catalog/blob/main/docs/reference/releases.mdx
//! [k3s-config]: https://docs.k3s.io/add-ons/helm#customizing-packaged-components-with-helmchartconfig

use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use zeabur_catalog_format::common::{Encoding, Generate, Mode, PodSecurity, Reference};
use zeabur_catalog_format::component::{
    Component, Health, HealthKind, HelmRelease, Input, InputValue, Release,
};
use zeabur_kube::k8s_openapi::api::batch::v1::Job;
use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, Secret};
use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::k8s_openapi::{ByteString, Metadata, NamespaceResourceScope, Resource};
use zeabur_kube::{ClusterFacts, DynamicObject, ServedResource};
use zeabur_model::PodSecurityLevel;
use zeabur_model::kernel::isolation::PLATFORM_LABEL;
use zeabur_model::kernel::names::is_system_namespace;
use zeabur_model::readiness::{self, Explained, GenericStatus, Reason, Status};

use crate::api::{ComponentFile, Snapshot};
use crate::install::{merge, parse_values, set_path};

/// The label that marks what Zeabur installed, with [`MANAGED_BY_ZEABUR`].
pub const MANAGED_BY: &str = "app.kubernetes.io/managed-by";
/// Its value.
pub const MANAGED_BY_ZEABUR: &str = "zeabur";
/// The label naming the component an object belongs to.
pub const COMPONENT: &str = "catalog.zeabur.com/component";
/// The label naming the release an object belongs to.
pub const RELEASE: &str = "catalog.zeabur.com/release";
/// The annotation naming the component file's `sourceDigest` an object was written from.
pub const SOURCE_DIGEST: &str = "catalog.zeabur.com/source-digest";
/// The annotation naming the catalog release an object was written from.
pub const CATALOG_VERSION: &str = "catalog.zeabur.com/version";
/// Where `HelmChart`s go: the namespace K3s's and RKE2's helm-controller watch.
pub const CHART_NAMESPACE: &str = "kube-system";
/// The field manager Zeabur writes as.
pub const FIELD_MANAGER: &str = "zeabur";
/// The annotation on a `HelmChart` that keeps the given and generated values of its release's
/// `path` inputs, as a JSON object of input name to value, for the next install to reuse.
pub const INPUTS: &str = "catalog.zeabur.com/inputs";

/// Values of inputs, by component and then by input name.
pub type Values = BTreeMap<String, BTreeMap<String, InputValue>>;

/// The values an install starts from.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    /// What the person gives; an empty text is no value.
    pub given: &'a Values,
    /// What an earlier install wrote: the `INPUTS` annotation and the keys of the Secrets Zeabur
    /// wrote.
    pub kept: &'a Values,
    /// The cluster's Service addresses, for an input that `generate: {type: clusterIP}`; `None`
    /// leaves such an input empty.
    pub addresses: Option<&'a ServiceAddresses>,
}

/// The cluster's Service CIDRs and the addresses Services hold in them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServiceAddresses {
    /// The `spec.cidrs` of every `ServiceCIDR`, such as `10.43.0.0/16`.
    pub cidrs: Vec<String>,
    /// The name of every `IPAddress`: the addresses Services hold.
    pub taken: BTreeSet<IpAddr>,
}

impl ServiceAddresses {
    /// The lowest address of the first CIDR's static band that neither a Service nor `picked`
    /// holds. Kubernetes assigns addresses from the band above it first, so the band is for fixed
    /// `ClusterIP`s; it holds `min(max(16, size / 16), 256)` addresses after the first
    /// [cluster-ip-allocation].
    ///
    /// [cluster-ip-allocation]: https://kubernetes.io/docs/concepts/services-networking/cluster-ip-allocation/
    #[must_use]
    pub fn free(&self, picked: &BTreeSet<IpAddr>) -> Option<IpAddr> {
        let (base, host_bits) = self.cidrs.first().and_then(|c| cidr(c))?;
        let size = 1_u128
            .checked_shl(u32::from(host_bits))
            .unwrap_or(u128::MAX);
        let band = (size / 16).clamp(16, 256).min(size.saturating_sub(2));
        (1..=band)
            .filter_map(|offset| plus(base, offset))
            .find(|a| !self.taken.contains(a) && !picked.contains(a))
    }
}

/// The network address and the number of host bits of a CIDR such as `10.43.0.0/16`.
fn cidr(text: &str) -> Option<(IpAddr, u8)> {
    let (address, prefix) = text.split_once('/')?;
    let base: IpAddr = address.parse().ok()?;
    let bits = if base.is_ipv4() { 32 } else { 128 };
    Some((
        base,
        bits - prefix.parse::<u8>().ok().filter(|p| *p <= bits)?,
    ))
}

/// `base` plus `offset`.
fn plus(base: IpAddr, offset: u128) -> Option<IpAddr> {
    Some(match base {
        IpAddr::V4(v4) => IpAddr::V4(
            u32::from(v4)
                .checked_add(u32::try_from(offset).ok()?)?
                .into(),
        ),
        IpAddr::V6(v6) => IpAddr::V6(u128::from(v6).checked_add(offset)?.into()),
    })
}

/// Where an input's value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum InputState {
    /// The person gives it.
    Given,
    /// An earlier install wrote it.
    Kept,
    /// The catalog's default.
    Default,
    /// Zeabur makes it: random bytes, or a free Service address.
    Generated,
    /// None, and none is needed.
    Empty,
    /// None, and the install needs one.
    Missing,
}

/// How a form asks for an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum InputKind {
    /// Text.
    Text,
    /// `true` or `false`.
    Flag,
    /// A whole number.
    Integer,
    /// Text stored in a Secret, never shown back.
    Secret,
}

impl InputKind {
    /// The kind of `input`: a Secret's, else its default's.
    #[must_use]
    pub const fn of(input: &Input) -> Self {
        if input.secret.is_some() {
            return Self::Secret;
        }
        match input.default {
            Some(InputValue::Flag(_)) => Self::Flag,
            Some(InputValue::Integer(_)) => Self::Integer,
            Some(InputValue::Text(_)) | None => Self::Text,
        }
    }

    const fn admits(self, value: &InputValue) -> bool {
        matches!(
            (self, value),
            (Self::Text | Self::Secret, InputValue::Text(_))
                | (Self::Flag, InputValue::Flag(_))
                | (Self::Integer, InputValue::Integer(_))
        )
    }
}

/// One input of an install, with the value it writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The component.
    pub component: String,
    /// The input, as the catalog declares it.
    pub input: Input,
    /// Where the value comes from.
    pub state: InputState,
    /// The value; `None` when [`InputState::Empty`] or [`InputState::Missing`].
    pub value: Option<InputValue>,
}

/// The cluster an install is for.
#[derive(Debug, Clone, Copy)]
pub struct Target<'a> {
    /// `single-node` or `multi-node`.
    pub profile: &'a str,
    /// `k3s`, `rke2` or `kubesolo`; `None` for any other cluster.
    pub distribution: Option<&'a str>,
    /// The nodes' platform, `linux/amd64`.
    pub platform: &'a str,
    /// kube-apiserver's `gitVersion`, `v1.36.5+k3s1`.
    pub kube_version: &'a str,
    /// The capabilities the cluster has.
    pub capabilities: &'a BTreeSet<String>,
    /// The components the cluster has.
    pub components: &'a BTreeSet<String>,
    /// The components Zeabur wrote, finished or not; [`install`] plans them again so that one
    /// it began is finished.
    pub written: &'a BTreeSet<String>,
    /// The components the distribution packages, with their `HelmChart` in `kube-system`.
    pub packaged: &'a BTreeMap<String, String>,
    /// The namespace Zeabur runs in, which [`crate::zeabur::objects`] admit.
    pub zeabur_namespace: &'a str,
}

/// One release to apply.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// The component.
    pub component: String,
    /// The release's name in the component file.
    pub release: String,
    /// The objects to apply, in order.
    pub objects: Vec<Desired>,
    /// CRDs, as `plural.group`: when every one exists and Zeabur did not write it, the step is
    /// left out.
    pub skip_if_present: Vec<String>,
}

/// The text of each file of the manifest releases, by URL.
pub type Manifests = BTreeMap<String, std::sync::Arc<str>>;

/// One file of a manifest release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    /// Its URL.
    pub url: String,
    /// The hex SHA-256 the catalog lists for it.
    pub sha256: String,
}

/// One object to apply.
#[derive(Debug, Clone, PartialEq)]
pub struct Desired {
    /// The object; `types` is always set.
    pub object: DynamicObject,
    /// The namespace it goes in when its kind is namespaced and it names none.
    pub namespace: String,
}

/// What to know before installing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// The catalog's e2e has not passed the component on this distribution and platform.
    Untested {
        /// The component.
        component: String,
    },
    /// The component's namespaces run pods with more than the `restricted` Pod Security level.
    Privileged {
        /// The component.
        component: String,
        /// `baseline` or `privileged`.
        level: PodSecurityLevel,
    },
    /// Another tool installed a component that the install adds values to, so Zeabur leaves
    /// it as it is.
    Unchanged {
        /// The component.
        component: String,
        /// Every value Zeabur's components add to it, as YAML, for the person to add.
        values: String,
    },
}

/// An install.
#[derive(Debug, Clone, PartialEq)]
pub struct Install {
    /// The components it installs, in order: the ones needed, then the one asked for.
    pub components: Vec<String>,
    /// The releases, in order.
    pub steps: Vec<Step>,
    /// What to know.
    pub warnings: Vec<Warning>,
    /// The inputs of its components, in order.
    pub inputs: Vec<Resolved>,
}

impl Install {
    /// The refusal of the first required input without a value.
    #[must_use]
    pub fn missing(&self) -> Option<Refusal> {
        self.inputs
            .iter()
            .find(|r| r.state == InputState::Missing)
            .map(|r| Refusal::Missing {
                component: r.component.clone(),
                input: r.input.name.clone(),
            })
    }
}

/// What a component uses that Zeabur cannot install yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum Unsupported {
    /// It adds values to other components' releases and has no Helm chart: Zeabur knows the
    /// components it installed by their `HelmChart`s, so its values would leave their targets
    /// when they are written again.
    ContributesWithoutChart,
    /// It has a release type, a Pod Security level or a way to generate an input that this
    /// build does not know.
    NewerRelease,
    /// Its file in the catalog release Zeabur read does not decode.
    Unreadable,
}

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ContributesWithoutChart => {
                "adds values to other components but has no Helm chart"
            }
            Self::NewerRelease => "needs something this version does not know",
            Self::Unreadable => "has a catalog file this version cannot read",
        })
    }
}

/// What the component `id` of `snapshot` uses that Zeabur cannot install yet, wherever it is
/// installed; `None` for a component `snapshot` does not list.
#[must_use]
pub fn unsupported_in(snapshot: &Snapshot, id: &str) -> Option<Unsupported> {
    if snapshot.problems.contains_key(id) {
        return Some(Unsupported::Unreadable);
    }
    snapshot.components.get(id).and_then(unsupported)
}

/// The file of the component `id`, or why there is none to install.
fn file_of<'a>(snapshot: &'a Snapshot, id: &str) -> Result<&'a ComponentFile, Refusal> {
    snapshot.components.get(id).ok_or_else(|| {
        if snapshot.problems.contains_key(id) {
            Refusal::Unsupported {
                component: id.to_owned(),
                why: Unsupported::Unreadable,
            }
        } else {
            Refusal::Unknown(id.to_owned())
        }
    })
}

/// What `file` uses that Zeabur cannot install yet, wherever it is installed.
#[must_use]
pub fn unsupported(file: &ComponentFile) -> Option<Unsupported> {
    let d = &file.declaration;
    if !d.contributes.is_empty() && !d.releases.iter().any(|r| r.helm().is_some()) {
        return Some(Unsupported::ContributesWithoutChart);
    }
    (!d.known()).then_some(Unsupported::NewerRelease)
}

/// The Pod Security level of `component`'s namespaces; `None` for a level this build does not know,
/// which [`unsupported`] refuses.
#[must_use]
pub const fn pod_security(component: &Component) -> Option<PodSecurityLevel> {
    match component.security.pod_security {
        PodSecurity::Restricted => Some(PodSecurityLevel::Restricted),
        PodSecurity::Baseline => Some(PodSecurityLevel::Baseline),
        PodSecurity::Privileged => Some(PodSecurityLevel::Privileged),
        PodSecurity::Unknown => None,
    }
}

/// Why a component cannot be installed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// The catalog has no such component.
    #[error("Zeabur Catalog has no component {0}")]
    Unknown(String),
    /// Zeabur cannot install it yet.
    #[error("Zeabur cannot install {component} yet: it {why}")]
    Unsupported {
        /// The component.
        component: String,
        /// What it uses.
        why: Unsupported,
    },
    /// Its catalog file does not read.
    #[error("Zeabur Catalog's file of {component} does not read: {message}")]
    Broken {
        /// The component.
        component: String,
        /// Why.
        message: String,
    },
    /// A required input has no value.
    #[error("{component} needs a value for {input}.")]
    Missing {
        /// The component.
        component: String,
        /// The input.
        input: String,
    },
    /// A given value is not of the input's kind.
    #[error("{input} of {component} takes {expected}.")]
    WrongValue {
        /// The component.
        component: String,
        /// The input.
        input: String,
        /// What it takes, in words.
        expected: &'static str,
    },
    /// The catalog does not offer it here.
    #[error("Zeabur Catalog does not offer {component} on {place}")]
    NotHere {
        /// The component.
        component: String,
        /// The profile or distribution.
        place: String,
    },
    /// No component provides a capability it needs.
    #[error("no component of Zeabur Catalog provides {0}")]
    NoProvider(String),
}

/// The components installing `id` on `target` installs, in order: the ones it needs and the
/// cluster lacks, then `id`.
///
/// # Errors
///
/// The component or one it needs is unknown, not offered here, or uses what Zeabur cannot
/// install yet.
pub fn components(
    snapshot: &Snapshot,
    id: &str,
    target: &Target<'_>,
) -> Result<Vec<String>, Refusal> {
    let mut order = Vec::new();
    install_order(snapshot, id, target, &mut order, &mut BTreeSet::new())?;
    for component in &order {
        let file = file_of(snapshot, component)?;
        check(file, target)?;
    }
    Ok(order)
}

/// How to install `id` on `target` with `inputs` and the files of its manifest releases.
///
/// A required input without a value is [`InputState::Missing`], and [`Install::missing`] refuses
/// it, so a plan can still show the form.
///
/// # Errors
///
/// What [`components`] refuses, a given value of the wrong kind, or a catalog file that does
/// not read, or a file of `manifests` is missing.
pub fn install(
    snapshot: &Snapshot,
    id: &str,
    target: &Target<'_>,
    manifests: &Manifests,
    inputs: Inputs<'_>,
) -> Result<Install, Refusal> {
    let order = components(snapshot, id, target)?;
    let version = &snapshot.index.catalog.version;
    let installed: BTreeSet<String> = target.written.iter().chain(&order).cloned().collect();
    let contributions = Contributions::of(snapshot, &installed)?;
    // A component installed in this plan is written again like one Zeabur wrote before.
    let after = Target {
        written: &installed,
        ..*target
    };
    let mut earlier: BTreeSet<String> = target
        .written
        .difference(&order.iter().cloned().collect())
        .cloned()
        .collect();
    let mut steps = Vec::new();
    let mut warnings = Vec::new();
    let mut resolved = Vec::new();
    let mut picked = fixed_addresses(snapshot, &order, inputs);
    for (i, component) in order.iter().enumerate() {
        let file = file_of(snapshot, component)?;
        if let Some(distribution) = target.distribution
            && !file.tested_on(&snapshot.index).iter().any(|t| {
                t.distribution.as_str() == distribution
                    && t.platforms.iter().any(|p| p == target.platform)
            })
        {
            warnings.push(Warning::Untested {
                component: component.clone(),
            });
        }
        let level = pod_security(&file.declaration).ok_or_else(|| Refusal::Unsupported {
            component: component.clone(),
            why: Unsupported::NewerRelease,
        })?;
        if level != PodSecurityLevel::Restricted {
            warnings.push(Warning::Privileged {
                component: component.clone(),
                level,
            });
        }
        let own = resolve(file, inputs, &mut picked)?;
        // Only the components before it add values: a contributor's objects are not there yet.
        steps.extend(steps_of(
            file,
            version,
            target.profile,
            manifests,
            &own,
            &Contributions::of(snapshot, &earlier)?,
            target.zeabur_namespace,
        )?);
        earlier.insert(component.clone());
        for c in &file.declaration.contributes {
            let (to, release) = (&c.target.component, &c.target.release);
            let again = |id: &String| {
                snapshot.components.get(id).is_some_and(|f| {
                    f.declaration
                        .contributes
                        .iter()
                        .any(|o| o.target == c.target)
                })
            };
            // A target installed later has the values in its own step, and a later contributor
            // writes it again once for all.
            if order.iter().skip(i + 1).any(|o| o == to || again(o)) {
                continue;
            }
            let values = contributions.to(to, release);
            match rewrite(snapshot, &after, to, release, inputs, &contributions)? {
                Some(step) => steps.push(step),
                None if target.components.contains(to) => warnings.push(Warning::Unchanged {
                    component: to.clone(),
                    values: yaml_values(&values).map_err(broken(&file.id))?,
                }),
                None => {}
            }
        }
        resolved.extend(own);
    }
    Ok(Install {
        components: order,
        steps,
        warnings,
        inputs: resolved,
    })
}

/// The addresses that the `clusterIP` inputs of `order` already have, given, kept or by
/// default, so that no generated address is the same.
fn fixed_addresses(snapshot: &Snapshot, order: &[String], inputs: Inputs<'_>) -> BTreeSet<IpAddr> {
    let mut fixed = BTreeSet::new();
    for file in order.iter().filter_map(|id| snapshot.components.get(id)) {
        for input in &file.declaration.inputs {
            if input.generate != Some(Generate::ClusterIp) {
                continue;
            }
            let set = [inputs.given, inputs.kept]
                .map(|values| values.get(&file.id).and_then(|m| m.get(&input.name)));
            for value in set.into_iter().chain([input.default.as_ref()]).flatten() {
                if let InputValue::Text(text) = value
                    && let Ok(address) = text.parse()
                {
                    fixed.insert(address);
                }
            }
        }
    }
    fixed
}

/// The value of each input of `file`: given, else kept, else the default, else generated. An
/// address it generates goes into `picked`, so no other input takes it.
fn resolve(
    file: &ComponentFile,
    inputs: Inputs<'_>,
    picked: &mut BTreeSet<IpAddr>,
) -> Result<Vec<Resolved>, Refusal> {
    let of = |values: &Values, name: &str| {
        values
            .get(&file.id)
            .and_then(|m| m.get(name))
            .filter(|v| !matches!(v, InputValue::Text(t) if t.is_empty()))
            .cloned()
    };
    let mut resolved = Vec::new();
    for input in &file.declaration.inputs {
        let kind = InputKind::of(input);
        let (state, value) = if let Some(given) = of(inputs.given, &input.name) {
            if !kind.admits(&given) {
                return Err(Refusal::WrongValue {
                    component: file.id.clone(),
                    input: input.name.clone(),
                    expected: match kind {
                        InputKind::Flag => "true or false",
                        InputKind::Integer => "a whole number",
                        InputKind::Text | InputKind::Secret => "text",
                    },
                });
            }
            (InputState::Given, Some(given))
        } else if let Some(kept) = of(inputs.kept, &input.name) {
            (InputState::Kept, Some(kept))
        } else if let Some(default) = &input.default {
            (InputState::Default, Some(default.clone()))
        } else if let Some(value) = generated(file, input, inputs.addresses, picked)? {
            (InputState::Generated, Some(InputValue::Text(value)))
        } else if input.required {
            (InputState::Missing, None)
        } else {
            (InputState::Empty, None)
        };
        resolved.push(Resolved {
            component: file.id.clone(),
            input: input.clone(),
            state,
            value,
        });
    }
    Ok(resolved)
}

/// The value Zeabur makes for `input`, if it makes one: random bytes, or a free Service address
/// that goes into `picked`.
fn generated(
    file: &ComponentFile,
    input: &Input,
    addresses: Option<&ServiceAddresses>,
    picked: &mut BTreeSet<IpAddr>,
) -> Result<Option<String>, Refusal> {
    match &input.generate {
        Some(Generate::Random { bytes, encoding }) => {
            random(*bytes, *encoding)
                .map(Some)
                .map_err(|message| Refusal::Broken {
                    component: file.id.clone(),
                    message,
                })
        }
        Some(Generate::ClusterIp) => {
            let free = addresses.and_then(|a| a.free(picked));
            picked.extend(free);
            Ok(free.map(|a| a.to_string()))
        }
        Some(Generate::Unknown) | None => Ok(None),
    }
}

/// `bytes` random bytes in `encoding`.
fn random(bytes: u8, encoding: Encoding) -> Result<String, String> {
    let mut buffer = vec![0; usize::from(bytes)];
    aws_lc_rs::rand::fill(&mut buffer).map_err(|_unspecified| "no random bytes".to_owned())?;
    Ok(match encoding {
        Encoding::Hex => hex::encode(buffer),
        Encoding::Base64url => {
            use base64::Engine as _;
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buffer)
        }
    })
}

/// Appends what installing `id` installs: what it needs and the cluster lacks, then `id`.
fn install_order(
    snapshot: &Snapshot,
    id: &str,
    target: &Target<'_>,
    order: &mut Vec<String>,
    visiting: &mut BTreeSet<String>,
) -> Result<(), Refusal> {
    if order.iter().any(|c| c == id) || !visiting.insert(id.to_owned()) {
        return Ok(());
    }
    let file = file_of(snapshot, id)?;
    for reference in &file.declaration.requires {
        let needed = match reference {
            Reference::Component(component)
                if target.components.contains(component) && !target.written.contains(component) =>
            {
                continue;
            }
            Reference::Component(component) => component.clone(),
            Reference::Capability(capability) => {
                match written_provider(snapshot, target, capability) {
                    Some(id) => id,
                    None if target.capabilities.contains(capability) => continue,
                    None => crate::api::profile(target.profile)
                        .and_then(|profile| snapshot.provider(capability, profile))
                        .map(|c| c.id.clone())
                        .ok_or_else(|| Refusal::NoProvider(capability.clone()))?,
                }
            }
        };
        install_order(snapshot, &needed, target, order, visiting)?;
    }
    order.push(id.to_owned());
    Ok(())
}

/// The component Zeabur wrote that provides `capability`.
fn written_provider(snapshot: &Snapshot, target: &Target<'_>, capability: &str) -> Option<String> {
    target
        .written
        .iter()
        .find(|id| {
            snapshot
                .components
                .get(*id)
                .is_some_and(|f| f.declaration.capability() == Some(capability))
        })
        .cloned()
}

/// Refuses what the catalog does not offer here and what Zeabur cannot install yet.
fn check(file: &ComponentFile, target: &Target<'_>) -> Result<(), Refusal> {
    let d = &file.declaration;
    match crate::api::profile(target.profile).map(|p| d.mode(p)) {
        Some(Mode::Recommended | Mode::Optional) => {}
        _ => {
            return Err(Refusal::NotHere {
                component: file.id.clone(),
                place: format!("a {} cluster", target.profile),
            });
        }
    }
    if let Some(distribution) = target.distribution
        && !d.distributions.iter().any(|x| x.as_str() == distribution)
    {
        return Err(Refusal::NotHere {
            component: file.id.clone(),
            place: distribution.to_owned(),
        });
    }
    if !crate::kube_version::admits(&d.kube_version, target.kube_version).unwrap_or(false) {
        return Err(Refusal::NotHere {
            component: file.id.clone(),
            place: format!("Kubernetes {}", target.kube_version),
        });
    }
    unsupported(file).map_or(Ok(()), |why| {
        Err(Refusal::Unsupported {
            component: file.id.clone(),
            why,
        })
    })
}

/// What components add to other components' releases.
#[derive(Debug, Clone, Default)]
pub struct Contributions(BTreeMap<(String, String), Value>);

impl Contributions {
    /// What `contributors` add, each release's merged in id order, so the later id wins a key
    /// both set.
    ///
    /// # Errors
    ///
    /// A contribution does not read.
    pub fn of(snapshot: &Snapshot, contributors: &BTreeSet<String>) -> Result<Self, Refusal> {
        let mut added = BTreeMap::new();
        for id in contributors {
            let Some(file) = snapshot.components.get(id) else {
                continue;
            };
            for c in &file.declaration.contributes {
                let values = parse_values(Some(&c.values)).map_err(broken(id))?;
                let key = (c.target.component.clone(), c.target.release.clone());
                merge(
                    added
                        .entry(key)
                        .or_insert_with(|| Value::Object(serde_json::Map::new())),
                    values,
                );
            }
        }
        Ok(Self(added))
    }

    /// What is added to `component`'s `release`; an empty map when nothing is.
    #[must_use]
    pub fn to(&self, component: &str, release: &str) -> Value {
        self.0
            .get(&(component.to_owned(), release.to_owned()))
            .cloned()
            .unwrap_or_else(|| Value::Object(serde_json::Map::new()))
    }
}

/// The steps that write again what removing `id` takes its values out of.
///
/// Each writes a `HelmChart` Zeabur wrote, or a `HelmChartConfig` of a packaged chart. A
/// `HelmChartConfig` left with no values has an empty `valuesContent`.
///
/// # Errors
///
/// A release or contribution does not read.
pub fn without(
    snapshot: &Snapshot,
    id: &str,
    target: &Target<'_>,
    kept: &Values,
) -> Result<Vec<Step>, Refusal> {
    let Some(file) = snapshot.components.get(id) else {
        return Ok(Vec::new());
    };
    let rest = target
        .written
        .iter()
        .filter(|c| *c != id)
        .cloned()
        .collect();
    let contributions = Contributions::of(snapshot, &rest)?;
    let mut steps: Vec<Step> = Vec::new();
    for c in &file.declaration.contributes {
        let (to, release) = (&c.target.component, &c.target.release);
        if steps
            .iter()
            .any(|s| &s.component == to && &s.release == release)
        {
            continue;
        }
        let inputs = Inputs {
            given: &Values::new(),
            kept,
            addresses: None,
        };
        steps.extend(rewrite(
            snapshot,
            target,
            to,
            release,
            inputs,
            &contributions,
        )?);
    }
    Ok(steps)
}

/// The step that writes `component`'s `release` again with `contributions`: its `HelmChart`
/// when Zeabur wrote it, else the `HelmChartConfig` of the chart the distribution packages;
/// `None` for any other component.
fn rewrite(
    snapshot: &Snapshot,
    target: &Target<'_>,
    component: &str,
    release: &str,
    inputs: Inputs<'_>,
    contributions: &Contributions,
) -> Result<Option<Step>, Refusal> {
    let version = &snapshot.index.catalog.version;
    let Some(file) = snapshot.components.get(component) else {
        return Ok(None);
    };
    if target.written.contains(component) {
        let Some(r) = file.declaration.release(release).and_then(Release::helm) else {
            return Ok(None);
        };
        let own = resolve(file, inputs, &mut BTreeSet::new())?;
        let mut object =
            chart_of(file, r, target.profile, contributions, &own).map_err(broken(component))?;
        mark(&mut object.metadata, file, &r.name, version);
        return Ok(Some(Step {
            component: component.to_owned(),
            release: release.to_owned(),
            objects: vec![Desired {
                object,
                namespace: r.namespace.clone(),
            }],
            skip_if_present: Vec::new(),
        }));
    }
    let Some(chart) = target.packaged.get(component) else {
        return Ok(None);
    };
    let values = contributions.to(component, release);
    let mut config = HelmChartConfig {
        metadata: ObjectMeta {
            name: Some(chart.clone()),
            namespace: Some(CHART_NAMESPACE.to_owned()),
            ..ObjectMeta::default()
        },
        spec: HelmChartConfigSpec {
            values_content: Some(
                values_content(&values)
                    .map_err(broken(component))?
                    .unwrap_or_default(),
            ),
        },
    };
    label_release(&mut config.metadata, component, release);
    config
        .metadata
        .annotations
        .get_or_insert_default()
        .insert(CATALOG_VERSION.to_owned(), version.clone());
    Ok(Some(Step {
        component: component.to_owned(),
        release: release.to_owned(),
        objects: vec![Desired {
            object: dynamic(&config).map_err(broken(component))?,
            namespace: CHART_NAMESPACE.to_owned(),
        }],
        skip_if_present: Vec::new(),
    }))
}

fn broken(component: &str) -> impl Fn(String) -> Refusal + Copy {
    move |message| Refusal::Broken {
        component: component.to_owned(),
        message,
    }
}

/// The files the manifest releases of `file` install.
///
/// # Errors
///
/// The catalog lists no `sha256` digest for a URL.
pub fn downloads(file: &ComponentFile) -> Result<Vec<Download>, Refusal> {
    let mut downloads = Vec::new();
    for release in &file.declaration.releases {
        let Release::Manifest(release) = release else {
            continue;
        };
        for (index, url) in release.resolved_urls().into_iter().enumerate() {
            let Some(sha256) = release
                .url_digests
                .get(index)
                .and_then(|d| d.strip_prefix("sha256:"))
            else {
                return Err(Refusal::Broken {
                    component: file.id.clone(),
                    message: format!("it lists no sha256 digest for {url}"),
                });
            };
            downloads.push(Download {
                url,
                sha256: sha256.to_ascii_lowercase(),
            });
        }
    }
    Ok(downloads)
}

/// The text of a downloaded file when its SHA-256 is the catalog's.
///
/// # Errors
///
/// The digest differs, or the file is not UTF-8.
pub fn verify(download: &Download, bytes: Vec<u8>) -> Result<String, String> {
    let got = hex::encode(Sha256::digest(&bytes));
    if got != download.sha256 {
        return Err(format!(
            "{} has sha256 {got}, and Zeabur Catalog lists {}",
            download.url, download.sha256
        ));
    }
    String::from_utf8(bytes).map_err(|_e| format!("{} is not UTF-8 text", download.url))
}

/// The steps of one component alone, with the files of its manifest releases and its `inputs`.
///
/// A removal passes no inputs, and deletes the steps' objects in reverse; the Secrets of inputs
/// stay.
///
/// The last step holds the objects Zeabur adds to the component ([`crate::zeabur`]) for a Zeabur
/// in `zeabur_namespace`.
///
/// Each Secret goes in the step of the first release in its namespace, or the first step, after
/// the namespace and before the release, so a chart finds it.
///
/// # Errors
///
/// A release does not read, or a file of `manifests` is missing.
pub fn steps_of(
    file: &ComponentFile,
    catalog_version: &str,
    profile: &str,
    manifests: &Manifests,
    inputs: &[Resolved],
    contributions: &Contributions,
    zeabur_namespace: &str,
) -> Result<Vec<Step>, Refusal> {
    let broken = broken(&file.id);
    let level = pod_security(&file.declaration)
        .ok_or_else(|| broken("its Pod Security level is unknown".to_owned()))?;
    let releases = &file.declaration.releases;
    let release_namespaces: Vec<&str> = releases.iter().filter_map(Release::namespace).collect();
    let mut secrets = secrets(inputs);
    let mut namespaces = BTreeSet::new();
    let mut steps = Vec::new();
    for (index, release) in releases.iter().enumerate() {
        let (Some(name), Some(release_namespace)) = (release.name(), release.namespace()) else {
            return Err(broken(
                "it has a release of a type this version does not know".to_owned(),
            ));
        };
        let mut objects = Vec::new();
        if namespaces.insert(release_namespace.to_owned())
            && let Some(namespace) = namespace(release_namespace, level)
        {
            objects.push(Desired {
                object: dynamic(&namespace).map_err(broken)?,
                namespace: String::new(),
            });
        }
        let here: Vec<Secret> = if index == 0 {
            let homeless = |s: &Secret| {
                !release_namespaces
                    .iter()
                    .any(|r| Some(*r) == s.metadata.namespace.as_deref())
            };
            let (here, rest) = secrets.into_iter().partition(|s| {
                s.metadata.namespace.as_deref() == Some(release_namespace) || homeless(s)
            });
            secrets = rest;
            here
        } else {
            let (here, rest) = secrets
                .into_iter()
                .partition(|s| s.metadata.namespace.as_deref() == Some(release_namespace));
            secrets = rest;
            here
        };
        let mut made = here
            .iter()
            .map(dynamic)
            .collect::<Result<Vec<_>, _>>()
            .map_err(broken)?;
        made.extend(match release {
            Release::Helm(chart) => {
                vec![chart_of(file, chart, profile, contributions, inputs).map_err(broken)?]
            }
            Release::Inline(inline) => objects_of(&inline.manifests).map_err(broken)?,
            Release::Manifest(manifest) => {
                manifest_objects(&release_namespaces, &manifest.resolved_urls(), manifests)
                    .map_err(broken)?
            }
            Release::Unknown => Vec::new(),
        });
        objects.extend(made.into_iter().map(|mut object| {
            mark(&mut object.metadata, file, name, catalog_version);
            Desired {
                object,
                namespace: release_namespace.to_owned(),
            }
        }));
        steps.push(Step {
            component: file.id.clone(),
            release: name.to_owned(),
            objects,
            skip_if_present: release.skip_if_present().to_vec(),
        });
    }
    steps.extend(zeabur_step(file, zeabur_namespace).map_err(broken)?);
    Ok(steps)
}

/// The step with the objects Zeabur adds to `file`'s component, if it adds any.
fn zeabur_step(file: &ComponentFile, zeabur_namespace: &str) -> Result<Option<Step>, String> {
    let objects = crate::zeabur::objects(&file.id, zeabur_namespace)?;
    Ok((!objects.is_empty()).then(|| Step {
        component: file.id.clone(),
        release: crate::zeabur::RELEASE.to_owned(),
        objects: objects
            .into_iter()
            .map(|object| Desired {
                namespace: object.metadata.namespace.clone().unwrap_or_default(),
                object,
            })
            .collect(),
        skip_if_present: Vec::new(),
    }))
}

/// The objects of a manifest release's files at `urls`, without the component's namespaces
/// (`namespaces`): each is written once, with its Pod Security level.
fn manifest_objects(
    namespaces: &[&str],
    urls: &[String],
    manifests: &Manifests,
) -> Result<Vec<DynamicObject>, String> {
    let mut objects = Vec::new();
    for url in urls {
        let text = manifests
            .get(url)
            .ok_or_else(|| format!("{url} was not downloaded"))?;
        objects.extend(objects_of(text)?);
    }
    objects.retain(|o| {
        o.types.as_ref().is_none_or(|t| t.kind != Namespace::KIND)
            || !namespaces
                .iter()
                .any(|n| o.metadata.name.as_deref() == Some(*n))
    });
    Ok(objects)
}

/// The Secrets of the `secret` inputs that have a value: the inputs that share a Secret are its
/// keys.
fn secrets(inputs: &[Resolved]) -> Vec<Secret> {
    let mut secrets: Vec<Secret> = Vec::new();
    for r in inputs {
        let (Some(at), Some(value)) = (&r.input.secret, &r.value) else {
            continue;
        };
        let index = secrets
            .iter()
            .position(|s| {
                s.metadata.name.as_ref() == Some(&at.name)
                    && s.metadata.namespace.as_ref() == Some(&at.namespace)
            })
            .unwrap_or_else(|| {
                secrets.push(Secret {
                    metadata: ObjectMeta {
                        name: Some(at.name.clone()),
                        namespace: Some(at.namespace.clone()),
                        ..ObjectMeta::default()
                    },
                    ..Secret::default()
                });
                secrets.len() - 1
            });
        if let Some(secret) = secrets.get_mut(index) {
            secret
                .data
                .get_or_insert_default()
                .insert(at.key.clone(), ByteString(value.text().into_bytes()));
        }
    }
    secrets
}

/// The Secrets a component's inputs are kept in, as `namespace/name`; a removal leaves them,
/// since a new install reads their keys back.
#[must_use]
pub fn input_secrets(file: &ComponentFile) -> Vec<String> {
    let mut names: Vec<String> = file
        .declaration
        .inputs
        .iter()
        .filter_map(|i| i.secret.as_ref())
        .map(|s| format!("{}/{}", s.namespace, s.name))
        .collect();
    names.dedup();
    names
}

/// Whether `found` already holds what `desired` writes.
///
/// Only the kinds whose content an input or a catalog release changes are compared: a
/// `HelmChart`'s `spec` and `INPUTS` annotation, a `HelmChartConfig`'s `spec`, and a Secret's
/// `data`. A step with an object that
/// does not hold it is written again. Other kinds count as current once they exist.
#[must_use]
pub fn holds(desired: &DynamicObject, found: &DynamicObject) -> bool {
    let field = match desired.types.as_ref().map(|t| t.kind.as_str()) {
        Some(HelmChart::KIND) if kept_inputs(desired) != kept_inputs(found) => return false,
        Some(HelmChart::KIND | HelmChartConfig::KIND) => "spec",
        Some(Secret::KIND) => "data",
        _ => return true,
    };
    let empty = Value::Null;
    covers(
        found.data.get(field).unwrap_or(&empty),
        desired.data.get(field).unwrap_or(&empty),
    )
}

fn kept_inputs(chart: &DynamicObject) -> Option<&String> {
    chart.metadata.annotations.as_ref()?.get(INPUTS)
}

/// Whether every field of `desired` is in `found` with the same value.
fn covers(found: &Value, desired: &Value) -> bool {
    match (found, desired) {
        (Value::Object(found), Value::Object(desired)) => desired
            .iter()
            .all(|(k, v)| found.get(k).is_some_and(|f| covers(f, v))),
        (_, Value::Null) => true,
        _ => found == desired,
    }
}

/// The `HelmChart` of a Helm release: its values with the profile's, then the contributions,
/// then the values of `inputs` at their paths. The inputs given, kept or generated go in its
/// `INPUTS` annotation, so a later install keeps them.
fn chart_of(
    file: &ComponentFile,
    release: &HelmRelease,
    profile: &str,
    contributions: &Contributions,
    inputs: &[Resolved],
) -> Result<DynamicObject, String> {
    let first_chart = file
        .declaration
        .releases
        .iter()
        .find_map(Release::helm)
        .map(|r| r.name.as_str());
    let mut values = profile_values(release, profile)?;
    merge(&mut values, contributions.to(&file.id, &release.name));
    let mut kept = serde_json::Map::new();
    for r in inputs {
        let (Some(path), Some(value)) = (&r.input.path, &r.value) else {
            continue;
        };
        if r.input.release.as_deref().or(first_chart) != Some(release.name.as_str()) {
            continue;
        }
        set_path(&mut values, path, value.to_json());
        if matches!(
            r.state,
            InputState::Given | InputState::Kept | InputState::Generated
        ) {
            kept.insert(r.input.name.clone(), value.to_json());
        }
    }
    let mut chart = helm_chart(release, &values)?;
    if !kept.is_empty() {
        chart
            .metadata
            .annotations
            .get_or_insert_default()
            .insert(INPUTS.to_owned(), Value::Object(kept).to_string());
    }
    dynamic(&chart)
}

/// `object` as a [`DynamicObject`], with its `apiVersion` and `kind`.
pub(crate) fn dynamic<K: Resource + Serialize>(object: &K) -> Result<DynamicObject, String> {
    let mut value = serde_json::to_value(object).map_err(|e| e.to_string())?;
    if let Some(fields) = value.as_object_mut() {
        fields.insert("apiVersion".to_owned(), K::API_VERSION.into());
        fields.insert("kind".to_owned(), K::KIND.into());
    }
    serde_json::from_value(value).map_err(|e| e.to_string())
}

/// The namespace labels of the Pod Security modes.
pub(crate) const POD_SECURITY_MODES: [&str; 3] = [
    PodSecurityLevel::ENFORCE,
    PodSecurityLevel::AUDIT,
    PodSecurityLevel::WARN,
];

/// A namespace with its Pod Security level in every mode [pss], labelled as the platform's. `audit`
/// and `warn` also check workload objects, so a cluster default stricter than the level would flag
/// each one. A project's policy admits the platform's namespaces, so an operator reaches the
/// databases it runs in a project.
///
/// `None` for a namespace Kubernetes keeps for itself, such as `kube-system`: it exists, and its
/// pods need more than a component's level.
///
/// [pss]: https://kubernetes.io/docs/concepts/security/pod-security-admission/#pod-security-admission-labels-for-namespaces
pub(crate) fn namespace(name: &str, level: PodSecurityLevel) -> Option<Namespace> {
    if is_system_namespace(name) {
        return None;
    }
    Some(Namespace {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            labels: Some(
                POD_SECURITY_MODES
                    .into_iter()
                    .map(|mode| (mode.to_owned(), level.as_str().to_owned()))
                    .chain([(PLATFORM_LABEL.to_owned(), "true".to_owned())])
                    .collect(),
            ),
            ..ObjectMeta::default()
        },
        ..Namespace::default()
    })
}

/// A release's values with the profile's merged over them.
fn profile_values(release: &HelmRelease, profile: &str) -> Result<Value, String> {
    let mut values = parse_values(release.values.as_deref())?;
    if let Some(over) = crate::api::profile(profile).and_then(|p| release.profiles.get(p)) {
        merge(&mut values, parse_values(Some(over))?);
    }
    Ok(values)
}

/// What a chart release installs.
pub(crate) struct ChartRelease<'a> {
    /// The Helm release name.
    pub name: &'a str,
    /// The chart's name in `repo`, or an OCI reference.
    pub chart: &'a str,
    /// The HTTP chart repository.
    pub repo: Option<&'a str>,
    /// The chart version.
    pub version: &'a str,
    /// The release's namespace.
    pub namespace: &'a str,
}

/// A chart release as a `HelmChart` with `values`. `failurePolicy: abort` keeps a failed install
/// or upgrade as it is, for a person to see, rather than helm-controller's default of
/// uninstalling and installing again.
pub(crate) fn chart_object(chart: &ChartRelease<'_>, values: &Value) -> Result<HelmChart, String> {
    let values_content = values_content(values)?;
    Ok(HelmChart {
        metadata: ObjectMeta {
            name: Some(chart.name.to_owned()),
            namespace: Some(CHART_NAMESPACE.to_owned()),
            ..ObjectMeta::default()
        },
        spec: HelmChartSpec {
            chart: chart.chart.to_owned(),
            repo: chart.repo.map(str::to_owned),
            version: Some(chart.version.to_owned()),
            target_namespace: Some(chart.namespace.to_owned()),
            failure_policy: Some(FailurePolicy::Abort),
            values_content,
        },
        status: None,
    })
}

/// `values` as YAML; `None` for no values.
fn values_content(values: &Value) -> Result<Option<String>, String> {
    if values.as_object().is_some_and(serde_json::Map::is_empty) {
        return Ok(None);
    }
    yaml_values(values).map(Some)
}

fn yaml_values(values: &Value) -> Result<String, String> {
    serde_saphyr::to_string(values).map_err(|e| e.to_string())
}

fn helm_chart(release: &HelmRelease, values: &Value) -> Result<HelmChart, String> {
    chart_object(
        &ChartRelease {
            name: release.helm_name(),
            chart: &release.chart,
            repo: release.repo.as_deref(),
            version: &release.version,
            namespace: &release.namespace,
        },
        values,
    )
}

/// The objects of a YAML stream.
fn objects_of(text: &str) -> Result<Vec<DynamicObject>, String> {
    let documents: Vec<Value> = serde_saphyr::from_multiple(text).map_err(|e| e.to_string())?;
    let mut objects = Vec::new();
    for document in documents.into_iter().filter(|d| !d.is_null()) {
        let object: DynamicObject = serde_json::from_value(document).map_err(|e| e.to_string())?;
        if object.types.is_none() || object.metadata.name.is_none() {
            return Err("an object has no kind or name".to_owned());
        }
        objects.push(object);
    }
    Ok(objects)
}

/// Labels an object as the release's.
pub(crate) fn label_release(metadata: &mut ObjectMeta, component: &str, release: &str) {
    metadata.labels.get_or_insert_default().extend([
        (MANAGED_BY.to_owned(), MANAGED_BY_ZEABUR.to_owned()),
        (COMPONENT.to_owned(), component.to_owned()),
        (RELEASE.to_owned(), release.to_owned()),
    ]);
}

/// Labels an object as the release's and annotates it with the catalog file it came from.
fn mark(metadata: &mut ObjectMeta, file: &ComponentFile, release: &str, catalog_version: &str) {
    label_release(metadata, &file.id, release);
    metadata.annotations.get_or_insert_default().extend([
        (SOURCE_DIGEST.to_owned(), file.source_digest.clone()),
        (CATALOG_VERSION.to_owned(), catalog_version.to_owned()),
    ]);
}

/// The components with a `HelmChart` Zeabur wrote, as `namespace/name` → component. Each is
/// installed or being installed by Zeabur, whatever detection finds.
#[must_use]
pub fn written(facts: &ClusterFacts) -> BTreeMap<String, String> {
    facts
        .k3s_helm_chart_labels
        .iter()
        .filter(|(_, labels)| {
            labels
                .get(MANAGED_BY)
                .is_some_and(|m| m == MANAGED_BY_ZEABUR)
        })
        .filter_map(|(key, labels)| Some((key.clone(), labels.get(COMPONENT)?.clone())))
        .collect()
}

/// The steps of the component that have not started: its chart releases with no `HelmChart`
/// Zeabur wrote for them.
///
/// A release whose `skipIfPresent` CRDs all exist is left out, as the install leaves it out.
#[must_use]
pub fn unstarted(file: &ComponentFile, facts: &ClusterFacts) -> Vec<String> {
    let started: BTreeSet<&str> = facts
        .k3s_helm_chart_labels
        .values()
        .filter(|labels| {
            labels
                .get(MANAGED_BY)
                .is_some_and(|m| m == MANAGED_BY_ZEABUR)
                && labels.get(COMPONENT).is_some_and(|c| *c == file.id)
        })
        .filter_map(|labels| labels.get(RELEASE).map(String::as_str))
        .collect();
    file.declaration
        .releases
        .iter()
        .filter(|r| r.helm().is_some())
        .filter(|r| {
            let skip = r.skip_if_present();
            skip.is_empty() || !skip.iter().all(|crd| facts.crds.contains(crd))
        })
        .filter_map(Release::name)
        .filter(|name| !started.contains(name))
        .map(str::to_owned)
        .collect()
}

/// The installed components that need `id`, which keep it from being removed. An installed
/// component whose file does not read may need it too, so it also keeps `id`.
#[must_use]
pub fn needed_by<'a>(
    snapshot: &'a Snapshot,
    id: &str,
    installed: &BTreeSet<String>,
) -> Vec<&'a str> {
    snapshot
        .needed_by(id)
        .into_iter()
        .chain(snapshot.problems.keys().map(String::as_str))
        .filter(|c| *c != id && installed.contains(*c))
        .collect()
}

/// How far one step, or one check, has come.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// Not written yet.
    Pending,
    /// Written; the controller works on it.
    Waiting,
    /// Done.
    Ready,
    /// It will not get ready without a change: why.
    Failed(String),
}

/// A K3s/RKE2 `HelmChart`, with the fields Zeabur writes and the status readiness reads
/// [helm-controller-types].
///
/// [helm-controller-types]: https://github.com/k3s-io/helm-controller/blob/v0.16.27/pkg/apis/helm.cattle.io/v1/types.go
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HelmChart {
    /// Its metadata; `managedFields` tells when Zeabur last wrote it.
    #[serde(default)]
    pub metadata: ObjectMeta,
    /// What to install.
    #[serde(default)]
    pub spec: HelmChartSpec,
    /// Its status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<HelmChartStatus>,
}

impl Resource for HelmChart {
    const API_VERSION: &'static str = "helm.cattle.io/v1";
    const GROUP: &'static str = "helm.cattle.io";
    const KIND: &'static str = "HelmChart";
    const VERSION: &'static str = "v1";
    const URL_PATH_SEGMENT: &'static str = "helmcharts";
    type Scope = NamespaceResourceScope;
}

impl Metadata for HelmChart {
    type Ty = ObjectMeta;

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

/// `HelmChart.spec`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelmChartSpec {
    /// The chart's name in `repo`, or an OCI reference.
    #[serde(default)]
    pub chart: String,
    /// The chart repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// The chart version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The namespace of the release.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_namespace: Option<String>,
    /// What helm-controller does when an install or upgrade fails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_policy: Option<FailurePolicy>,
    /// The values, as YAML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values_content: Option<String>,
}

/// `HelmChart.spec.failurePolicy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FailurePolicy {
    /// Uninstall and install again, helm-controller's default.
    Reinstall,
    /// Keep the failed release as it is.
    Abort,
}

/// A K3s/RKE2 `HelmChartConfig`, which changes the values of the `HelmChart` of the same name
/// [helm-controller-types].
///
/// [helm-controller-types]: https://github.com/k3s-io/helm-controller/blob/v0.16.27/pkg/apis/helm.cattle.io/v1/types.go
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HelmChartConfig {
    /// Its metadata; `managedFields` tells when Zeabur last wrote it.
    #[serde(default)]
    pub metadata: ObjectMeta,
    /// The values.
    #[serde(default)]
    pub spec: HelmChartConfigSpec,
}

impl Resource for HelmChartConfig {
    const API_VERSION: &'static str = "helm.cattle.io/v1";
    const GROUP: &'static str = "helm.cattle.io";
    const KIND: &'static str = "HelmChartConfig";
    const VERSION: &'static str = "v1";
    const URL_PATH_SEGMENT: &'static str = "helmchartconfigs";
    type Scope = NamespaceResourceScope;
}

impl Metadata for HelmChartConfig {
    type Ty = ObjectMeta;

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

/// `HelmChartConfig.spec`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelmChartConfigSpec {
    /// The values merged over the chart's, as YAML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values_content: Option<String>,
}

/// `HelmChart.status`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelmChartStatus {
    /// The Job that installs or upgrades it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_name: Option<String>,
    /// `JobCreated` and `Failed`.
    #[serde(default)]
    pub conditions: Vec<Condition>,
}

/// A status condition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Condition {
    /// Its type.
    #[serde(rename = "type")]
    pub kind: String,
    /// `True`, `False` or `Unknown`.
    pub status: String,
    /// Why, in words.
    #[serde(default)]
    pub message: Option<String>,
}

/// When Zeabur last wrote the object.
fn written_at(metadata: &ObjectMeta) -> Option<Timestamp> {
    metadata
        .managed_fields
        .iter()
        .flatten()
        .filter(|m| m.manager.as_deref() == Some(FIELD_MANAGER))
        .filter_map(|m| m.time.as_ref().map(|t| t.0))
        .max()
}

/// Whether a `HelmChart` is installed as last written.
///
/// `job`, the Job its status names, must be created at or after Zeabur's last write and complete,
/// and the `Failed` condition must not be `True`. The condition has no time, so a failure counts
/// only while its Job is not older than the write.
#[must_use]
pub fn chart_progress(chart: &HelmChart, job: Option<&Job>) -> Progress {
    progress_since(written_at(&chart.metadata), chart, job)
}

/// Whether the `HelmChart` a `HelmChartConfig` changes is installed with it: as
/// [`chart_progress`], with the Job created at or after Zeabur's last write of the config.
#[must_use]
pub fn config_progress(config: &ObjectMeta, chart: &HelmChart, job: Option<&Job>) -> Progress {
    progress_since(written_at(config), chart, job)
}

fn progress_since(written: Option<Timestamp>, chart: &HelmChart, job: Option<&Job>) -> Progress {
    let conditions = chart
        .status
        .as_ref()
        .map_or(&[][..], |s| s.conditions.as_slice());
    let current = job.map(|job| {
        let created = job.metadata.creation_timestamp.as_ref().map(|t| t.0);
        match (written, created) {
            (Some(written), Some(created)) => created >= truncate(written),
            (None, Some(_)) => true,
            (_, None) => false,
        }
    });
    if current != Some(false)
        && let Some(failed) = conditions
            .iter()
            .find(|c| c.kind == "Failed" && c.status == "True")
    {
        return Progress::Failed(failed.message.clone().unwrap_or_default());
    }
    let Some(job) = job else {
        return Progress::Waiting;
    };
    if current != Some(true) {
        return Progress::Waiting;
    }
    let state = readiness::job(job);
    match (state.status, state.reason) {
        (Status::Current, Reason::JobComplete) => Progress::Ready,
        (Status::Failed, _) => Progress::Failed(format!(
            "the Helm Job {} failed",
            job.metadata.name.as_deref().unwrap_or_default()
        )),
        _ => Progress::Waiting,
    }
}

/// Kubernetes keeps timestamps to the second, so a Job created in the second of the write
/// counts as after it.
fn truncate(t: Timestamp) -> Timestamp {
    Timestamp::from_second(t.as_second()).unwrap_or(t)
}

/// Whether an inline object of `group` and `kind` is ready, by kstatus.
///
/// The built-in kinds that `zeabur_model::readiness` knows follow the rule of their kind, a CRD
/// follows `Established`, and the rest follow the generic rule. A failure names the object and
/// its reason or condition.
#[must_use]
pub fn object_progress(
    group: &str,
    kind: &str,
    object: Option<&DynamicObject>,
    now: Timestamp,
) -> Progress {
    let Some(object) = object else {
        return Progress::Pending;
    };
    kstatus(group, kind, object, now).unwrap_or(Progress::Waiting)
}

/// The kinds with a rule of their own in `zeabur_model::readiness`, as `(group, kind)`.
const OWN_RULE: &[(&str, &str)] = &[
    ("apps", "Deployment"),
    ("apps", "StatefulSet"),
    ("apps", "DaemonSet"),
    ("", "Pod"),
    ("batch", "Job"),
    ("apiextensions.k8s.io", "CustomResourceDefinition"),
];

/// The kstatus answer for `object`; `None` when it does not parse as its kind.
fn kstatus(group: &str, kind: &str, object: &DynamicObject, now: Timestamp) -> Option<Progress> {
    let name = object.metadata.name.as_deref().unwrap_or_default();
    let answer = |explained: Explained<'_>| progress(kind, name, explained);
    Some(match (group, kind) {
        ("apps", "Deployment") => answer(readiness::deployment(&parsed(object)?).into()),
        ("apps", "StatefulSet") => answer(readiness::stateful_set(&parsed(object)?).into()),
        ("apps", "DaemonSet") => answer(readiness::daemon_set(&parsed(object)?).into()),
        ("", "Pod") => answer(readiness::pod(&parsed(object)?, now).into()),
        ("batch", "Job") => answer(readiness::job(&parsed(object)?).into()),
        ("apiextensions.k8s.io", "CustomResourceDefinition") => {
            answer(readiness::custom_resource_definition(&parsed(object)?))
        }
        _ => answer(readiness::generic(&object.metadata, &status_of(object)?)),
    })
}

fn progress(kind: &str, name: &str, explained: Explained<'_>) -> Progress {
    match explained.readiness.status {
        Status::Current => Progress::Ready,
        Status::InProgress => Progress::Waiting,
        Status::Failed => Progress::Failed(explained.cause.map_or_else(
            || format!("{kind} {name}: {}", explained.readiness.reason),
            |cause| format!("{kind} {name}: {cause}"),
        )),
    }
}

fn parsed<K: serde::de::DeserializeOwned>(object: &DynamicObject) -> Option<K> {
    serde_json::to_value(object)
        .and_then(serde_json::from_value)
        .ok()
}

/// The `status` fields the generic rule reads.
fn status_of(object: &DynamicObject) -> Option<GenericStatus> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        status: GenericStatus,
    }
    serde_json::from_value::<Body>(object.data.clone())
        .ok()
        .map(|body| body.status)
}

/// The served resource of a health check's kind: the one named by its `plural.group`.
#[must_use]
pub fn health_resource(kind: HealthKind, served: &[ServedResource]) -> Option<&ServedResource> {
    let resource = kind.resource();
    let (plural, group) = resource.split_once('.').unwrap_or((resource, ""));
    served
        .iter()
        .find(|r| r.resource == plural && r.group == group)
}

/// Whether a health check passes on `objects` of `resource`, the objects it names.
///
/// Each object of a kind with a rule of its own is Current by kstatus. Each other object passes
/// the generic rule's checks of deletion, generation, `Reconciling` and `Stalled`, and has the
/// check's condition `True`. No object yet is waiting. A failure names the object and its reason
/// or condition.
#[must_use]
pub fn check_progress(
    check: &Health,
    resource: &ServedResource,
    objects: &[DynamicObject],
    now: Timestamp,
) -> Progress {
    if objects.is_empty() {
        return Progress::Waiting;
    }
    let (group, kind) = (resource.group.as_str(), resource.kind.as_str());
    let own_rule = OWN_RULE.contains(&(group, kind));
    let mut waiting = false;
    for object in objects {
        let progress = if own_rule {
            kstatus(group, kind, object, now).unwrap_or(Progress::Waiting)
        } else {
            condition_progress(kind, object, check.condition.as_deref().unwrap_or_default())
        };
        match progress {
            Progress::Ready => {}
            Progress::Failed(why) => return Progress::Failed(why),
            Progress::Pending | Progress::Waiting => waiting = true,
        }
    }
    if waiting {
        Progress::Waiting
    } else {
        Progress::Ready
    }
}

/// `condition` of `object` is `True`, once the generic rule finds the object observed, not
/// reconciling and not stalled.
fn condition_progress(kind: &str, object: &DynamicObject, condition: &str) -> Progress {
    let Some(status) = status_of(object) else {
        return Progress::Waiting;
    };
    let explained = readiness::generic(&object.metadata, &status);
    let reason = explained.readiness.reason;
    if explained.readiness.status == Status::Failed {
        let name = object.metadata.name.as_deref().unwrap_or_default();
        return progress(kind, name, explained);
    }
    let unsettled = [
        Reason::Terminating,
        Reason::LatestGenerationNotObserved,
        Reason::Reconciling,
    ];
    let holds = status
        .conditions
        .iter()
        .any(|c| c.type_ == condition && c.status == "True");
    if holds && !unsettled.contains(&reason) {
        Progress::Ready
    } else {
        Progress::Waiting
    }
}

/// A check's `timeout`, such as `60s` or `5m`, in seconds; 300 when it does not read.
#[must_use]
pub fn timeout_seconds(text: &str) -> i64 {
    let (number, unit) = text.split_at(
        text.find(|c: char| !c.is_ascii_digit())
            .unwrap_or(text.len()),
    );
    let n: i64 = number.parse().unwrap_or(0);
    match unit {
        "s" | "" if n > 0 => n,
        "m" if n > 0 => n * 60,
        "h" if n > 0 => n * 3600,
        _ => 300,
    }
}

/// The selector of a check as `key=value,…`, for a label-selected list.
#[must_use]
pub fn selector(labels: &BTreeMap<String, String>) -> String {
    labels
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;
    use zeabur_catalog_format::component::{Contribution, HealthResource, ReleaseRef};

    use super::*;
    use crate::api;

    const FILES: &[(&str, &str)] = &[
        (
            "cert-manager",
            include_str!("../tests/api/cert-manager.json"),
        ),
        (
            "cloudnative-pg",
            include_str!("../tests/api/cloudnative-pg.json"),
        ),
        ("crowdsec", include_str!("../tests/api/crowdsec.json")),
        ("falco", include_str!("../tests/api/falco.json")),
        ("keda", include_str!("../tests/api/keda.json")),
        (
            "keda-http-add-on",
            include_str!("../tests/api/keda-http-add-on.json"),
        ),
        (
            "system-upgrade-controller",
            include_str!("../tests/api/system-upgrade-controller.json"),
        ),
        (
            "tailscale-operator",
            include_str!("../tests/api/tailscale-operator.json"),
        ),
        ("traefik", include_str!("../tests/api/traefik.json")),
        ("velero", include_str!("../tests/api/velero.json")),
        (
            "victoria-logs",
            include_str!("../tests/api/victoria-logs.json"),
        ),
        (
            "victoria-metrics-operator",
            include_str!("../tests/api/victoria-metrics-operator.json"),
        ),
    ];

    const SUC: &str =
        "https://github.com/rancher/system-upgrade-controller/releases/download/v0.21.1";

    /// Short stand-ins for the files of the manifest releases of `FILES`.
    fn manifests() -> Manifests {
        let crd = |name: &str| {
            format!(
                "apiVersion: apiextensions.k8s.io/v1\nkind: CustomResourceDefinition\nmetadata:\n  name: {name}\n"
            )
        };
        [
            (
                "https://github.com/kubernetes-sigs/gateway-api/releases/download/v1.6.3/standard-install.yaml".to_owned(),
                crd("httproutes.gateway.networking.k8s.io"),
            ),
            (format!("{SUC}/crd.yaml"), crd("plans.upgrade.cattle.io")),
            (
                format!("{SUC}/system-upgrade-controller.yaml"),
                "apiVersion: v1\nkind: Namespace\nmetadata:\n  name: system-upgrade\n---\n\
                 apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: system-upgrade-controller\n  \
                 namespace: system-upgrade\n"
                    .to_owned(),
            ),
        ]
        .into_iter()
        .map(|(url, text)| (url, text.into()))
        .collect()
    }

    /// Zeabur Catalog v0.6.1 with the components of `FILES` only.
    fn snapshot() -> Snapshot {
        let mut index = api::index(include_bytes!("../tests/api/catalog.json")).unwrap();
        index
            .components
            .retain(|c| FILES.iter().any(|(id, _)| *id == c.id));
        let fetched: BTreeMap<String, api::Decoded> = FILES
            .iter()
            .map(|(id, text)| {
                (
                    (*id).to_owned(),
                    api::component(&index, id, text.as_bytes()).unwrap(),
                )
            })
            .collect();
        Snapshot::assemble(index, fetched, None, Timestamp::UNIX_EPOCH).unwrap()
    }

    static NO_VALUES: Values = BTreeMap::new();

    const fn none() -> Inputs<'static> {
        Inputs {
            given: &NO_VALUES,
            kept: &NO_VALUES,
            addresses: None,
        }
    }

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    fn kind(object: &DynamicObject) -> &str {
        object.types.as_ref().map_or("", |t| t.kind.as_str())
    }

    fn label<'a>(object: &'a DynamicObject, key: &str) -> Option<&'a str> {
        object
            .metadata
            .labels
            .as_ref()?
            .get(key)
            .map(String::as_str)
    }

    fn values(items: &[(&str, &str, InputValue)]) -> Values {
        let mut values = Values::new();
        for (component, input, value) in items {
            values
                .entry((*component).to_owned())
                .or_default()
                .insert((*input).to_owned(), value.clone());
        }
        values
    }

    fn text(s: &str) -> InputValue {
        InputValue::Text(s.to_owned())
    }

    fn with_inputs(id: &str, given: &Values, kept: &Values) -> Result<Install, Refusal> {
        let empty = BTreeSet::new();
        let target = Target {
            profile: "single-node",
            distribution: Some("k3s"),
            platform: "linux/amd64",
            kube_version: "v1.36.5+k3s1",
            capabilities: &empty,
            components: &empty,
            written: &empty,
            packaged: &BTreeMap::new(),
            zeabur_namespace: "zeabur",
        };
        install(
            &snapshot(),
            id,
            &target,
            &manifests(),
            Inputs {
                given,
                kept,
                addresses: None,
            },
        )
    }

    fn velero_given() -> Values {
        values(&[
            ("velero", "bucket", text("backups")),
            ("velero", "s3Url", text("http://minio.minio:9000")),
            (
                "velero",
                "credentials",
                text("[default]\naws_access_key_id=a\n"),
            ),
        ])
    }

    /// The objects of an install, by `kind/name`.
    fn objects(install: &Install) -> Vec<String> {
        install
            .steps
            .iter()
            .flat_map(|s| &s.objects)
            .map(|d| {
                format!(
                    "{}/{}",
                    kind(&d.object),
                    d.object.metadata.name.clone().unwrap_or_default()
                )
            })
            .collect()
    }

    fn secret(install: &Install, name: &str) -> BTreeMap<String, String> {
        let object = install
            .steps
            .iter()
            .flat_map(|s| &s.objects)
            .find(|d| {
                kind(&d.object) == "Secret" && d.object.metadata.name.as_deref() == Some(name)
            })
            .unwrap();
        let secret: Secret = object.object.clone().try_parse().unwrap();
        secret
            .data
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| (k, String::from_utf8(v.0).unwrap()))
            .collect()
    }

    fn chart_of(install: &Install) -> HelmChart {
        install
            .steps
            .iter()
            .flat_map(|s| &s.objects)
            .find(|d| kind(&d.object) == HelmChart::KIND)
            .unwrap()
            .object
            .clone()
            .try_parse()
            .unwrap()
    }

    fn state(install: &Install, input: &str) -> InputState {
        install
            .inputs
            .iter()
            .find(|r| r.input.name == input)
            .unwrap()
            .state
    }

    fn addresses(cidr: &str, taken: &[&str]) -> ServiceAddresses {
        ServiceAddresses {
            cidrs: vec![cidr.to_owned()],
            taken: taken.iter().map(|a| a.parse().unwrap()).collect(),
        }
    }

    #[test]
    fn a_free_address_is_the_lowest_of_the_static_band_that_nobody_holds() {
        let k3s = addresses("10.43.0.0/16", &["10.43.0.1", "10.43.0.2", "10.43.0.10"]);
        assert_eq!(
            k3s.free(&BTreeSet::new()),
            Some("10.43.0.3".parse().unwrap())
        );
        let picked = BTreeSet::from(["10.43.0.3".parse().unwrap()]);
        assert_eq!(k3s.free(&picked), Some("10.43.0.4".parse().unwrap()));
        // A /24 keeps 16 addresses after the network address for fixed ClusterIPs.
        let full: Vec<String> = (1..=16).map(|i| format!("10.96.0.{i}")).collect();
        let full: Vec<&str> = full.iter().map(String::as_str).collect();
        assert_eq!(
            addresses("10.96.0.0/24", &full).free(&BTreeSet::new()),
            None
        );
        assert_eq!(
            addresses("fd00:10:96::/112", &["fd00:10:96::1"]).free(&BTreeSet::new()),
            Some("fd00:10:96::2".parse().unwrap())
        );
        assert_eq!(ServiceAddresses::default().free(&BTreeSet::new()), None);
    }

    /// velero with two inputs that take a fixed ClusterIP, as Zeabur Builder's `clusterIp` does.
    fn with_cluster_ips(given: &Values, kept: &Values, at: Option<&ServiceAddresses>) -> Install {
        let mut snapshot = snapshot();
        let velero = snapshot.components.get_mut("velero").unwrap();
        for (name, path) in [
            ("clusterIp", "service.clusterIP"),
            ("otherIp", "other.clusterIP"),
        ] {
            velero.declaration.inputs.push(Input {
                name: name.to_owned(),
                description: String::new(),
                release: None,
                path: Some(path.to_owned()),
                secret: None,
                generate: Some(Generate::ClusterIp),
                required: false,
                default: None,
                test_value: None,
            });
        }
        let empty = BTreeSet::new();
        let cluster = Target {
            profile: "single-node",
            distribution: Some("k3s"),
            platform: "linux/amd64",
            kube_version: "v1.36.5+k3s1",
            capabilities: &empty,
            components: &empty,
            written: &empty,
            packaged: &BTreeMap::new(),
            zeabur_namespace: "zeabur",
        };
        let given = velero_given().into_iter().chain(given.clone()).fold(
            Values::new(),
            |mut all, (component, values)| {
                all.entry(component).or_default().extend(values);
                all
            },
        );
        let inputs = Inputs {
            given: &given,
            kept,
            addresses: at,
        };
        install(&snapshot, "velero", &cluster, &manifests(), inputs).unwrap()
    }

    fn written_value(install: &Install, input: &str) -> Option<InputValue> {
        install
            .inputs
            .iter()
            .find(|r| r.input.name == input)
            .and_then(|r| r.value.clone())
    }

    #[test]
    fn a_cluster_ip_input_takes_a_free_address_that_the_next_install_keeps() {
        let k3s = addresses("10.43.0.0/16", &["10.43.0.1", "10.43.0.10"]);
        let install = with_cluster_ips(&Values::new(), &Values::new(), Some(&k3s));
        assert_eq!(state(&install, "clusterIp"), InputState::Generated);
        assert_eq!(
            written_value(&install, "clusterIp"),
            Some(text("10.43.0.2"))
        );
        // Two inputs never take the same address.
        assert_eq!(written_value(&install, "otherIp"), Some(text("10.43.0.3")));
        let chart = chart_of(&install);
        let chart_values: Value =
            serde_saphyr::from_str(chart.spec.values_content.as_deref().unwrap()).unwrap();
        assert_eq!(chart_values["service"]["clusterIP"], "10.43.0.2");
        let kept: Value =
            serde_json::from_str(&chart.metadata.annotations.unwrap_or_default()[INPUTS]).unwrap();
        assert_eq!(kept["clusterIp"], "10.43.0.2");

        // Once the Service holds it, the kept address stays; a given one wins over both.
        let held = addresses("10.43.0.0/16", &["10.43.0.1", "10.43.0.2", "10.43.0.10"]);
        let kept = values(&[("velero", "clusterIp", text("10.43.0.2"))]);
        let again = with_cluster_ips(&Values::new(), &kept, Some(&held));
        assert_eq!(state(&again, "clusterIp"), InputState::Kept);
        assert_eq!(written_value(&again, "clusterIp"), Some(text("10.43.0.2")));
        let given = values(&[("velero", "clusterIp", text("10.43.0.50"))]);
        let chosen = with_cluster_ips(&given, &kept, Some(&held));
        assert_eq!(
            written_value(&chosen, "clusterIp"),
            Some(text("10.43.0.50"))
        );

        // A given address that no Service holds yet is not generated for another input.
        let given = values(&[("velero", "otherIp", text("10.43.0.2"))]);
        let install = with_cluster_ips(&given, &Values::new(), Some(&k3s));
        assert_eq!(
            written_value(&install, "clusterIp"),
            Some(text("10.43.0.3"))
        );
    }

    #[test]
    fn a_cluster_ip_input_stays_empty_without_the_clusters_addresses() {
        let install = with_cluster_ips(&Values::new(), &Values::new(), None);
        assert_eq!(state(&install, "clusterIp"), InputState::Empty);
        let full = addresses("10.96.0.0/28", &[]);
        let taken: Vec<String> = (1..=14).map(|i| format!("10.96.0.{i}")).collect();
        let full = ServiceAddresses {
            taken: taken.iter().map(|a| a.parse().unwrap()).collect(),
            ..full
        };
        let install = with_cluster_ips(&Values::new(), &Values::new(), Some(&full));
        assert_eq!(state(&install, "clusterIp"), InputState::Empty);
    }

    #[test]
    fn path_inputs_go_into_the_chart_and_secret_inputs_into_secrets_before_it() {
        let install = with_inputs("velero", &velero_given(), &Values::new()).unwrap();
        assert_eq!(install.missing(), None);
        assert_eq!(
            objects(&install),
            [
                "Namespace/velero",
                "Secret/velero-credentials",
                "Secret/velero-ca",
                "Secret/velero-repo-credentials",
                "Secret/velero-backup-key",
                "HelmChart/velero",
            ]
        );

        let chart = chart_of(&install);
        let values: Value =
            serde_saphyr::from_str(chart.spec.values_content.as_deref().unwrap()).unwrap();
        let location = &values["configuration"]["backupStorageLocation"][0];
        assert_eq!(location["bucket"], "backups");
        assert_eq!(location["config"]["s3Url"], "http://minio.minio:9000");
        // The rest of the list item stays as the catalog wrote it.
        assert_eq!(location["provider"], "aws");
        assert_eq!(location["config"]["region"], "us-east-1");
        assert_eq!(values["deployNodeAgent"], false);
        assert_eq!(state(&install, "region"), InputState::Default);

        // Only given and generated values are kept for the next install, not defaults.
        let kept: Value =
            serde_json::from_str(&chart.metadata.annotations.unwrap_or_default()[INPUTS]).unwrap();
        assert_eq!(
            kept,
            json!({"bucket": "backups", "s3Url": "http://minio.minio:9000"})
        );

        assert_eq!(
            secret(&install, "velero-credentials")["cloud"],
            "[default]\naws_access_key_id=a\n"
        );
        let password = &secret(&install, "velero-repo-credentials")["repository-password"];
        assert_eq!(password.len(), 128);
        assert!(password.chars().all(|c| c.is_ascii_hexdigit()));
        let key = &secret(&install, "velero-backup-key")["key"];
        assert_eq!(key.len(), 32);
        assert!(
            key.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        assert_eq!(state(&install, "backupKey"), InputState::Generated);
    }

    #[test]
    fn a_required_input_without_a_value_is_missing_and_refused() {
        let install = with_inputs("velero", &Values::new(), &Values::new()).unwrap();
        assert_eq!(state(&install, "bucket"), InputState::Missing);
        assert_eq!(state(&install, "credentials"), InputState::Missing);
        assert_eq!(
            install.missing(),
            Some(Refusal::Missing {
                component: "velero".to_owned(),
                input: "bucket".to_owned(),
            })
        );
        // An empty text is no value.
        let given = values(&[("velero", "bucket", text(""))]);
        let install = with_inputs("velero", &given, &Values::new()).unwrap();
        assert_eq!(state(&install, "bucket"), InputState::Missing);
    }

    #[test]
    fn kept_values_are_reused_and_a_kept_secret_is_never_generated_again() {
        let kept = values(&[
            ("velero", "bucket", text("old")),
            ("velero", "s3Url", text("http://old:9000")),
            ("velero", "credentials", text("old credentials")),
            ("velero", "repositoryPassword", text("kept password")),
        ]);
        let given = values(&[("velero", "bucket", text("new"))]);
        let install = with_inputs("velero", &given, &kept).unwrap();
        assert_eq!(install.missing(), None);
        assert_eq!(state(&install, "bucket"), InputState::Given);
        assert_eq!(state(&install, "s3Url"), InputState::Kept);
        assert_eq!(state(&install, "repositoryPassword"), InputState::Kept);
        assert_eq!(
            secret(&install, "velero-repo-credentials")["repository-password"],
            "kept password"
        );
        assert_eq!(
            secret(&install, "velero-credentials")["cloud"],
            "old credentials"
        );
        let chart = chart_of(&install);
        let kept: Value =
            serde_json::from_str(&chart.metadata.annotations.unwrap_or_default()[INPUTS]).unwrap();
        assert_eq!(kept, json!({"bucket": "new", "s3Url": "http://old:9000"}));
    }

    #[test]
    fn inputs_that_share_a_secret_are_its_keys() {
        let given = values(&[
            ("tailscale-operator", "oauthClientId", text("id")),
            ("tailscale-operator", "oauthClientSecret", text("secret")),
        ]);
        let install = with_inputs("tailscale-operator", &given, &Values::new()).unwrap();
        assert_eq!(
            objects(&install),
            [
                "Namespace/tailscale",
                "Secret/operator-oauth",
                "HelmChart/tailscale-operator",
            ]
        );
        assert_eq!(
            secret(&install, "operator-oauth"),
            BTreeMap::from([
                ("client_id".to_owned(), "id".to_owned()),
                ("client_secret".to_owned(), "secret".to_owned()),
            ])
        );
        // A chart without path inputs keeps nothing.
        assert!(
            !chart_of(&install)
                .metadata
                .annotations
                .unwrap_or_default()
                .contains_key(INPUTS)
        );
    }

    #[test]
    fn a_value_of_another_kind_is_refused() {
        let mut given = velero_given();
        given
            .get_mut("velero")
            .unwrap()
            .insert("fileBackup".to_owned(), text("yes"));
        assert_eq!(
            with_inputs("velero", &given, &Values::new()),
            Err(Refusal::WrongValue {
                component: "velero".to_owned(),
                input: "fileBackup".to_owned(),
                expected: "true or false",
            })
        );
        given
            .get_mut("velero")
            .unwrap()
            .insert("fileBackup".to_owned(), InputValue::Flag(true));
        let install = with_inputs("velero", &given, &Values::new()).unwrap();
        let values: Value =
            serde_saphyr::from_str(chart_of(&install).spec.values_content.as_deref().unwrap())
                .unwrap();
        assert_eq!(values["deployNodeAgent"], true);
    }

    #[test]
    fn a_removal_writes_no_secrets() {
        let velero = &snapshot().components["velero"];
        let steps = steps_of(
            velero,
            "0.6.1",
            "single-node",
            &Manifests::new(),
            &[],
            &Contributions::default(),
            "zeabur",
        )
        .unwrap();
        assert!(
            steps
                .iter()
                .flat_map(|s| &s.objects)
                .all(|d| kind(&d.object) != "Secret")
        );
        assert_eq!(
            input_secrets(velero),
            [
                "velero/velero-credentials",
                "velero/velero-ca",
                "velero/velero-repo-credentials",
                "velero/velero-backup-key",
            ]
        );
    }

    #[test]
    fn what_zeabur_cannot_install_yet_says_why() {
        let snapshot = snapshot();
        assert_eq!(unsupported(&snapshot.components["crowdsec"]), None);
        assert_eq!(unsupported(&snapshot.components["traefik"]), None);
        assert_eq!(unsupported(&snapshot.components["velero"]), None);
    }

    #[test]
    fn a_chart_or_secret_with_other_content_is_written_again() {
        let install = with_inputs("velero", &velero_given(), &Values::new()).unwrap();
        let desired: Vec<&DynamicObject> = install
            .steps
            .iter()
            .flat_map(|s| &s.objects)
            .map(|d| &d.object)
            .collect();
        let chart = desired.iter().find(|o| kind(o) == HelmChart::KIND).unwrap();
        // The cluster adds fields; what Zeabur writes is still there.
        let mut found = (*chart).clone();
        found.data["spec"]["bootstrap"] = json!(false);
        found.data["status"] = json!({"jobName": "helm-install-velero"});
        assert!(holds(chart, &found));

        let mut other_bucket = velero_given();
        other_bucket
            .get_mut("velero")
            .unwrap()
            .insert("bucket".to_owned(), text("other"));
        let changed = with_inputs("velero", &other_bucket, &Values::new()).unwrap();
        let changed = dynamic(&chart_of(&changed)).unwrap();
        assert!(!holds(&changed, &found));

        // The same spec, with a value that was a default and is now given.
        let mut defaulted = found.clone();
        defaulted
            .metadata
            .annotations
            .get_or_insert_default()
            .insert(INPUTS.to_owned(), "{}".to_owned());
        assert!(!holds(chart, &defaulted));

        let credentials = desired
            .iter()
            .find(|o| o.metadata.name.as_deref() == Some("velero-credentials"))
            .unwrap();
        let mut other = (*credentials).clone();
        other.data["data"]["cloud"] = json!("b3RoZXI=");
        assert!(!holds(credentials, &other));
        assert!(holds(credentials, credentials));
        let namespace = desired.first().unwrap();
        assert!(holds(
            namespace,
            &object(
                json!({"apiVersion": "v1", "kind": "Namespace", "metadata": {"name": "velero"}})
            )
        ));
    }

    fn plan(id: &str, capabilities: &[&str], components: &[&str]) -> Result<Install, Refusal> {
        plan_written(id, capabilities, components, &[])
    }

    fn plan_written(
        id: &str,
        capabilities: &[&str],
        components: &[&str],
        written: &[&str],
    ) -> Result<Install, Refusal> {
        let capabilities = set(capabilities);
        let components = set(components);
        let written = set(written);
        let target = Target {
            profile: "single-node",
            distribution: Some("k3s"),
            platform: "linux/amd64",
            kube_version: "v1.36.5+k3s1",
            capabilities: &capabilities,
            components: &components,
            written: &written,
            packaged: &BTreeMap::new(),
            zeabur_namespace: "zeabur",
        };
        install(&snapshot(), id, &target, &manifests(), none())
    }

    #[test]
    fn cert_manager_installs_its_chart_then_its_issuers() {
        let install = plan("cert-manager", &["ingress"], &[]).unwrap();
        assert_eq!(install.components, ["cert-manager"]);
        let releases: Vec<&str> = install.steps.iter().map(|s| s.release.as_str()).collect();
        assert_eq!(releases, ["cert-manager", "issuers"]);

        let chart = &install.steps[0].objects;
        let namespace = &chart[0].object;
        assert_eq!(kind(namespace), "Namespace");
        assert_eq!(
            label(namespace, PodSecurityLevel::ENFORCE),
            Some("restricted")
        );
        let helm: HelmChart = chart[1].object.clone().try_parse().unwrap();
        assert_eq!(kind(&chart[1].object), HelmChart::KIND);
        assert_eq!(helm.metadata.namespace.as_deref(), Some(CHART_NAMESPACE));
        assert_eq!(
            helm.spec.chart,
            "oci://quay.io/jetstack/charts/cert-manager"
        );
        assert_eq!(helm.spec.version.as_deref(), Some("v1.21.3"));
        assert_eq!(helm.spec.target_namespace.as_deref(), Some("cert-manager"));
        assert_eq!(helm.spec.failure_policy, Some(FailurePolicy::Abort));
        assert_eq!(label(&chart[1].object, COMPONENT), Some("cert-manager"));
        assert_eq!(label(&chart[1].object, MANAGED_BY), Some(MANAGED_BY_ZEABUR));
        assert_eq!(
            helm.metadata.annotations.unwrap_or_default()[CATALOG_VERSION],
            "0.6.1"
        );

        // The single-node values merge over the release's: both are in the chart's values.
        let values: Value =
            serde_saphyr::from_str(helm.spec.values_content.as_deref().unwrap()).unwrap();
        assert_eq!(values["crds"]["keep"], true);
        assert_eq!(values["resources"]["limits"]["memory"], "128Mi");
        assert_eq!(values["webhook"]["resources"]["limits"]["memory"], "64Mi");
        assert!(
            values["webhook"]["image"]["tag"]
                .as_str()
                .unwrap()
                .contains("@sha256:")
        );

        // The issuers' namespace is already written by the step before.
        let issuers: Vec<&str> = install.steps[1]
            .objects
            .iter()
            .map(|d| d.object.metadata.name.as_deref().unwrap())
            .collect();
        assert_eq!(issuers, ["letsencrypt-staging", "letsencrypt"]);
        assert_eq!(
            label(&install.steps[1].objects[0].object, RELEASE),
            Some("issuers")
        );
    }

    #[test]
    fn a_needed_capability_the_cluster_lacks_comes_first() {
        let install = plan("cert-manager", &[], &[]).unwrap();
        assert_eq!(install.components, ["traefik", "cert-manager"]);

        let install = plan("cloudnative-pg", &["ingress"], &[]).unwrap();
        assert_eq!(install.components, ["cert-manager", "cloudnative-pg"]);
    }

    #[test]
    fn a_needed_component_comes_first_unless_installed() {
        let install = plan("keda-http-add-on", &[], &[]).unwrap();
        assert_eq!(install.components, ["keda", "keda-http-add-on"]);
        let install = plan("keda-http-add-on", &[], &["keda"]).unwrap();
        assert_eq!(install.components, ["keda-http-add-on"]);
    }

    #[test]
    fn a_needed_component_zeabur_began_stays_in_the_plan_once_detected() {
        // cert-manager's chart is written and its CRDs make it detected, but its issuers are not
        // written yet.
        let install = plan_written(
            "cloudnative-pg",
            &["ingress", "certificates"],
            &["cert-manager"],
            &["cert-manager"],
        )
        .unwrap();
        assert_eq!(install.components, ["cert-manager", "cloudnative-pg"]);
        let install = plan_written("keda-http-add-on", &[], &["keda"], &["keda"]).unwrap();
        assert_eq!(install.components, ["keda", "keda-http-add-on"]);
    }

    #[test]
    fn an_unknown_component_is_refused() {
        assert!(matches!(
            plan("nothing", &[], &[]),
            Err(Refusal::Unknown(_))
        ));
    }

    #[test]
    fn a_distribution_the_component_does_not_list_is_refused() {
        let capabilities = set(&["ingress"]);
        let components = set(&[]);
        let target = Target {
            profile: "single-node",
            distribution: Some("minikube"),
            platform: "linux/amd64",
            kube_version: "v1.36.5+k3s1",
            capabilities: &capabilities,
            components: &components,
            written: &BTreeSet::new(),
            packaged: &BTreeMap::new(),
            zeabur_namespace: "zeabur",
        };
        let refusal =
            install(&snapshot(), "cert-manager", &target, &manifests(), none()).unwrap_err();
        assert!(matches!(refusal, Refusal::NotHere { place, .. } if place == "minikube"));
    }

    #[test]
    fn a_kubernetes_version_outside_the_components_range_is_refused() {
        let capabilities = set(&["ingress"]);
        let components = set(&[]);
        let target = Target {
            profile: "single-node",
            distribution: Some("k3s"),
            platform: "linux/amd64",
            kube_version: "v1.33.4+k3s1",
            capabilities: &capabilities,
            components: &components,
            written: &BTreeSet::new(),
            packaged: &BTreeMap::new(),
            zeabur_namespace: "zeabur",
        };
        let refusal =
            install(&snapshot(), "victoria-logs", &target, &manifests(), none()).unwrap_err();
        assert!(
            matches!(refusal, Refusal::NotHere { component, place } if component == "victoria-logs" && place == "Kubernetes v1.33.4+k3s1")
        );
        assert!(install(&snapshot(), "cert-manager", &target, &manifests(), none()).is_ok());
    }

    #[test]
    fn a_namespace_kubernetes_keeps_is_not_written() {
        let traefik = &snapshot().components["traefik"];
        let namespaces: Vec<String> = steps_of(
            traefik,
            "0.6.1",
            "single-node",
            &manifests(),
            &[],
            &Contributions::default(),
            "zeabur",
        )
        .unwrap()
        .into_iter()
        .flat_map(|s| s.objects)
        .filter(|d| kind(&d.object) == "Namespace")
        .filter_map(|d| d.object.metadata.name)
        .collect();
        assert_eq!(namespaces, ["traefik"]);
    }

    #[test]
    fn a_manifest_release_writes_the_objects_of_its_files() {
        let install = plan("system-upgrade-controller", &[], &[]).unwrap();
        let steps: Vec<(&str, Vec<&str>)> = install
            .steps
            .iter()
            .map(|s| {
                (
                    s.release.as_str(),
                    s.objects.iter().map(|d| kind(&d.object)).collect(),
                )
            })
            .collect();
        // The controller file's own Namespace is left out: the first step writes it with the
        // component's Pod Security level.
        assert_eq!(
            steps,
            [
                ("crds", vec!["Namespace", "CustomResourceDefinition"]),
                ("controller", vec!["Deployment"]),
            ]
        );
        let namespace = &install.steps[0].objects[0].object;
        assert_eq!(
            label(namespace, PodSecurityLevel::ENFORCE),
            Some("privileged")
        );
        let deployment = &install.steps[1].objects[0];
        assert_eq!(label(&deployment.object, RELEASE), Some("controller"));
        assert_eq!(
            label(&deployment.object, COMPONENT),
            Some("system-upgrade-controller")
        );

        let traefik = plan("traefik", &[], &[]).unwrap();
        assert_eq!(
            traefik.steps[0].skip_if_present,
            ["httproutes.gateway.networking.k8s.io"]
        );
    }

    #[test]
    fn a_manifest_release_without_its_files_is_refused() {
        let file = &snapshot().components["system-upgrade-controller"];
        let refusal = steps_of(
            file,
            "0.5.5",
            "single-node",
            &Manifests::new(),
            &[],
            &Contributions::default(),
            "zeabur",
        )
        .unwrap_err();
        assert!(refusal.to_string().contains(&format!("{SUC}/crd.yaml")));
    }

    #[test]
    fn each_file_of_a_manifest_release_has_the_catalogs_digest() {
        let snapshot = snapshot();
        let file = &snapshot.components["system-upgrade-controller"];
        assert_eq!(
            downloads(file).unwrap(),
            [
                Download {
                    url: format!("{SUC}/crd.yaml"),
                    sha256: "1393ba034e7691273fc519b721a7f72a0cdbef8c7b051c9e408be96090b8478d"
                        .to_owned(),
                },
                Download {
                    url: format!("{SUC}/system-upgrade-controller.yaml"),
                    sha256: "5b46172058b198f8297c9966aacdcd9da2604f740254f0beae74e61b1ad84eac"
                        .to_owned(),
                },
            ]
        );

        let mut unlisted = file.clone();
        if let Release::Manifest(release) = &mut unlisted.declaration.releases[1] {
            release.url_digests.clear();
        }
        let refusal = downloads(&unlisted).unwrap_err();
        assert!(refusal.to_string().contains(&format!(
            "no sha256 digest for {SUC}/system-upgrade-controller.yaml"
        )));
    }

    #[test]
    fn a_download_is_used_only_when_its_digest_matches() {
        let empty = Download {
            url: "https://example.com/empty.yaml".to_owned(),
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_owned(),
        };
        assert_eq!(verify(&empty, Vec::new()).unwrap(), "");
        let changed = verify(&empty, b"kind: Namespace".to_vec()).unwrap_err();
        assert!(
            changed.contains("https://example.com/empty.yaml"),
            "{changed}"
        );
    }

    #[test]
    fn an_inline_only_component_writes_its_objects() {
        let install = plan(
            "victoria-logs",
            &["certificates"],
            &["victoria-metrics-operator"],
        )
        .unwrap();
        assert_eq!(install.components, ["victoria-logs"]);
        let kinds: Vec<&str> = install.steps[0]
            .objects
            .iter()
            .map(|d| kind(&d.object))
            .collect();
        assert_eq!(kinds[0], "Namespace");
        assert!(kinds[1..].iter().all(|k| *k != HelmChart::KIND));
    }

    #[test]
    fn a_component_another_installed_one_needs_is_kept() {
        let snapshot = snapshot();
        let installed = set(&["keda", "keda-http-add-on"]);
        assert_eq!(
            needed_by(&snapshot, "keda", &installed),
            ["keda-http-add-on"]
        );
        assert_eq!(
            needed_by(&snapshot, "keda", &set(&["keda"])),
            Vec::<&str>::new()
        );
        // Needed through its capability.
        let installed = set(&["cert-manager", "cloudnative-pg"]);
        assert_eq!(
            needed_by(&snapshot, "cert-manager", &installed),
            ["cloudnative-pg"]
        );
        // An installed component whose file does not read may need it.
        let mut unread = snapshot;
        let file = unread.components.remove("keda-http-add-on").unwrap();
        unread.problems.insert(file.id, "too new".to_owned());
        assert_eq!(
            needed_by(&unread, "keda", &set(&["keda", "keda-http-add-on"])),
            ["keda-http-add-on"]
        );
    }

    fn chart(written: &str, conditions: &Value) -> HelmChart {
        serde_json::from_value(json!({
            "metadata": {
                "name": "cert-manager",
                "managedFields": [
                    {"manager": "zeabur", "operation": "Apply", "time": written},
                    {"manager": "helm-controller", "operation": "Update", "time": "2030-01-01T00:00:00Z"},
                ],
            },
            "status": {"jobName": "helm-install-cert-manager", "conditions": conditions},
        }))
        .unwrap()
    }

    fn job(created: &str, conditions: &Value) -> Job {
        serde_json::from_value(json!({
            "metadata": {"name": "helm-install-cert-manager", "creationTimestamp": created},
            "status": {"conditions": conditions},
        }))
        .unwrap()
    }

    fn complete() -> Value {
        json!([
            {"type": "SuccessCriteriaMet", "status": "True"},
            {"type": "Complete", "status": "True"},
        ])
    }

    #[test]
    fn a_chart_is_ready_when_the_job_after_the_last_write_completed() {
        let written = "2026-11-19T10:00:00.400Z";
        let c = chart(written, &json!([]));
        assert_eq!(chart_progress(&c, None), Progress::Waiting);
        // The Job of the write before.
        let old = job("2026-11-19T09:00:00Z", &complete());
        assert_eq!(chart_progress(&c, Some(&old)), Progress::Waiting);
        // Created in the same second as the write.
        let now = job("2026-11-19T10:00:00Z", &complete());
        assert_eq!(chart_progress(&c, Some(&now)), Progress::Ready);
        let running = job("2026-11-19T10:00:01Z", &json!([]));
        assert_eq!(chart_progress(&c, Some(&running)), Progress::Waiting);
    }

    #[test]
    fn a_failed_chart_or_job_is_a_failure() {
        let failed = chart(
            "2026-11-19T10:00:00Z",
            &json!([{"type": "Failed", "status": "True", "message": "chart not found"}]),
        );
        assert_eq!(
            chart_progress(&failed, None),
            Progress::Failed("chart not found".to_owned())
        );

        let c = chart("2026-11-19T10:00:00Z", &json!([]));
        let j = job(
            "2026-11-19T10:00:02Z",
            &json!([{"type": "Failed", "status": "True", "reason": "BackoffLimitExceeded"}]),
        );
        assert!(matches!(chart_progress(&c, Some(&j)), Progress::Failed(_)));
    }

    #[test]
    fn a_failure_of_the_job_before_the_last_write_waits_for_the_next_job() {
        let failed = chart(
            "2026-11-19T10:00:00Z",
            &json!([{"type": "Failed", "status": "True", "message": "the old values"}]),
        );
        let before = job(
            "2026-11-19T09:59:00Z",
            &json!([{"type": "Failed", "status": "True", "reason": "BackoffLimitExceeded"}]),
        );
        assert_eq!(chart_progress(&failed, Some(&before)), Progress::Waiting);
        let after = job(
            "2026-11-19T10:00:01Z",
            &json!([{"type": "Failed", "status": "True", "reason": "BackoffLimitExceeded"}]),
        );
        assert!(matches!(
            chart_progress(&failed, Some(&after)),
            Progress::Failed(_)
        ));
    }

    fn object(v: Value) -> DynamicObject {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn an_inline_object_is_ready_by_the_kstatus_rule_of_its_kind() {
        let now = Timestamp::from_second(2_000_000_000).unwrap();
        let progress = |group, kind, v: Value| object_progress(group, kind, Some(&object(v)), now);
        assert_eq!(
            object_progress("", "ConfigMap", None, now),
            Progress::Pending
        );
        let plain = json!({"kind": "ConfigMap", "apiVersion": "v1", "metadata": {}});
        let issuer = |status: &str| {
            json!({"metadata": {"name": "letsencrypt", "generation": 1},
                "status": {"conditions": [{"type": "Ready", "status": status}]}})
        };
        let unobserved = json!({"metadata": {"name": "letsencrypt", "generation": 2},
            "status": {"observedGeneration": 1,
                "conditions": [{"type": "Ready", "status": "True"}]}});
        let crd = |conditions: Value| {
            json!({"metadata": {"name": "issuers.cert-manager.io"},
                "spec": {"group": "cert-manager.io", "scope": "Namespaced",
                    "names": {"plural": "issuers", "kind": "Issuer"}, "versions": []},
                "status": {"conditions": conditions}})
        };
        let fresh_deployment = json!({"metadata": {"name": "webhook", "generation": 1},
            "spec": {"replicas": 1, "selector": {}, "template": {}}});
        let cases = [
            ("a ConfigMap", "", "ConfigMap", plain, Progress::Ready),
            (
                "not ready",
                "cert-manager.io",
                "Issuer",
                issuer("False"),
                Progress::Waiting,
            ),
            (
                "ready",
                "cert-manager.io",
                "Issuer",
                issuer("True"),
                Progress::Ready,
            ),
            (
                "an older generation",
                "cert-manager.io",
                "Issuer",
                unobserved,
                Progress::Waiting,
            ),
            (
                "a CRD not established",
                "apiextensions.k8s.io",
                "CustomResourceDefinition",
                crd(json!([])),
                Progress::Waiting,
            ),
            (
                "an established CRD",
                "apiextensions.k8s.io",
                "CustomResourceDefinition",
                crd(json!([{"type": "Established", "status": "True"}])),
                Progress::Ready,
            ),
            (
                "a Deployment with no replica yet",
                "apps",
                "Deployment",
                fresh_deployment,
                Progress::Waiting,
            ),
        ];
        for (name, group, kind, v, expected) in cases {
            assert_eq!(progress(group, kind, v), expected, "{name}");
        }
    }

    #[test]
    fn a_failed_object_says_its_reason_and_condition() {
        let now = Timestamp::from_second(2_000_000_000).unwrap();
        let stalled = object(
            json!({"metadata": {"name": "pg"}, "status": {"conditions": [
            {"type": "Stalled", "status": "True", "reason": "BadSpec",
             "message": "no such storage class"}]}}),
        );
        assert_eq!(
            object_progress("postgresql.cnpg.io", "Cluster", Some(&stalled), now),
            Progress::Failed("Cluster pg: Stalled: BadSpec: no such storage class".to_owned())
        );
        let past_deadline = object(json!({"metadata": {"name": "webhook"},
            "spec": {"selector": {}, "template": {}, "progressDeadlineSeconds": 600},
            "status": {"conditions": [{"type": "Progressing", "status": "False",
                "reason": "ProgressDeadlineExceeded"}]}}));
        assert_eq!(
            object_progress("apps", "Deployment", Some(&past_deadline), now),
            Progress::Failed("Deployment webhook: ProgressDeadlineExceeded".to_owned())
        );
    }

    fn served(group: &str, kind: &str) -> ServedResource {
        let lower = kind.to_ascii_lowercase();
        ServedResource {
            group: group.to_owned(),
            version: "v1".to_owned(),
            resource: if lower.ends_with('s') {
                format!("{lower}es")
            } else {
                format!("{lower}s")
            },
            kind: kind.to_owned(),
            namespaced: true,
            verbs: Vec::new(),
        }
    }

    #[test]
    fn a_health_check_kind_is_found_in_discovery() {
        let all = [
            served("", "Node"),
            served("", "Pod"),
            served("apps", "Deployment"),
            served("longhorn.io", "Node"),
            served("gateway.networking.k8s.io", "GatewayClass"),
        ];
        let found = |kind| health_resource(kind, &all).map(|r| r.group.as_str());
        assert_eq!(found(HealthKind::Deployment), Some("apps"));
        assert_eq!(found(HealthKind::Pod), Some(""));
        assert_eq!(found(HealthKind::LonghornNode), Some("longhorn.io"));
        assert_eq!(
            found(HealthKind::GatewayClass),
            Some("gateway.networking.k8s.io")
        );
        assert_eq!(found(HealthKind::ApiService), None);
    }

    #[test]
    fn a_health_check_reads_its_workloads_by_kstatus() {
        let file = &snapshot().components["cert-manager"];
        let check = &file.declaration.health[0];
        let resource = served("apps", "Deployment");
        let now = Timestamp::from_second(2_000_000_000).unwrap();
        assert_eq!(
            check_progress(check, &resource, &[], now),
            Progress::Waiting
        );

        let deployment = |available: i32| {
            object(json!({
                "apiVersion": "apps/v1",
                "kind": "Deployment",
                "metadata": {"name": "cert-manager", "generation": 1},
                "spec": {"replicas": 1, "selector": {}, "template": {}},
                "status": {
                    "observedGeneration": 1,
                    "replicas": 1,
                    "updatedReplicas": 1,
                    "readyReplicas": available,
                    "availableReplicas": available,
                    "conditions": [{"type": "Available", "status": if available > 0 { "True" } else { "False" }}],
                },
            }))
        };
        assert_eq!(
            check_progress(check, &resource, &[deployment(0)], now),
            Progress::Waiting
        );
        assert_eq!(
            check_progress(check, &resource, &[deployment(1)], now),
            Progress::Ready
        );
    }

    #[test]
    fn a_health_check_condition_counts_once_the_generation_is_observed() {
        let check = Health {
            name: "nodes-ready".to_owned(),
            resource: HealthResource {
                kind: HealthKind::LonghornNode,
                namespace: Some("longhorn-system".to_owned()),
                name: None,
                selector: None,
                all: true,
            },
            condition: Some("Ready".to_owned()),
            timeout: "60s".to_owned(),
            advice: String::new(),
        };
        let resource = served("longhorn.io", "Node");
        let now = Timestamp::from_second(2_000_000_000).unwrap();
        let node = |observed: i64| {
            object(json!({"metadata": {"name": "n1", "generation": 2},
                "status": {"observedGeneration": observed,
                    "conditions": [{"type": "Ready", "status": "True"}]}}))
        };
        assert_eq!(
            check_progress(&check, &resource, &[node(1)], now),
            Progress::Waiting
        );
        assert_eq!(
            check_progress(&check, &resource, &[node(2)], now),
            Progress::Ready
        );
    }

    #[test]
    fn a_check_timeout_reads_seconds_and_minutes() {
        assert_eq!(timeout_seconds("60s"), 60);
        assert_eq!(timeout_seconds("5m"), 300);
        assert_eq!(timeout_seconds("soon"), 300);
    }

    /// The test snapshot where `from` adds `values` to keda's release.
    fn contributing(snapshot: &mut Snapshot, from: &str, values: &str) {
        let file = snapshot.components.get_mut(from).unwrap();
        file.declaration.contributes.push(Contribution {
            target: ReleaseRef {
                component: "keda".to_owned(),
                release: "keda".to_owned(),
            },
            values: values.to_owned(),
        });
    }

    struct Cluster {
        components: BTreeSet<String>,
        written: BTreeSet<String>,
        packaged: BTreeMap<String, String>,
        capabilities: BTreeSet<String>,
    }

    impl Cluster {
        fn new(components: &[&str], written: &[&str]) -> Self {
            Self {
                components: set(components),
                written: set(written),
                packaged: BTreeMap::new(),
                capabilities: set(&["ingress", "certificates"]),
            }
        }

        fn target(&self) -> Target<'_> {
            Target {
                profile: "single-node",
                distribution: Some("k3s"),
                platform: "linux/amd64",
                kube_version: "v1.36.5+k3s1",
                capabilities: &self.capabilities,
                components: &self.components,
                written: &self.written,
                packaged: &self.packaged,
                zeabur_namespace: "zeabur",
            }
        }
    }

    fn values_of(step: &Step) -> Value {
        let content = step
            .objects
            .iter()
            .find_map(|d| d.object.data["spec"]["valuesContent"].as_str())
            .unwrap_or("{}");
        serde_saphyr::from_str(content).unwrap()
    }

    #[test]
    fn a_contributor_without_a_helm_chart_is_refused() {
        let mut snapshot = snapshot();
        contributing(&mut snapshot, "system-upgrade-controller", "a: 1");
        let cluster = Cluster::new(&[], &[]);
        let refusal = install(
            &snapshot,
            "system-upgrade-controller",
            &cluster.target(),
            &manifests(),
            none(),
        )
        .unwrap_err();
        assert!(
            refusal.to_string().contains("has no Helm chart"),
            "{refusal}"
        );
    }

    #[test]
    fn contributions_merge_over_the_release_values_in_id_order() {
        let mut snapshot = snapshot();
        contributing(&mut snapshot, "keda-http-add-on", "a: http\nonly: http");
        contributing(&mut snapshot, "cloudnative-pg", "a: cnpg");
        let cluster = Cluster::new(&[], &["keda-http-add-on", "cloudnative-pg"]);
        let install = install(&snapshot, "keda", &cluster.target(), &manifests(), none()).unwrap();
        let values = values_of(&install.steps[0]);
        // keda-http-add-on comes after cloudnative-pg by id, so its value wins.
        assert_eq!(values["a"], "http");
        assert_eq!(values["only"], "http");
        let own = values_of(
            &steps_of(
                &snapshot.components["keda"],
                "0.6.1",
                "single-node",
                &manifests(),
                &[],
                &Contributions::default(),
                "zeabur",
            )
            .unwrap()[0],
        );
        for (key, value) in own.as_object().unwrap() {
            assert_eq!(&values[key], value, "{key}");
        }
    }

    #[test]
    fn a_rewritten_target_keeps_its_input_values() {
        let mut snapshot = snapshot();
        snapshot
            .components
            .get_mut("keda")
            .unwrap()
            .declaration
            .contributes
            .push(Contribution {
                target: ReleaseRef {
                    component: "velero".to_owned(),
                    release: "velero".to_owned(),
                },
                values: "added: keda".to_owned(),
            });
        let cluster = Cluster::new(&["velero"], &["velero"]);
        let kept = values(&[
            ("velero", "bucket", text("old")),
            ("velero", "credentials", text("old credentials")),
        ]);
        let inputs = Inputs {
            given: &NO_VALUES,
            kept: &kept,
            addresses: None,
        };
        let install = install(&snapshot, "keda", &cluster.target(), &manifests(), inputs).unwrap();
        let velero = install.steps.last().unwrap();
        assert_eq!(velero.component, "velero");
        assert_eq!(
            velero
                .objects
                .iter()
                .map(|d| kind(&d.object))
                .collect::<Vec<_>>(),
            ["HelmChart"]
        );
        let values = values_of(velero);
        assert_eq!(values["added"], "keda");
        assert_eq!(
            values["configuration"]["backupStorageLocation"][0]["bucket"],
            "old"
        );
        assert!(install.inputs.iter().all(|r| r.component != "velero"));
    }

    #[test]
    fn a_target_installed_with_its_contributor_gets_the_values_after_the_contributor() {
        let install = plan("crowdsec", &["certificates"], &[]).unwrap();
        let steps: Vec<(&str, &str)> = install
            .steps
            .iter()
            .map(|s| (s.component.as_str(), s.release.as_str()))
            .collect();
        let traefik = |s: &Step| s.component == "traefik";
        let first = install.steps.iter().position(traefik).unwrap();
        let last = install.steps.iter().rposition(traefik).unwrap();
        let middleware = install
            .steps
            .iter()
            .position(|s| s.release == "bouncer-middleware")
            .unwrap();
        // Traefik does not use the bouncer before CrowdSec's middleware is there.
        assert!(first < middleware && middleware < last, "{steps:?}");
        assert!(
            values_of(&install.steps[first])["experimental"]["plugins"]
                .get("crowdsec-bouncer")
                .is_none()
        );
        assert!(
            values_of(&install.steps[last])["experimental"]["plugins"]["crowdsec-bouncer"]
                .is_object()
        );
    }

    #[test]
    fn installing_a_contributor_writes_the_chart_zeabur_installed_again() {
        let mut snapshot = snapshot();
        contributing(&mut snapshot, "keda-http-add-on", "a: http");
        let cluster = Cluster::new(&["keda"], &["keda"]);
        let install = install(
            &snapshot,
            "keda-http-add-on",
            &cluster.target(),
            &manifests(),
            none(),
        )
        .unwrap();
        let steps: Vec<(&str, &str)> = install
            .steps
            .iter()
            .map(|s| (s.component.as_str(), s.release.as_str()))
            .collect();
        // keda-http-add-on needs keda, so keda comes first without the value, and is written
        // again with it once the add-on's objects are there.
        assert_eq!(
            steps,
            [
                ("keda", "keda"),
                ("keda-http-add-on", "http-add-on"),
                ("keda", "keda")
            ]
        );
        assert!(values_of(&install.steps[0]).get("a").is_none());
        assert_eq!(values_of(&install.steps[2])["a"], "http");

        // cloudnative-pg does not need keda: its own steps, then keda's chart again.
        contributing(&mut snapshot, "cloudnative-pg", "b: cnpg");
        let install = super::install(
            &snapshot,
            "cloudnative-pg",
            &cluster.target(),
            &manifests(),
            none(),
        )
        .unwrap();
        let last = install.steps.last().unwrap();
        assert_eq!(
            (last.component.as_str(), last.release.as_str()),
            ("keda", "keda")
        );
        assert_eq!(last.objects.len(), 1);
        assert_eq!(kind(&last.objects[0].object), HelmChart::KIND);
        assert_eq!(values_of(last)["b"], "cnpg");
    }

    #[test]
    fn a_packaged_target_gets_a_helm_chart_config() {
        let mut snapshot = snapshot();
        contributing(&mut snapshot, "cloudnative-pg", "b: cnpg");
        let mut cluster = Cluster::new(&["keda"], &[]);
        cluster
            .packaged
            .insert("keda".to_owned(), "rke2-keda".to_owned());
        let install = install(
            &snapshot,
            "cloudnative-pg",
            &cluster.target(),
            &manifests(),
            none(),
        )
        .unwrap();
        let last = install.steps.last().unwrap();
        let config: HelmChartConfig = last.objects[0].object.clone().try_parse().unwrap();
        assert_eq!(config.metadata.name.as_deref(), Some("rke2-keda"));
        assert_eq!(config.metadata.namespace.as_deref(), Some(CHART_NAMESPACE));
        assert_eq!(label(&last.objects[0].object, COMPONENT), Some("keda"));
        assert_eq!(
            label(&last.objects[0].object, MANAGED_BY),
            Some(MANAGED_BY_ZEABUR)
        );
        // Only the contributions: the packaged chart keeps the distribution's own values.
        assert_eq!(values_of(last), serde_json::json!({"b": "cnpg"}));

        let without = without(
            &snapshot,
            "cloudnative-pg",
            &cluster.target(),
            &Values::new(),
        )
        .unwrap();
        let config: HelmChartConfig = without[0].objects[0].object.clone().try_parse().unwrap();
        // An empty config stays, so the removal waits for the chart without the values.
        assert_eq!(config.spec.values_content.as_deref(), Some(""));
    }

    #[test]
    fn a_contribution_to_a_component_not_installed_waits() {
        let mut snapshot = snapshot();
        contributing(&mut snapshot, "cloudnative-pg", "b: cnpg");
        let cluster = Cluster::new(&[], &[]);
        let install = install(
            &snapshot,
            "cloudnative-pg",
            &cluster.target(),
            &manifests(),
            none(),
        )
        .unwrap();
        assert!(install.steps.iter().all(|s| s.component != "keda"));
        assert!(
            install
                .warnings
                .iter()
                .all(|w| !matches!(w, Warning::Unchanged { .. }))
        );

        // Installing keda later picks it up.
        let cluster = Cluster::new(&["cloudnative-pg"], &["cloudnative-pg"]);
        let install =
            super::install(&snapshot, "keda", &cluster.target(), &manifests(), none()).unwrap();
        assert_eq!(values_of(&install.steps[0])["b"], "cnpg");
    }

    #[test]
    fn a_target_another_tool_installed_is_left_with_a_warning() {
        let mut snapshot = snapshot();
        contributing(&mut snapshot, "cloudnative-pg", "b: cnpg");
        let cluster = Cluster::new(&["keda"], &[]);
        let install = install(
            &snapshot,
            "cloudnative-pg",
            &cluster.target(),
            &manifests(),
            none(),
        )
        .unwrap();
        assert!(install.steps.iter().all(|s| s.component != "keda"));
        assert!(install.warnings.contains(&Warning::Unchanged {
            component: "keda".to_owned(),
            values: "b: cnpg\n".to_owned(),
        }));
    }

    #[test]
    fn removing_a_contributor_writes_its_target_without_it() {
        let mut snapshot = snapshot();
        contributing(&mut snapshot, "cloudnative-pg", "b: cnpg");
        contributing(&mut snapshot, "keda-http-add-on", "a: http");
        let cluster = Cluster::new(
            &["keda", "keda-http-add-on", "cloudnative-pg"],
            &["keda", "keda-http-add-on", "cloudnative-pg"],
        );
        let steps = without(
            &snapshot,
            "cloudnative-pg",
            &cluster.target(),
            &Values::new(),
        )
        .unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(kind(&steps[0].objects[0].object), HelmChart::KIND);
        let values = values_of(&steps[0]);
        assert_eq!(values["a"], "http");
        assert!(values.get("b").is_none());
    }
}
