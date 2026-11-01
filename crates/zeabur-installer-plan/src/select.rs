//! Choosing components: the defaults, then `enable` and `disable`, then dependencies.
//!
//! On a server with an install record, a default that the record does not have is installed only
//! when `enable` names it or a chosen component needs it.
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
use crate::plan::{Warning, warning};

/// The distribution of a personal install.
pub const PERSONAL_DISTRIBUTION: &str = "kubesolo";

/// The profile whose values a personal install uses.
pub const PERSONAL_PROFILE: &str = "single-node";

/// What a team install adds to the catalog's recommended components: deploying from images and
/// Git, and PostgreSQL databases.
pub const FEATURES: &[&str] = &["builds", "image-updates", "databases/cloudnative-pg"];

/// What Zeabur needs to run: HTTPS and usage numbers.
pub const REQUIRED: &[&str] = &["ingress", "certificates", "metrics/metrics-server"];

/// Component ids the catalog renamed, old to new.
pub const RENAMED: &[(&str, &str)] = &[("container-registry/zot", "builds/zeabur-builder")];

/// The id a component the catalog renamed has now.
#[must_use]
pub fn renamed(old: &str) -> Option<&'static str> {
    RENAMED
        .iter()
        .find(|(from, _)| *from == old)
        .map(|(_, to)| *to)
}

/// The chosen components, in install order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// Component ids and why each is installed, in install order.
    pub components: Vec<(String, Why)>,
    /// Components the defaults or configuration named that this installer leaves out, with the
    /// reason.
    pub left_out: BTreeMap<String, String>,
    /// Defaults that the install record does not have and that are not installed.
    pub offered: Vec<String>,
    /// About the configuration's names.
    pub warnings: Vec<Warning>,
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

/// Chooses the components for a configuration, on a server whose install record has `installed`.
///
/// # Errors
///
/// When a name matches nothing, a named component was not tested on the distribution, or a chosen
/// component needs one that is disabled, unsupported or untested.
pub fn select(
    index: &Index,
    config: &Config,
    installed: Option<&BTreeSet<String>>,
) -> Result<Selection, PlanError> {
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
            installed,
        },
    )
}

/// Chooses the components of a personal install: those the catalog recommends for
/// [`PERSONAL_PROFILE`] and tested on [`PERSONAL_DISTRIBUTION`], then `enable` and `disable`.
///
/// # Errors
///
/// As [`select`].
pub fn select_personal(
    index: &Index,
    config: &PersonalConfig,
    installed: Option<&BTreeSet<String>>,
) -> Result<Selection, PlanError> {
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
            installed,
        },
    )
}

struct Wanted<'a> {
    base: Vec<String>,
    enable: &'a [String],
    disable: &'a [String],
    profile: &'a str,
    distribution: &'a str,
    /// The install record's components.
    installed: Option<&'a BTreeSet<String>>,
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
    for id in &wanted.base {
        chosen.insert(id.clone(), Why::Default);
    }
    for name in wanted.enable {
        let id = resolve(index, profile, distribution, name).map_err(|e| match renamed(name) {
            Some(new) if index.component(new).is_some() => PlanError::Renamed {
                old: name.clone(),
                new: new.to_owned(),
            },
            Some(_) | None => e,
        })?;
        chosen.insert(id, Why::Enabled);
    }
    let mut warnings = Vec::new();
    let mut disabled = BTreeSet::new();
    for name in wanted.disable {
        let name = current_name(index, name, &mut warnings);
        let matched: Vec<String> = chosen
            .keys()
            .filter(|id| matches(index, id, name))
            .cloned()
            .collect();
        if matched.is_empty() && !known(index, name) {
            return Err(PlanError::Unknown(name.to_owned()));
        }
        for id in matched {
            chosen.remove(&id);
            disabled.insert(id);
        }
    }

    let mut left_out = leave_out_unusable(index, profile, distribution, &mut chosen)?;
    let taken_over = under_new_names(wanted.installed);
    let held = hold_back(index, profile, taken_over.as_ref(), &mut chosen);

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
    let offered = held
        .into_iter()
        .filter(|id| !order.iter().any(|(seen, _)| seen == id))
        .collect();
    Ok(Selection {
        components: order,
        left_out,
        offered,
        warnings,
    })
}

/// Takes out of `chosen` the components that cannot run on the distribution or profile, or that
/// were not tested on the distribution, and returns them with the reason.
fn leave_out_unusable(
    index: &Index,
    profile: &str,
    distribution: &str,
    chosen: &mut BTreeMap<String, Why>,
) -> Result<BTreeMap<String, String>, PlanError> {
    let mut left_out = BTreeMap::new();
    let mut unusable = Vec::new();
    for (id, why) in chosen.iter() {
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
    Ok(left_out)
}

/// The install record's components, with each renamed one also under its new id: a server that
/// has `container-registry/zot` has `builds/zeabur-builder`, which keeps the same zot release.
#[must_use]
pub fn under_new_names(installed: Option<&BTreeSet<String>>) -> Option<BTreeSet<String>> {
    let mut all = installed?.clone();
    for (old, new) in RENAMED {
        if all.contains(*old) {
            all.insert((*new).to_owned());
        }
    }
    Some(all)
}

/// Takes out of `chosen` the defaults that `installed` lacks, except what Zeabur needs, and
/// returns them.
fn hold_back(
    index: &Index,
    profile: &str,
    installed: Option<&BTreeSet<String>>,
    chosen: &mut BTreeMap<String, Why>,
) -> Vec<String> {
    let Some(installed) = installed else {
        return Vec::new();
    };
    let required = required(index, profile);
    let held: Vec<String> = chosen
        .iter()
        .filter(|(id, why)| {
            **why == Why::Default && !installed.contains(*id) && !required.contains(*id)
        })
        .map(|(id, _)| id.clone())
        .collect();
    for id in &held {
        chosen.remove(id);
    }
    held
}

fn known(index: &Index, name: &str) -> bool {
    index.components.iter().any(|c| matches(index, &c.id, name))
}

/// `name`, or the id the catalog renamed it to, with a warning.
fn current_name<'a>(index: &Index, name: &'a str, warnings: &mut Vec<Warning>) -> &'a str {
    match renamed(name) {
        Some(new) if !known(index, name) && known(index, new) => {
            warnings.push(warning(
                "config.renamed",
                &[name, new],
                format!(
                    "disable: `{name}` is now `{new}` in this catalog, so `{new}` is not installed. \
                     Write `{new}` in the configuration file instead."
                ),
            ));
            new
        }
        Some(_) | None => name,
    }
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
