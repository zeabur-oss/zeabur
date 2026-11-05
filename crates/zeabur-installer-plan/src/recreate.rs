//! Objects an upgrade deletes and creates again, because the API server refuses to update them
//! in place, such as a StatefulSet whose `volumeClaimTemplates` change (zeabur-oss/zeabur#269).
//!
//! The catalog lists them per component ([`Recreates`]); `apply` deletes each with
//! `--cascade=orphan` before the upgrade, so what it created stays, and Helm creates it again.

use std::cmp::Ordering;

use crate::catalog::{Component, Recreate, Recreates};
use crate::plan::{Warning, warning};
use crate::record::{ChartChange, Record, precedence};

/// What the planned install changes against the record that matters here.
#[derive(Debug, Clone, Copy)]
pub struct Upgrade<'a> {
    /// The recorded install.
    pub record: &'a Record,
    /// The planned profile.
    pub profile: &'a str,
    /// The catalog version of the index, when it has one.
    pub catalog: Option<&'a str>,
}

impl<'a> Upgrade<'a> {
    /// From the record to the planned profile and catalog version.
    #[must_use]
    pub const fn new(record: &'a Record, profile: &'a str, catalog: Option<&'a str>) -> Self {
        Self {
            record,
            profile,
            catalog,
        }
    }
}

/// The objects the upgrade of one release recreates: none unless its chart changes.
#[must_use]
pub fn for_release(
    component: &Component,
    release: &str,
    change: Option<&ChartChange>,
    upgrade: Upgrade<'_>,
) -> Vec<Recreate> {
    let Some(change) = change else {
        return Vec::new();
    };
    let mut found = Vec::new();
    if switches_profile(upgrade, change) {
        add(&mut found, &component.recreates.profile_switch, release);
    }
    for entries in steps(&component.recreates, upgrade) {
        add(&mut found, entries, release);
    }
    found
}

/// A record from before the installer kept the profile says nothing about it; a values change
/// may then be a switch.
fn switches_profile(upgrade: Upgrade<'_>, change: &ChartChange) -> bool {
    upgrade
        .record
        .profile
        .as_deref()
        .map_or(change.values, |recorded| recorded != upgrade.profile)
}

/// The entries of each catalog version from the recorded one up to, not including, the planned
/// one. A missing version leaves that end open, and a key that is not a version counts.
fn steps<'a>(recreates: &'a Recreates, upgrade: Upgrade<'_>) -> Vec<&'a Vec<Recreate>> {
    let installed = upgrade.record.catalog.as_deref();
    let mut steps = Vec::new();
    if installed.is_some() && installed == upgrade.catalog {
        return steps;
    }
    for (from, entries) in &recreates.from_version {
        if let Some(installed) = installed
            && precedence(from, installed) == Some(Ordering::Less)
        {
            continue;
        }
        if let Some(planned) = upgrade.catalog
            && matches!(
                precedence(from, planned),
                Some(Ordering::Equal | Ordering::Greater)
            )
        {
            continue;
        }
        steps.push(entries);
    }
    steps
}

/// Adds the entries of `release`, each object once with the union of its fields.
fn add(found: &mut Vec<Recreate>, entries: &[Recreate], release: &str) {
    for entry in entries {
        if entry.release != release {
            continue;
        }
        let same = found.iter_mut().find(|f| {
            f.namespace == entry.namespace && f.kind == entry.kind && f.name == entry.name
        });
        match same {
            Some(same) => {
                for field in &entry.fields {
                    if !same.fields.contains(field) {
                        same.fields.push(field.clone());
                    }
                }
            }
            None => found.push(entry.clone()),
        }
    }
}

