//! Choosing components: the defaults, then `enable` and `disable`, then dependencies. The
//! install record does not change the choice.
//!
//! A team install installs the catalog's recommended components for the profile and
//! [`FEATURES`]; a personal install the ones recommended for [`PERSONAL_PROFILE`]. Only components
//! whose `testedOn` names
//! the distribution are installed by default: a default one that is not is left out, and one the
//! configuration enables is installed with a warning. Names are capabilities or component ids,
//! so the catalog decides which component fills each one. The install order is the union of each
//! chosen component's `installOrder`, taken in id order: every list already places a component
//! after its dependencies, so the union does too, and the result never depends on the order of
//! the configuration file.

use std::collections::{BTreeMap, BTreeSet};

use crate::PlanError;
use crate::catalog::{Default, Index};
use crate::config::{Config, PersonalConfig};
use crate::plan::{Warning, warning};

/// The distribution of a personal install.
pub const PERSONAL_DISTRIBUTION: &str = "kubesolo";

/// The profile whose values a personal install uses.
pub const PERSONAL_PROFILE: &str = "single-node";

/// What a team install adds to the catalog's recommended components: deploying from images and
/// Git, and PostgreSQL databases.
pub const FEATURES: &[&str] = &["builds", "image-updates", "databases/cloudnative-pg"];

/// What Zeabur needs to run: HTTPS and usage numbers. `gui` never suggests leaving them out.
pub const REQUIRED: &[&str] = &["ingress", "certificates", "metrics/metrics-server"];

/// The chosen components, in install order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// Component ids and why each is installed, in install order.
    pub components: Vec<(String, Why)>,
    /// Components the defaults or configuration named that this installer leaves out, with the
    /// reason.
    pub left_out: BTreeMap<String, LeftOut>,
    /// About the configuration's names.
    pub warnings: Vec<Warning>,
}

/// Why a component is left out. Its text is what the plan shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeftOut {
    /// It runs only on these distributions.
    Distributions(Vec<String>),
    /// Its `defaults` is `unsupported` on the profile.
    Unsupported {
        /// The profile.
        profile: String,
    },
    /// A default that was not tested on the distribution.
    Untested {
        /// The distribution.
        distribution: String,
    },
    /// A default that needs a component not tested on the distribution.
    NeedsUntested {
        /// The component it needs.
        dependency: String,
        /// The distribution.
        distribution: String,
    },
}

impl std::fmt::Display for LeftOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Distributions(only) => write!(f, "runs on {} only", only.join(", ")),
            Self::Unsupported { profile } => write!(f, "unsupported on {profile}"),
            Self::Untested { distribution } => write!(f, "not tested on {distribution}"),
            Self::NeedsUntested {
                dependency,
                distribution,
            } => write!(f, "needs {dependency}, not tested on {distribution}"),
        }
    }
}

