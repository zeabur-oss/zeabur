//! Removing components that left the configuration.
//!
//! The installer removes only what it installed: the `HelmChart` objects in `kube-system` with
//! its `app.kubernetes.io/managed-by` label and the [`COMPONENT_ANNOTATION`]. Deleting one makes
//! helm-controller run `helm uninstall`. `helm uninstall` deletes the CRDs that the release owns
//! ([`release_crds`]), and with them all their objects. What Helm leaves stays: the other CRDs and
//! the volumes the catalog lists in `leftovers`, the objects of manifest releases, and the
//! component's namespaces with their Secrets.
//!
//! `apply` refuses to remove a component that something still uses ([`InUse`]), and one that
//! keeps data without volumes that stay, unless `allowDataLoss` names it.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::catalog::{Component, Index};
use crate::plan::{COMPONENT_ANNOTATION, ComponentPlan, MANAGER, Warning, warning};
use crate::storage::LONGHORN;

/// The leftover that means the chart's volumes stay.
const VOLUMES: &str = "persistentvolumeclaims";
/// The leftover that means the chart's CRDs stay.
const CRDS: &str = "customresourcedefinitions";
/// The annotations with which Helm marks the objects of a release.
const RELEASE_NAME: &str = "meta.helm.sh/release-name";
const RELEASE_NAMESPACE: &str = "meta.helm.sh/release-namespace";
/// The annotation with which `helm uninstall` keeps an object.
const RESOURCE_POLICY: &str = "helm.sh/resource-policy";
/// The Longhorn setting its uninstall job reads; the job fails while it is `false`.
const LONGHORN_CONFIRMATION: &str = "deleting-confirmation-flag";

/// A `HelmChart` the installer made, as found in the cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// The component in its [`COMPONENT_ANNOTATION`].
    pub component: String,
    /// The `HelmChart`'s name, which is also the Helm release's name.
    pub chart: String,
    /// The release's namespace.
    pub namespace: String,
}

/// One component to remove.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Removal {
    /// `<capability>/<upstream>`.
    pub id: String,
    /// Name for people.
    pub display_name: String,
    /// The `HelmChart` objects in `kube-system` that `apply` deletes.
    pub helm_charts: Vec<String>,
    /// Namespaces that stay, with what is left in them.
    pub namespaces: Vec<String>,
    /// What `helm uninstall` leaves, as the catalog says. It does not have
    /// `customresourcedefinitions` when the cluster shows that the release owns its CRDs.
    pub leftovers: Vec<String>,
    /// The CRDs that `helm uninstall` deletes, with all their objects.
    pub deleted_crds: Vec<String>,
    /// Manifest releases, whose objects stay.
    pub manifests: Vec<String>,
    /// What `apply` looks for before it removes the component.
    pub in_use: Vec<InUse>,
    /// What still uses it, found by the installer; any entry stops `apply`.
    pub used_by: Vec<String>,
    /// Removing it deletes data it keeps.
    pub loses_data: bool,
    /// `allowDataLoss` names it.
    pub confirmed: bool,
    /// `kubectl` arguments run before the `HelmChart` objects are deleted.
    pub prepare: Vec<Vec<String>>,
    /// CRDs whose objects it writes itself, so they are not something that uses it.
    #[serde(skip)]
    pub internal_crds: Vec<String>,
}

impl Removal {
    /// Records the CRDs that `helm uninstall` deletes ([`release_crds`]). The objects of these
    /// CRDs outside the namespaces removed together then count as use.
    pub fn delete_crds(&mut self, crds: Vec<String>) {
        if crds.is_empty() {
            return;
        }
        self.leftovers.retain(|l| l != CRDS);
        for crd in crds.iter().filter(|crd| !self.internal_crds.contains(crd)) {
            let check = InUse::Objects {
                resource: crd.clone(),
            };
            if !self.in_use.contains(&check) {
                self.in_use.push(check);
            }
        }
        self.deleted_crds = crds;
    }

    /// Why `apply` does not remove it, if it does not.
    #[must_use]
    pub fn refusal(&self) -> Option<String> {
        if !self.used_by.is_empty() {
            Some(format!(
                "{} is still in use: {}",
                self.id,
                self.used_by.join(", ")
            ))
        } else if self.loses_data && !self.confirmed {
            Some(format!(
                "removing {} deletes the data it keeps; to remove it anyway, add it to \
                 allowDataLoss in the configuration",
                self.id
            ))
        } else {
            None
        }
    }
}

