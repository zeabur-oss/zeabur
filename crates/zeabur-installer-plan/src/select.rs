//! Choosing components: a preset, then `enable` and `disable`, then dependencies.
//!
//! A preset is only a choice of capabilities; the catalog decides which component fills each one,
//! so a new catalog release changes `standard` without an installer release. The install order is
//! the union of each chosen component's `installOrder`, taken in id order: every list already
//! places a component after its dependencies, so the union does too, and the result never depends
//! on the order of the configuration file.

use std::collections::{BTreeMap, BTreeSet};

use crate::PlanError;
use crate::catalog::{Default, Index};
use crate::config::{Config, PersonalConfig, Preset};

/// The distribution of a personal install.
pub const PERSONAL_DISTRIBUTION: &str = "kubesolo";

/// The profile whose values a personal install uses.
pub const PERSONAL_PROFILE: &str = "single-node";

/// What a personal install offers: the capabilities the catalog tests on `KubeSolo` (catalog
/// `config/kubernetes.yaml`), less `databases`.
///
/// `KubeSolo`'s webhook replaces the annotations of
/// every PVC, which stops `CloudNativePG`, and removing PVCs from the webhook's rules leaves
/// volumes unbound.
pub const PERSONAL: &[&str] = &["ingress", "certificates", "builds"];

/// The capability of Zeabur Builder, which brings zot. A personal install offers it, but only on
/// a host with enough memory.
pub const BUILD_CAPABILITIES: &[&str] = &["builds"];

/// What a personal install installs unless told otherwise.
const PERSONAL_DEFAULT: &[&str] = &["ingress", "certificates"];

/// Components `minimal` installs: reachable over HTTPS, and usage numbers for Zeabur.
const MINIMAL: &[&str] = &["ingress", "certificates", "metrics/metrics-server"];

/// What `standard` adds to the catalog's recommended components, so that a team can deploy from
/// images and Git and make PostgreSQL databases. Zeabur Builder brings zot.
const STANDARD: &[&str] = &["builds", "image-updates", "databases/cloudnative-pg"];

/// What `secure` adds to `standard`. Backups are left out until the installer can configure an
/// encrypted repository: an unencrypted backup undoes disk encryption.
const SECURE: &[&str] = &[
    "runtime-security",
    "vulnerability-scanning",
    "intrusion-prevention",
];

/// The chosen components, in install order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// Component ids and why each is installed, in install order.
    pub components: Vec<(String, Why)>,
    /// Components the preset or configuration named that this installer leaves out, with the
    /// reason.
    pub left_out: BTreeMap<String, String>,
}

/// Why a component is installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// Part of the preset.
    Preset,
    /// Named in `enable`.
    Enabled,
    /// Another chosen component needs it.
    Dependency,
}

/// Chooses the components for a configuration.
///
/// # Errors
///
/// When a name matches nothing, or a chosen component needs one that is disabled or unsupported.
pub fn select(index: &Index, config: &Config) -> Result<Selection, PlanError> {
    let profile = config.profile.as_str();
    let preset: Vec<String> = match config.preset {
        Preset::Minimal => resolve_all(index, profile, MINIMAL)?,
        Preset::Standard => standard(index, profile)?,
        Preset::Secure => {
            let mut ids = standard(index, profile)?;
            ids.extend(resolve_all(index, profile, SECURE)?);
            ids
        }
    };
    choose(
        index,
        &Wanted {
            base: preset,
            enable: &config.enable,
            disable: &config.disable,
            profile,
            distribution: config.distribution.id(),
            offered: None,
        },
    )
}

/// Chooses the components of a personal install: ingress and certificates, then `enable` and
/// `disable` within [`PERSONAL`].
///
/// # Errors
///
/// As [`select`].
pub fn select_personal(index: &Index, config: &PersonalConfig) -> Result<Selection, PlanError> {
    choose(
        index,
        &Wanted {
            base: resolve_all(index, PERSONAL_PROFILE, PERSONAL_DEFAULT)?,
            enable: &config.enable,
            disable: &config.disable,
            profile: PERSONAL_PROFILE,
            distribution: PERSONAL_DISTRIBUTION,
            offered: Some(PERSONAL),
        },
    )
}

struct Wanted<'a> {
    base: Vec<String>,
    enable: &'a [String],
    disable: &'a [String],
    profile: &'a str,
    distribution: &'a str,
    /// Capabilities outside this list are left out of a personal install.
    offered: Option<&'a [&'a str]>,
}