/// What `apply` does about an object the upgrade of `release` recreates.
#[must_use]
pub fn warning_for(release: &str, object: &Recreate) -> Warning {
    let named = format!("{} {}/{}", object.kind, object.namespace, object.name);
    let text = if object.kind == "StatefulSet" {
        format!(
            "Upgrading {release} recreates {named}; its Pods and volumes stay, and the volumes \
             keep their size."
        )
    } else {
        format!("Upgrading {release} recreates {named}; what it created stays.")
    };
    warning("recreate", &[release, &object.kind, &named], text)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    fn zot(fields: &[&str]) -> Recreate {
        Recreate {
            release: "zot".to_owned(),
            namespace: "zot".to_owned(),
            kind: "StatefulSet".to_owned(),
            name: "zot".to_owned(),
            fields: fields.iter().map(|&f| f.to_owned()).collect(),
        }
    }

    fn builder(recreates: Recreates) -> Component {
        let mut component: Component = serde_json::from_value(serde_json::json!({
            "id": "builds/zeabur-builder",
            "capability": "builds",
            "metadata": {"name": "Zeabur Builder"},
            "defaults": {},
            "distributions": ["k3s"],
            "kubeVersion": ">=1.34.0-0",
            "testedOn": [],
            "releases": [],
            "security": {"summary": "", "podSecurity": "restricted"},
        }))
        .unwrap();
        component.recreates = recreates;
        component
    }

    fn record(profile: Option<&str>, catalog: Option<&str>) -> Record {
        Record {
            installer: "0.2.0".to_owned(),
            components: BTreeSet::new(),
            version: None,
            charts: BTreeMap::new(),
            values: BTreeMap::new(),
            helm: BTreeSet::new(),
            profile: profile.map(str::to_owned),
            catalog: catalog.map(str::to_owned),
        }
    }

    fn change(values: bool) -> ChartChange {
        ChartChange {
            component: "builds/zeabur-builder".to_owned(),
            release: "zot".to_owned(),
            from: "1.0.0".to_owned(),
            to: "1.0.0".to_owned(),
            values,
        }
    }

    fn found(
        component: &Component,
        record: &Record,
        profile: &str,
        catalog: &str,
    ) -> Vec<Recreate> {
        let upgrade = Upgrade {
            record,
            profile,
            catalog: Some(catalog),
        };
        for_release(component, "zot", Some(&change(true)), upgrade)
    }

    #[test]
    fn a_profile_switch_recreates_what_the_catalog_lists_for_it() {
        let component = builder(Recreates {
            profile_switch: vec![zot(&["spec"])],
            from_version: BTreeMap::new(),
        });
        let single = record(Some("single-node"), Some("0.3.0"));
        assert_eq!(
            found(&component, &single, "multi-node", "0.3.0"),
            [zot(&["spec"])]
        );
        assert_eq!(found(&component, &single, "single-node", "0.3.0"), []);
    }

    #[test]
    fn an_unchanged_chart_recreates_nothing() {
        let component = builder(Recreates {
            profile_switch: vec![zot(&["spec"])],
            from_version: BTreeMap::new(),
        });
        let upgrade = Upgrade {
            record: &record(Some("single-node"), None),
            profile: "multi-node",
            catalog: None,
        };
        assert_eq!(for_release(&component, "zot", None, upgrade), []);
    }

    #[test]
    fn an_upgrade_over_several_versions_recreates_what_each_step_lists() {
        let other = Recreate {
            name: "zot-cache".to_owned(),
            ..zot(&["spec.serviceName"])
        };
        let component = builder(Recreates {
            profile_switch: Vec::new(),
            from_version: BTreeMap::from([
                ("0.1.0".to_owned(), vec![zot(&["spec.selector"])]),
                ("0.2.0".to_owned(), vec![zot(&["spec"]), other.clone()]),
            ]),
        });
        let old = record(Some("single-node"), Some("0.1.0"));
        assert_eq!(
            found(&component, &old, "single-node", "0.3.0"),
            [zot(&["spec.selector", "spec"]), other.clone()]
        );
        let previous = record(Some("single-node"), Some("0.2.0"));
        assert_eq!(
            found(&component, &previous, "single-node", "0.3.0"),
            [zot(&["spec"]), other]
        );
        let current = record(Some("single-node"), Some("0.3.0"));
        assert_eq!(found(&component, &current, "single-node", "0.3.0"), []);
    }

    #[test]
    fn a_record_without_a_profile_counts_a_values_change_as_a_switch() {
        let component = builder(Recreates {
            profile_switch: vec![zot(&["spec"])],
            from_version: BTreeMap::new(),
        });
        let old = record(None, Some("0.3.0"));
        let upgrade = Upgrade {
            record: &old,
            profile: "multi-node",
            catalog: Some("0.3.0"),
        };
        assert_eq!(
            for_release(&component, "zot", Some(&change(true)), upgrade),
            [zot(&["spec"])]
        );
        assert_eq!(
            for_release(&component, "zot", Some(&change(false)), upgrade),
            []
        );
    }

    #[test]
    fn the_warning_names_the_object() {
        let text = warning_for("zot", &zot(&["spec"])).text;
        assert!(text.contains("recreates StatefulSet zot/zot"), "{text}");
        assert!(text.contains("Pods and volumes stay"), "{text}");
    }
}
