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
use crate::config::{Config, Preset};

/// The distribution this installer sets up.
pub const DISTRIBUTION: &str = "rke2";

/// Components `minimal` installs: reachable over HTTPS, and usage numbers for Zeabur.
const MINIMAL: &[&str] = &["ingress", "certificates", "metrics/metrics-server"];

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
    let mut chosen: BTreeMap<String, Why> = BTreeMap::new();
    let mut left_out = BTreeMap::new();

    let preset: Vec<String> = match config.preset {
        Preset::Minimal => resolve_all(index, profile, MINIMAL)?,
        Preset::Standard => recommended(index, profile),
        Preset::Secure => {
            let mut ids = recommended(index, profile);
            ids.extend(resolve_all(index, profile, SECURE)?);
            ids
        }
    };
    for id in preset {
        chosen.insert(id, Why::Preset);
    }
    for name in &config.enable {
        chosen.insert(resolve(index, profile, name)?, Why::Enabled);
    }
    let mut disabled = BTreeSet::new();
    for name in &config.disable {
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
            if !component.supports(DISTRIBUTION) {
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