fn choose(index: &Index, wanted: &Wanted<'_>) -> Result<Selection, PlanError> {
    let profile = wanted.profile;
    let mut chosen: BTreeMap<String, Why> = BTreeMap::new();
    let mut left_out = BTreeMap::new();
    for id in &wanted.base {
        chosen.insert(id.clone(), Why::Preset);
    }
    for name in wanted.enable {
        chosen.insert(resolve(index, profile, name)?, Why::Enabled);
    }
    let mut disabled = BTreeSet::new();
    for name in wanted.disable {
        let matched: Vec<String> = chosen
            .keys()
            .filter(|id| matches(index, id, name))
            .cloned()
            .collect();
        if matched.is_empty() && !index.components.iter().any(|c| matches(index, &c.id, name)) {
            return Err(PlanError::Unknown(name.clone()));
        }
        for id in matched {
            chosen.remove(&id);
            disabled.insert(id);
        }
    }

    let unusable: Vec<String> = chosen
        .keys()
        .filter_map(|id| {
            let component = index.component(id)?;
            if wanted
                .offered
                .is_some_and(|offered| !offered.contains(&component.capability.as_str()))
            {
                left_out.insert(id.clone(), "not part of a personal install".to_owned());
            } else if !component.supports(wanted.distribution) {
                left_out.insert(
                    id.clone(),
                    format!("runs on {} only", component.distributions.join(", ")),
                );
            } else if component.default_for(profile) == Default::Unsupported {
                left_out.insert(id.clone(), format!("unsupported on {profile}"));
            } else {
                return None;
            }
            Some(id.clone())
        })
        .collect();
    for id in &unusable {
        chosen.remove(id);
    }

    let mut order: Vec<(String, Why)> = Vec::new();
    for (id, why) in &chosen {
        let component = index
            .component(id)
            .ok_or_else(|| PlanError::Unknown(id.clone()))?;
        let chain = component
            .install_order
            .get(profile)
            .cloned()
            .unwrap_or_else(|| vec![id.clone()]);
        for dep in chain {
            if order.iter().any(|(seen, _)| *seen == dep) {
                continue;
            }
            if disabled.contains(&dep) || left_out.contains_key(&dep) {
                return Err(PlanError::NeedsDisabled {
                    component: id.clone(),
                    dependency: dep,
                });
            }
            let why = if dep == *id {
                *why
            } else {
                chosen.get(&dep).copied().unwrap_or(Why::Dependency)
            };
            order.push((dep, why));
        }
    }
    Ok(Selection {
        components: order,
        left_out,
    })
}

fn standard(index: &Index, profile: &str) -> Result<Vec<String>, PlanError> {
    let mut ids = recommended(index, profile);
    ids.extend(resolve_all(index, profile, STANDARD)?);
    Ok(ids)
}

fn recommended(index: &Index, profile: &str) -> Vec<String> {
    index
        .components
        .iter()
        .filter(|c| c.default_for(profile) == Default::Recommended)
        .map(|c| c.id.clone())
        .collect()
}

fn resolve_all(index: &Index, profile: &str, names: &[&str]) -> Result<Vec<String>, PlanError> {
    names
        .iter()
        .map(|name| resolve(index, profile, name))
        .collect()
}

/// A component id, or the provider of a capability: its recommended component on the profile,
/// else its first optional one (the catalog's own rule for a bare `requires`).
fn resolve(index: &Index, profile: &str, name: &str) -> Result<String, PlanError> {
    if name.contains('/') {
        return index
            .component(name)
            .map(|c| c.id.clone())
            .ok_or_else(|| PlanError::Unknown(name.to_owned()));
    }
    let providers: Vec<_> = index
        .components
        .iter()
        .filter(|c| c.capability == name)
        .collect();
    providers
        .iter()
        .find(|c| c.default_for(profile) == Default::Recommended)
        .or_else(|| {
            providers
                .iter()
                .find(|c| c.default_for(profile) == Default::Optional)
        })
        .map(|c| c.id.clone())
        .ok_or_else(|| PlanError::Unknown(name.to_owned()))
}

fn matches(index: &Index, id: &str, name: &str) -> bool {
    id == name || index.component(id).is_some_and(|c| c.capability == name)
}