/// What uses a component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum InUse {
    /// Objects of a CRD (`<plural>.<group>`), outside the namespaces of the components removed
    /// together. Cluster-scoped objects count when the CRD is in `deleted_crds`.
    Objects {
        /// The resource.
        resource: String,
    },
    /// `PersistentVolume` objects of a CSI driver.
    Volumes {
        /// The driver.
        driver: String,
    },
    /// Pods that run images from a registry, outside the namespaces of the components removed
    /// together.
    Images {
        /// `host:port`.
        registry: String,
    },
}

/// The label selector that finds the installer's `HelmChart` objects.
#[must_use]
pub fn selector() -> String {
    format!("app.kubernetes.io/managed-by={MANAGER}")
}

/// The [`Installed`] of a `HelmChart` object; `None` for one without the annotation, such as
/// Zeabur's own.
#[must_use]
pub fn installed(object: &serde_json::Value) -> Option<Installed> {
    let metadata = object.get("metadata")?;
    Some(Installed {
        component: metadata
            .pointer(&format!(
                "/annotations/{}",
                COMPONENT_ANNOTATION.replace('/', "~1")
            ))?
            .as_str()?
            .to_owned(),
        chart: metadata.get("name")?.as_str()?.to_owned(),
        namespace: object
            .pointer("/spec/targetNamespace")
            .and_then(serde_json::Value::as_str)
            .or_else(|| metadata.get("namespace")?.as_str())
            .unwrap_or("kube-system")
            .to_owned(),
    })
}

