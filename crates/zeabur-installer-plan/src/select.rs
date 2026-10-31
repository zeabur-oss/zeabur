//! Choosing components: the defaults, then `enable` and `disable`, then dependencies.
//!
//! A team install installs the catalog's recommended components for the profile and
//! [`FEATURES`]; a personal install the ones recommended for [`PERSONAL_PROFILE`]. Only components
//! whose `testedOn` names
//! the distribution are installed: a default one that is not is left out, and one the
//! configuration or a dependency asks for stops the plan. Names are capabilities or component ids,
//! so the catalog decides which component fills each one. The install order is the union of each
//! chosen component's `installOrder`, taken in id order: every list already places a component
//! after its dependencies, so the union does too, and the result never depends on the order of
//! the configuration file.

use std::collections::{BTreeMap, BTreeSet};

use crate::PlanError;
use crate::catalog::{Component, Default, Index};
use crate::config::{Config, PersonalConfig};

/// The distribution of a personal install.
pub const PERSONAL_DISTRIBUTION: &str = "kubesolo";

/// The profile whose values a personal install uses.
pub const PERSONAL_PROFILE: &str = "single-node";

/// What a team install adds to the catalog's recommended components: deploying from images and
/// Git, and PostgreSQL databases.
pub const FEATURES: &[&str] = &["builds", "image-updates", "databases/cloudnative-pg"];

/// What Zeabur needs to run: HTTPS and usage numbers.
pub const REQUIRED: &[&str] = &["ingress", "certificates", "metrics/metrics-server"];

/// The chosen components, in install order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// Component ids and why each is installed, in install order.
    pub components: Vec<(String, Why)>,
    /// Components the defaults or configuration named that this installer leaves out, with the
    /// reason.
    pub left_out: BTreeMap<String, String>,
}

/// Why a component is installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// Installed by default.
    Default,
    /// Named in `enable`.
    Enabled,
    /// Another chosen component needs it.
    Dependency,
}

/// Chooses the components for a configuration.
///
/// # Errors
///
/// When a name matches nothing, a named component was not tested on the distribution, or a chosen
/// component needs one that is disabled, unsupported or untested.
pub fn select(index: &Index, config: &Config) -> Result<Selection, PlanError> {
    let profile = config.profile.as_str();
    let distribution = config.distribution.id();
    let mut base = recommended(index, profile);
    base.extend(resolve_all(index, profile, distribution, FEATURES)?);
    choose(
        index,
        &Wanted {
            base,
            enable: &config.enable,
            disable: &config.disable,
            profile,
            distribution,
        },
    )
}

/// Chooses the components of a personal install: those the catalog recommends for
/// [`PERSONAL_PROFILE`] and tested on [`PERSONAL_DISTRIBUTION`], then `enable` and `disable`.
///
/// # Errors
///
/// As [`select`].
pub fn select_personal(index: &Index, config: &PersonalConfig) -> Result<Selection, PlanError> {
    choose(
        index,
        &Wanted {
            base: recommended(index, PERSONAL_PROFILE)
                .into_iter()
                .filter(|id| {
                    index
                        .component(id)
                        .is_some_and(|c| c.is_tested_on(PERSONAL_DISTRIBUTION))
                })
                .collect(),
            enable: &config.enable,
            disable: &config.disable,
            profile: PERSONAL_PROFILE,
            distribution: PERSONAL_DISTRIBUTION,
        },
    )
}

struct Wanted<'a> {
    base: Vec<String>,
    enable: &'a [String],
    disable: &'a [String],
    profile: &'a str,
    distribution: &'a str,
}

fn untested(component: &Component, distribution: &str) -> PlanError {
    PlanError::Untested {
        component: component.id.clone(),
        distribution: distribution.to_owned(),
        tested_on: component.tested_on.clone(),
    }
}

fn choose(index: &Index, wanted: &Wanted<'_>) -> Result<Selection, PlanError> {
    let profile = wanted.profile;
    let distribution = wanted.distribution;
    let mut chosen: BTreeMap<String, Why> = BTreeMap::new();
    let mut left_out = BTreeMap::new();
    for id in &wanted.base {
        chosen.insert(id.clone(), Why::Default);
    }
    for name in wanted.enable {
        chosen.insert(resolve(index, profile, distribution, name)?, Why::Enabled);
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

    let mut unusable = Vec::new();
    for (id, why) in &chosen {
        let Some(component) = index.component(id) else {
            continue;
        };
        if !component.supports(distribution) {
            left_out.insert(
                id.clone(),
                format!("runs on {} only", component.distributions.join(", ")),
            );
        } else if component.default_for(profile) == Default::Unsupported {
            left_out.insert(id.clone(), format!("unsupported on {profile}"));
        } else if !component.is_tested_on(distribution) {
            if *why == Why::Enabled {
                return Err(untested(component, distribution));
            }
            left_out.insert(id.clone(), format!("not tested on {distribution}"));
        } else {
            continue;
        }
        unusable.push(id.clone());
    }
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
        if let Some(dep) = chain.iter().find(|dep| {
            !disabled.contains(*dep)
                && index
                    .component(dep)
                    .is_some_and(|c| !c.is_tested_on(distribution))
        }) {
            if *why == Why::Default {
                left_out.insert(
                    id.clone(),
                    format!("needs {dep}, not tested on {distribution}"),
                );
                continue;
            }
            return Err(index.component(dep).map_or_else(
                || PlanError::Unknown(dep.clone()),
                |c| untested(c, distribution),
            ));
        }
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

/// The components that [`REQUIRED`] names on a profile; names the catalog lacks are skipped.
#[must_use]
pub fn required(index: &Index, profile: &str) -> BTreeSet<String> {
    REQUIRED
        .iter()
        .filter_map(|name| resolve(index, profile, "", name).ok())
        .collect()
}

fn resolve_all(
    index: &Index,
    profile: &str,
    distribution: &str,
    names: &[&str],
) -> Result<Vec<String>, PlanError> {
    names
        .iter()
        .map(|name| resolve(index, profile, distribution, name))
        .collect()
}

/// A component id, or the provider of a capability: its recommended component on the profile,
/// else its first optional one (the catalog's own rule for a bare `requires`), taking one tested
/// on the distribution first.
fn resolve(
    index: &Index,
    profile: &str,
    distribution: &str,
    name: &str,
) -> Result<String, PlanError> {
    if name.contains('/') {
        return index
            .component(name)
            .map(|c| c.id.clone())
            .ok_or_else(|| PlanError::Unknown(name.to_owned()));
    }
    let mut providers: Vec<_> = index
        .components
        .iter()
        .filter(|c| c.capability == name)
        .filter(|c| c.default_for(profile) != Default::Unsupported)
        .collect();
    providers.sort_by_key(|c| {
        (
            !c.is_tested_on(distribution),
            c.default_for(profile) != Default::Recommended,
        )
    });
    providers
        .first()
        .map(|c| c.id.clone())
        .ok_or_else(|| PlanError::Unknown(name.to_owned()))
}

fn matches(index: &Index, id: &str, name: &str) -> bool {
    id == name || index.component(id).is_some_and(|c| c.capability == name)
}