/// The reasons as the plan shows them.
#[must_use]
pub fn reasons(left_out: &BTreeMap<String, LeftOut>) -> BTreeMap<String, String> {
    left_out
        .iter()
        .map(|(id, reason)| (id.clone(), reason.to_string()))
        .collect()
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
/// When a name matches nothing or is an id that a component replaces, or a chosen component
/// needs one that is disabled or left out.
pub fn select(index: &Index, config: &Config) -> Result<Selection, PlanError> {
    let profile = config.profile.as_str();
    let distribution = config.distribution.id();
    choose(
        index,
        &Wanted {
            base: team_base(index, profile, distribution, FEATURES)?,
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

/// The recommended components for the profile and the `add` names.
fn team_base(
    index: &Index,
    profile: &str,
    distribution: &str,
    add: &[&str],
) -> Result<Vec<String>, PlanError> {
    let mut base = recommended(index, profile);
    base.extend(resolve_all(index, profile, distribution, add)?);
    Ok(base)
}

struct Wanted<'a> {
    base: Vec<String>,
    enable: &'a [String],
    disable: &'a [String],
    profile: &'a str,
    distribution: &'a str,
}

fn untested(id: &str, distribution: &str) -> Warning {
    warning(
        "config.untested",
        &[id, distribution],
        format!(
            "{id} was not tested on {distribution}. It is installed because the configuration \
             enables it or a component it enables needs it."
        ),
    )
}

fn choose(index: &Index, wanted: &Wanted<'_>) -> Result<Selection, PlanError> {
    let profile = wanted.profile;
    let distribution = wanted.distribution;
    let mut chosen: BTreeMap<String, Why> = BTreeMap::new();
    for id in &wanted.base {
        chosen.insert(id.clone(), Why::Default);
    }
    for name in wanted.enable.iter().chain(wanted.disable) {
        check_replaced(index, name)?;
    }
    for name in wanted.enable {
        chosen.insert(resolve(index, profile, distribution, name)?, Why::Enabled);
    }
    let mut warnings = Vec::new();
    let mut disabled = BTreeSet::new();
    for name in wanted.disable {
        let name = name.as_str();
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

    let mut left_out = leave_out_unusable(index, profile, distribution, &mut chosen, &mut warnings);

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
        if *why == Why::Default
            && let Some(dep) = chain.iter().find(|dep| {
                !disabled.contains(*dep)
                    && index
                        .component(dep)
                        .is_some_and(|c| !c.is_tested_on(distribution))
            })
        {
            left_out.insert(
                id.clone(),
                LeftOut::NeedsUntested {
                    dependency: dep.clone(),
                    distribution: distribution.to_owned(),
                },
            );
            continue;
        }
        for dep in chain {
            if order.iter().any(|(seen, _)| *seen == dep) {
                continue;
            }
            if *why == Why::Enabled && !disabled.contains(&dep) {
                take_untested(index, wanted, &dep, &mut left_out, &mut warnings);
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
        warnings,
    })
}

/// Puts back a dependency of an enabled component that was left out only because it was not tested
/// on the distribution, with a warning.
fn take_untested(
    index: &Index,
    wanted: &Wanted<'_>,
    dep: &str,
    left_out: &mut BTreeMap<String, LeftOut>,
    warnings: &mut Vec<Warning>,
) {
    let usable_untested = index.component(dep).is_some_and(|c| {
        c.supports(wanted.distribution)
            && c.default_for(wanted.profile) != Default::Unsupported
            && !c.is_tested_on(wanted.distribution)
    });
    if !usable_untested {
        return;
    }
    left_out.remove(dep);
    if !warnings
        .iter()
        .any(|w| w.code == "config.untested" && w.values.first().is_some_and(|v| v == dep))
    {
        warnings.push(untested(dep, wanted.distribution));
    }
}

/// Takes out of `chosen` the components that cannot run on the distribution or profile, and the
/// defaults that were not tested on the distribution, and returns them with the reason. An enabled
/// component that was not tested stays, with a warning.
fn leave_out_unusable(
    index: &Index,
    profile: &str,
    distribution: &str,
    chosen: &mut BTreeMap<String, Why>,
    warnings: &mut Vec<Warning>,
) -> BTreeMap<String, LeftOut> {
    let mut left_out = BTreeMap::new();
    let mut unusable = Vec::new();
    for (id, why) in chosen.iter() {
        let Some(component) = index.component(id) else {
            continue;
        };
        let enabled = *why == Why::Enabled;
        if !component.supports(distribution) {
            let only = component.distributions.join(", ");
            if enabled {
                warnings.push(warning(
                    "config.runsOn",
                    &[id, &only],
                    format!("The configuration enables {id}, which runs on {only} only, so it is not installed."),
                ));
            }
            left_out.insert(
                id.clone(),
                LeftOut::Distributions(component.distributions.clone()),
            );
        } else if component.default_for(profile) == Default::Unsupported {
            if enabled {
                warnings.push(warning(
                    "config.unsupportedOn",
                    &[id, profile],
                    format!("The configuration enables {id}, which is unsupported on {profile}, so it is not installed."),
                ));
            }
            left_out.insert(
                id.clone(),
                LeftOut::Unsupported {
                    profile: profile.to_owned(),
                },
            );
        } else if !component.is_tested_on(distribution) {
            if enabled {
                warnings.push(untested(id, distribution));
                continue;
            }
            left_out.insert(
                id.clone(),
                LeftOut::Untested {
                    distribution: distribution.to_owned(),
                },
            );
        } else {
            continue;
        }
        unusable.push(id.clone());
    }
    for id in &unusable {
        chosen.remove(id);
    }
    left_out
}

fn known(index: &Index, name: &str) -> bool {
    index.components.iter().any(|c| matches(index, &c.id, name))
}

/// Stops when a component replaces `name`.
fn check_replaced(index: &Index, name: &str) -> Result<(), PlanError> {
    index.replacement_of(name).map_or(Ok(()), |new| {
        Err(PlanError::Renamed {
            old: name.to_owned(),
            new: new.to_owned(),
        })
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use serde_json::{Value, json};

    use super::*;

    const CASES: &str = concat!(env!("ZEABUR_CATALOG"), "/conformance/select");

    fn names(value: &Value) -> Vec<&str> {
        value.as_array().map_or_else(Vec::new, |names| {
            names.iter().filter_map(Value::as_str).collect()
        })
    }

    fn owned(value: &Value) -> Vec<String> {
        names(value).into_iter().map(str::to_owned).collect()
    }

    fn index_of(case: &Value) -> Index {
        let components: Vec<Value> = case["components"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                let id = c["id"].as_str().unwrap();
                let mut component = c.clone();
                component["capability"] = json!(id.split('/').next().unwrap());
                component["metadata"] = json!({"name": id});
                component["releases"] = json!([]);
                component["security"] = json!({"summary": ""});
                component
            })
            .collect();
        let index = json!({
            "indexVersion": 2,
            "profiles": ["single-node", "multi-node"],
            "components": components,
        });
        Index::parse(index.to_string().as_bytes()).unwrap()
    }

    fn selected(case: &Value) -> Result<Selection, PlanError> {
        let index = index_of(case);
        let config = &case["config"];
        let profile = config["profile"].as_str().unwrap();
        let distribution = config["distribution"].as_str().unwrap();
        choose(
            &index,
            &Wanted {
                base: team_base(&index, profile, distribution, &names(&config["add"]))?,
                enable: &owned(&config["enable"]),
                disable: &owned(&config["disable"]),
                profile,
                distribution,
            },
        )
    }

    fn reason(left_out: &LeftOut) -> Value {
        match left_out {
            LeftOut::Distributions(distributions) => {
                json!({"reason": "distributions", "distributions": distributions})
            }
            LeftOut::Unsupported { profile } => {
                json!({"reason": "unsupported", "profile": profile})
            }
            LeftOut::Untested { distribution } => {
                json!({"reason": "untested", "distribution": distribution})
            }
            LeftOut::NeedsUntested {
                dependency,
                distribution,
            } => {
                json!({"reason": "needs-untested", "dependency": dependency, "distribution": distribution})
            }
        }
    }

    fn case_warning(warning: &Warning) -> Value {
        match (warning.code, warning.values.as_slice()) {
            ("config.untested", [component, distribution]) => {
                json!({"kind": "untested", "component": component, "distribution": distribution})
            }
            ("config.runsOn" | "config.unsupportedOn", [component, _]) => {
                json!({"kind": "left-out", "component": component})
            }
            _ => json!({"unexpected": warning.code, "values": warning.values}),
        }
    }

    /// `want` with its warnings in a fixed order, since a case lists them in any order.
    fn sorted_warnings(mut want: Value) -> Value {
        if let Some(warnings) = want.get_mut("warnings").and_then(Value::as_array_mut) {
            warnings.sort_by_key(Value::to_string);
        }
        want
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "a case can only expect the three selection errors"
    )]
    fn outcome(result: Result<Selection, PlanError>) -> Value {
        let selection = match result {
            Ok(selection) => selection,
            Err(error) => {
                let error = match error {
                    PlanError::Unknown(name) => json!({"kind": "unknown", "name": name}),
                    PlanError::Renamed { old, new } => {
                        json!({"kind": "replaced", "old": old, "new": new})
                    }
                    PlanError::NeedsDisabled {
                        component,
                        dependency,
                    } => {
                        json!({"kind": "needs-disabled", "component": component, "dependency": dependency})
                    }
                    other => json!({"unexpected": other.to_string()}),
                };
                return json!({"error": error});
            }
        };
        let mut out = serde_json::Map::new();
        if !selection.components.is_empty() {
            let components: Vec<Value> = selection
                .components
                .iter()
                .map(|(id, why)| {
                    let why = match why {
                        Why::Default => "default",
                        Why::Enabled => "enabled",
                        Why::Dependency => "dependency",
                    };
                    json!({"id": id, "why": why})
                })
                .collect();
            out.insert("components".to_owned(), json!(components));
        }
        if !selection.left_out.is_empty() {
            let left_out: serde_json::Map<String, Value> = selection
                .left_out
                .iter()
                .map(|(id, left_out)| (id.clone(), reason(left_out)))
                .collect();
            out.insert("leftOut".to_owned(), Value::Object(left_out));
        }
        if !selection.warnings.is_empty() {
            let warnings: Vec<Value> = selection.warnings.iter().map(case_warning).collect();
            out.insert("warnings".to_owned(), json!(warnings));
        }
        sorted_warnings(Value::Object(out))
    }

    #[test]
    fn the_catalogs_selection_cases_pass() {
        let mut files: Vec<_> = fs::read_dir(Path::new(CASES))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "yaml"))
            .collect();
        files.sort();
        assert!(!files.is_empty(), "no cases in {CASES}");
        let mut failures = Vec::new();
        for file in &files {
            let name = file.file_name().unwrap().to_string_lossy();
            let case: Value = match serde_saphyr::from_str(&fs::read_to_string(file).unwrap()) {
                Ok(case) => case,
                Err(error) => {
                    failures.push(format!("{name}: {error}"));
                    continue;
                }
            };
            let got = outcome(selected(&case));
            let want = sorted_warnings(case["want"].clone());
            if got != want {
                failures.push(format!(
                    "{name}: {}\n  got  {got}\n  want {want}",
                    case["description"].as_str().unwrap_or_default().trim(),
                ));
            }
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }
}