/// The names of the CRDs in `crds` that `helm uninstall` of the component's charts deletes.
///
/// Helm marks the CRDs of a chart's templates with the release, and does not mark those of its
/// `crds/` folder. A CRD with `helm.sh/resource-policy: keep` stays.
#[must_use]
pub fn release_crds(crds: &[serde_json::Value], installed: &[Installed], id: &str) -> Vec<String> {
    let annotation = |crd: &serde_json::Value, key: &str| {
        crd.pointer(&format!("/metadata/annotations/{}", key.replace('/', "~1")))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    crds.iter()
        .filter(|crd| {
            let name = annotation(crd, RELEASE_NAME);
            let namespace = annotation(crd, RELEASE_NAMESPACE);
            installed.iter().any(|i| {
                i.component == id
                    && name.as_deref() == Some(&i.chart)
                    && namespace.as_deref() == Some(&i.namespace)
            }) && annotation(crd, RESOURCE_POLICY).as_deref() != Some("keep")
        })
        .filter_map(|crd| crd.pointer("/metadata/name")?.as_str())
        .map(str::to_owned)
        .collect()
}

/// The components installed but no longer planned, in the order to remove them: a component
/// before what it needs.
///
/// A component the catalog no longer has is not removed; a warning says how to remove it by hand.
#[must_use]
pub fn removals(
    index: &Index,
    planned: &[ComponentPlan],
    installed: &[Installed],
    profile: &str,
    allow_data_loss: &[String],
) -> (Vec<Removal>, Vec<Warning>) {
    let planned: BTreeSet<&str> = planned.iter().map(|c| c.id.as_str()).collect();
    let gone: BTreeSet<&str> = installed
        .iter()
        .map(|i| i.component.as_str())
        .filter(|id| !planned.contains(id))
        .filter(|id| {
            !index
                .replacement_of(id)
                .is_some_and(|new| planned.contains(new))
        })
        .collect();
    let mut warnings = Vec::new();
    let mut order: Vec<&Component> = Vec::new();
    for id in &gone {
        let Some(component) = index.component(id) else {
            let charts: Vec<&str> = installed
                .iter()
                .filter(|i| i.component == *id)
                .map(|i| i.chart.as_str())
                .collect();
            warnings.push(warning(
                "remove.unknown",
                &[id, &charts.join(" ")],
                format!(
                    "{id} is installed but not in this catalog, so apply leaves it. To remove \
                     it: kubectl -n kube-system delete helmchart {}",
                    charts.join(" ")
                ),
            ));
            continue;
        };
        let chain = component
            .install_order
            .get(profile)
            .cloned()
            .unwrap_or_else(|| vec![component.id.clone()]);
        for dep in chain {
            if gone.contains(dep.as_str()) && !order.iter().any(|c| c.id == dep) {
                order.extend(index.component(&dep));
            }
        }
    }
    order.reverse();
    let removals = order
        .into_iter()
        .map(|component| removal(component, installed, allow_data_loss))
        .collect();
    (removals, warnings)
}

fn removal(component: &Component, installed: &[Installed], allow_data_loss: &[String]) -> Removal {
    let id = component.id.as_str();
    let mut namespaces: Vec<String> = Vec::new();
    for release in &component.releases {
        if release.namespace != "kube-system" && !namespaces.contains(&release.namespace) {
            namespaces.push(release.namespace.clone());
        }
    }
    let mut in_use: Vec<InUse> = component
        .in_use_crds()
        .map(|crd| InUse::Objects {
            resource: crd.clone(),
        })
        .collect();
    in_use.extend(
        component
            .detection
            .csi_drivers
            .iter()
            .map(|driver| InUse::Volumes {
                driver: driver.clone(),
            }),
    );
    if let Some(registry) = &component.provides.registry {
        in_use.push(InUse::Images {
            registry: registry.host.clone(),
        });
    }
    let loses_data = component.persists_data && !component.leftovers.iter().any(|l| l == VOLUMES);
    let confirmed = allow_data_loss.iter().any(|a| a == id);
    let prepare = if id == LONGHORN && confirmed {
        let namespace = namespaces.first().map_or("longhorn-system", String::as_str);
        vec![
            [
                "patch",
                "--namespace",
                namespace,
                "settings.longhorn.io",
                LONGHORN_CONFIRMATION,
                "--type=merge",
                "--patch",
                r#"{"value":"true"}"#,
            ]
            .map(str::to_owned)
            .to_vec(),
        ]
    } else {
        Vec::new()
    };
    Removal {
        id: id.to_owned(),
        display_name: component.metadata.name.clone(),
        helm_charts: installed
            .iter()
            .filter(|i| i.component == id)
            .map(|i| i.chart.clone())
            .collect(),
        namespaces,
        leftovers: component.leftovers.clone(),
        deleted_crds: Vec::new(),
        manifests: component
            .releases
            .iter()
            .filter(|r| r.chart.is_none())
            .map(|r| r.name.clone())
            .collect(),
        in_use,
        used_by: Vec::new(),
        loses_data,
        confirmed,
        prepare,
        internal_crds: component.internal_crds.clone(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::config::Config;
    use crate::plan::{Facts, plan};
    const INDEX: &str = include_str!(concat!(env!("ZEABUR_CATALOG_RELEASE"), "/index.json"));

    fn index() -> Index {
        Index::parse(INDEX.as_bytes()).unwrap()
    }

    fn planned(extra: &str) -> Vec<ComponentPlan> {
        let config = Config::parse(&format!(
            "access: {{mode: domain, identity: login.example.com, zeabur: zeabur.example.com}}\n\
             sources:\n  identityImage: ghcr.io/zeabur-oss/identity@sha256:aa\n  identityctl: /tmp/ctl\n  \
             zeaburImage: zeabur:dev\n  zeaburChart: /tmp/chart\n{extra}"
        ))
        .unwrap();
        let facts = Facts {
            arch: "arm64".to_owned(),
            hostname: "vps-1".to_owned(),
            node_name: None,
            host_hex: "a1b2c3".to_owned(),
            memory_mib: 8192,
            node_ips: Vec::new(),
            record: None,
            installer: None,
        };
        plan(&index(), &config, &facts).unwrap().components
    }

    fn installed(planned: &[ComponentPlan]) -> Vec<Installed> {
        planned
            .iter()
            .flat_map(|c| {
                c.releases.iter().filter_map(|r| match r {
                    crate::plan::ReleasePlan::HelmChart { object, .. } => super::installed(object),
                    crate::plan::ReleasePlan::Manifests { .. }
                    | crate::plan::ReleasePlan::Urls { .. } => None,
                })
            })
            .collect()
    }

    #[test]
    fn what_apply_installed_it_finds_again_by_the_label_and_annotation() {
        for component in planned("enable: [replicated-storage/longhorn]\n") {
            for release in &component.releases {
                if let crate::plan::ReleasePlan::HelmChart { name, object, .. } = release {
                    assert_eq!(
                        object["metadata"]["labels"]["app.kubernetes.io/managed-by"],
                        selector().split_once('=').unwrap().1
                    );
                    assert_eq!(
                        super::installed(object),
                        Some(Installed {
                            component: component.id.clone(),
                            chart: name.clone(),
                            namespace: object["spec"]["targetNamespace"]
                                .as_str()
                                .unwrap()
                                .to_owned(),
                        })
                    );
                }
            }
        }
        let zeabur = json!({"metadata": {"name": "zeabur", "labels": {"app.kubernetes.io/managed-by": MANAGER}}});
        assert_eq!(super::installed(&zeabur), None);
    }

    #[test]
    fn nothing_is_removed_while_the_configuration_keeps_it() {
        let now = planned("enable: [replicated-storage/longhorn]\n");
        let (removals, warnings) = removals(&index(), &now, &installed(&now), "single-node", &[]);
        assert_eq!(removals, []);
        assert_eq!(warnings, []);
    }

    #[test]
    fn zot_is_not_removed_while_builder_takes_over_its_release() {
        let now = planned("");
        assert!(
            now.iter().any(|c| c.id == "builds/zeabur-builder"),
            "builder is planned"
        );
        let mut found = installed(&now);
        for chart in &mut found {
            if chart.component == "builds/zeabur-builder" && chart.chart == "zot" {
                chart.component = "container-registry/zot".to_owned();
            }
        }
        let (kept, warnings) = removals(&index(), &now, &found, "single-node", &[]);
        assert_eq!(kept, []);
        assert_eq!(warnings, []);

        let without = planned("disable: [builds/zeabur-builder]\n");
        found.retain(|chart| chart.component == "container-registry/zot");
        let (left, warnings) = removals(&index(), &without, &found, "single-node", &[]);
        assert_eq!(left, []);
        assert!(
            warnings
                .iter()
                .any(|w| w.code == "remove.unknown" && w.values.contains(&"zot".to_owned())),
            "{warnings:?}"
        );
    }

    #[test]
    fn a_component_left_out_is_removed_before_what_it_needs() {
        let before = planned("");
        let after = planned("disable: [metrics/victoria-metrics, logs, observability-operator]\n");
        let (removals, _) = removals(&index(), &after, &installed(&before), "single-node", &[]);
        let ids: Vec<&str> = removals.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids.last(),
            Some(&"observability-operator/victoria-metrics-operator"),
            "{ids:?}"
        );
        assert!(ids.contains(&"metrics/victoria-metrics"), "{ids:?}");
        let metrics = removals
            .iter()
            .find(|r| r.id == "metrics/victoria-metrics")
            .unwrap();
        assert_ne!(metrics.helm_charts, Vec::<String>::new());
        assert!(metrics.leftovers.iter().any(|l| l == VOLUMES));
        assert!(!metrics.loses_data);
        assert_eq!(metrics.refusal(), None);
    }

    #[test]
    fn removing_longhorn_needs_allow_data_loss_and_looks_for_its_volumes() {
        let before = planned("enable: [replicated-storage/longhorn]\n");
        let after = planned("");
        let refused = removals(&index(), &after, &installed(&before), "single-node", &[]).0;
        let longhorn = refused.iter().find(|r| r.id == LONGHORN).unwrap();
        assert!(longhorn.loses_data);
        assert!(longhorn.refusal().unwrap().contains("allowDataLoss"));
        assert_eq!(longhorn.prepare, Vec::<Vec<String>>::new());
        assert!(longhorn.in_use.contains(&InUse::Volumes {
            driver: "driver.longhorn.io".to_owned()
        }));

        let allowed = removals(
            &index(),
            &after,
            &installed(&before),
            "single-node",
            &[LONGHORN.to_owned()],
        )
        .0;
        let mut longhorn = allowed.into_iter().find(|r| r.id == LONGHORN).unwrap();
        assert_eq!(longhorn.refusal(), None);
        assert!(
            longhorn.prepare[0]
                .iter()
                .any(|a| a == LONGHORN_CONFIRMATION)
        );
        longhorn.used_by = vec!["PersistentVolume pvc-1 (claim shop/data)".to_owned()];
        assert!(longhorn.refusal().unwrap().contains("pvc-1"));
    }

    #[test]
    fn reports_an_operator_writes_are_not_use() {
        let before = planned("enable: [vulnerability-scanning]\n");
        let after = planned("");
        let removals = removals(&index(), &after, &installed(&before), "single-node", &[]).0;
        let mut scanner = removals
            .into_iter()
            .find(|r| r.id == "vulnerability-scanning/trivy-operator")
            .unwrap();
        assert_eq!(scanner.in_use, []);
        scanner.delete_crds(vec![
            "vulnerabilityreports.aquasecurity.github.io".to_owned(),
        ]);
        assert_eq!(scanner.in_use, []);
    }

    #[test]
    fn an_operator_is_in_use_while_its_objects_exist() {
        let before = planned("enable: [event-autoscaling/keda]\n");
        let after = planned("");
        let removals = removals(&index(), &after, &installed(&before), "single-node", &[]).0;
        let keda = removals
            .iter()
            .find(|r| r.id == "event-autoscaling/keda")
            .unwrap();
        assert_eq!(
            keda.in_use,
            [InUse::Objects {
                resource: "scaledobjects.keda.sh".to_owned()
            }]
        );
        assert!(!keda.loses_data);
    }

    #[test]
    fn removing_builder_removes_its_zot_and_waits_for_builds_and_images() {
        let before = planned("");
        let after = planned("disable: [builds]\n");
        let removals = removals(&index(), &after, &installed(&before), "single-node", &[]).0;
        let builder = removals
            .iter()
            .find(|r| r.id == "builds/zeabur-builder")
            .unwrap();
        assert_eq!(builder.helm_charts, ["zot", "builder"]);
        assert_eq!(builder.namespaces, ["zot", "builder-system"]);
        assert!(builder.in_use.contains(&InUse::Objects {
            resource: "builds.builder.zeabur.com".to_owned()
        }));
        assert!(builder.in_use.contains(&InUse::Images {
            registry: "zot.zot.svc:5000".to_owned()
        }));
        assert!(!builder.loses_data);
    }

    fn crd(name: &str, annotations: &serde_json::Value) -> serde_json::Value {
        json!({"metadata": {"name": name, "annotations": annotations}})
    }

    #[test]
    fn the_crds_of_a_charts_templates_are_deleted_and_their_objects_count_as_use() {
        let before = planned("enable: [resource-recommendations/vertical-pod-autoscaler]\n");
        let after = planned("");
        let installed = installed(&before);
        let mut vpa = removals(&index(), &after, &installed, "single-node", &[])
            .0
            .into_iter()
            .find(|r| r.id == "resource-recommendations/vertical-pod-autoscaler")
            .unwrap();
        let release = installed.iter().find(|i| i.component == vpa.id).unwrap();
        let owned = json!({RELEASE_NAME: release.chart, RELEASE_NAMESPACE: release.namespace});
        let other = json!({RELEASE_NAME: release.chart, RELEASE_NAMESPACE: "elsewhere"});
        let crds = [
            crd("verticalpodautoscalers.autoscaling.k8s.io", &owned),
            crd(
                "verticalpodautoscalercheckpoints.autoscaling.k8s.io",
                &owned,
            ),
            crd(
                "kept.autoscaling.k8s.io",
                &json!({RELEASE_NAME: release.chart, RELEASE_NAMESPACE: release.namespace, RESOURCE_POLICY: "keep"}),
            ),
            crd("fromcrdsfolder.autoscaling.k8s.io", &json!({})),
            crd("widgets.example.com", &other),
        ];
        let deleted = release_crds(&crds, &installed, &vpa.id);
        assert_eq!(
            deleted,
            [
                "verticalpodautoscalers.autoscaling.k8s.io",
                "verticalpodautoscalercheckpoints.autoscaling.k8s.io"
            ]
        );
        assert!(vpa.leftovers.iter().any(|l| l == CRDS));

        vpa.delete_crds(deleted);
        assert!(!vpa.leftovers.iter().any(|l| l == CRDS));
        assert_eq!(
            vpa.deleted_crds,
            [
                "verticalpodautoscalers.autoscaling.k8s.io",
                "verticalpodautoscalercheckpoints.autoscaling.k8s.io"
            ]
        );
        assert!(vpa.in_use.contains(&InUse::Objects {
            resource: "verticalpodautoscalercheckpoints.autoscaling.k8s.io".to_owned()
        }));
    }

    #[test]
    fn crds_that_helm_leaves_stay_as_the_catalog_says() {
        let before = planned("enable: [resource-recommendations/vertical-pod-autoscaler]\n");
        let after = planned("");
        let installed = installed(&before);
        let mut vpa = removals(&index(), &after, &installed, "single-node", &[])
            .0
            .into_iter()
            .find(|r| r.id == "resource-recommendations/vertical-pod-autoscaler")
            .unwrap();
        let in_use = vpa.in_use.clone();
        let deleted = release_crds(
            &[crd("verticalpodautoscalers.autoscaling.k8s.io", &json!({}))],
            &installed,
            &vpa.id,
        );
        vpa.delete_crds(deleted);
        assert!(vpa.leftovers.iter().any(|l| l == CRDS));
        assert_eq!(vpa.deleted_crds, Vec::<String>::new());
        assert_eq!(vpa.in_use, in_use);
    }

    #[test]
    fn a_component_not_in_the_catalog_stays_with_a_warning() {
        let now = planned("");
        let mut charts = installed(&now);
        charts.push(Installed {
            component: "old/thing".to_owned(),
            chart: "thing".to_owned(),
            namespace: "thing".to_owned(),
        });
        let (removals, warnings) = removals(&index(), &now, &charts, "single-node", &[]);
        assert_eq!(removals, []);
        assert!(warnings[0].text.contains("delete helmchart thing"));
    }
}
