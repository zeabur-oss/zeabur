//! A container's environment.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{ConfigMap, EnvFromSource, EnvVar as KubeEnvVar};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use zeabur_model::adapters::FIELD_MANAGER;
use zeabur_model::adapters::provenance::detect_tool;
use zeabur_model::{EnvVar, ImportedEnv, ReadOnlyReason, Source, SourceKind};

use crate::fields::{Step, reaches};
use crate::snapshot::{ByName, Snapshot};
use crate::workload::{Workload, find_container};

/// The `env` entries of the container called `container`, in order; `None` when the workload
/// has no such container.
///
/// A Secret-backed entry is masked and carries no value: the projection never holds one.
/// Revealing it is a separate read, made with the person's own credential.
#[must_use]
pub fn env(workload: Workload<'_>, container: &str) -> Option<Vec<EnvVar>> {
    let view = find_container(workload, container)?;
    let path = view.path(workload);
    Some(
        view.env
            .iter()
            .map(|variable| EnvVar {
                name: variable.name.clone(),
                value: if variable.value_from.is_none() {
                    variable.value.clone()
                } else {
                    None
                },
                source: env_source(variable),
                masked: variable
                    .value_from
                    .as_ref()
                    .is_some_and(|from| from.secret_key_ref.is_some()),
                pending_restart: None,
                origin: workload.origin(format!("{path}.env[name={}]", variable.name)),
            })
            .collect(),
    )
}

/// Where one `env` entry's value comes from.
fn env_source(variable: &KubeEnvVar) -> Source {
    let Some(from) = variable.value_from.as_ref() else {
        return Source::new(SourceKind::Literal);
    };
    // kube-apiserver requires exactly one selector in `valueFrom`; with none there is no
    // reference to report, and no value either.
    let secret = from.secret_key_ref.as_ref().map(|reference| Source {
        name: Some(reference.name.clone()),
        key: Some(reference.key.clone()),
        optional: reference.optional == Some(true),
        ..Source::new(SourceKind::Secret)
    });
    secret
        .or_else(|| {
            from.config_map_key_ref.as_ref().map(|reference| Source {
                name: Some(reference.name.clone()),
                key: Some(reference.key.clone()),
                optional: reference.optional == Some(true),
                ..Source::new(SourceKind::ConfigMap)
            })
        })
        .or_else(|| {
            from.field_ref.as_ref().map(|reference| Source {
                field_path: Some(reference.field_path.clone()),
                ..Source::new(SourceKind::PodField)
            })
        })
        .or_else(|| {
            from.resource_field_ref.as_ref().map(|reference| Source {
                name: reference.container_name.clone(),
                field_path: Some(reference.resource.clone()),
                ..Source::new(SourceKind::ResourceField)
            })
        })
        .or_else(|| {
            from.file_key_ref.as_ref().map(|reference| Source {
                name: Some(reference.volume_name.clone()),
                key: Some(reference.key.clone()),
                field_path: Some(reference.path.clone()),
                optional: reference.optional == Some(true),
                ..Source::new(SourceKind::File)
            })
        })
        .unwrap_or_else(|| Source::new(SourceKind::Literal))
}

/// The `envFrom` imports of the container called `container`, in order; `None` when the
/// workload has no such container.
///
/// An import is not flattened into variables. `keyCount` is the number of keys the ConfigMap
/// holds, or for a Secret the count in [`Snapshot::secret_key_counts`]; a Secret is never read.
///
/// An import is read-only when another field manager owns the container's `envFrom` (the list
/// is atomic, so changing one entry takes the whole list), or when the imported object is
/// managed by a GitOps tool or Helm (its contents are restored on the next sync).
#[must_use]
pub fn env_from(
    snapshot: &Snapshot,
    workload: Workload<'_>,
    container: &str,
) -> Option<Vec<ImportedEnv>> {
    env_from_in(
        snapshot,
        &ByName::new(snapshot),
        workload,
        container,
        FIELD_MANAGER,
    )
}

/// [`env_from`], looking the imported objects up in `objects`; `field_manager` is the one whose
/// own writes never make the list read-only.
pub(crate) fn env_from_in(
    snapshot: &Snapshot,
    objects: &ByName<'_>,
    workload: Workload<'_>,
    container: &str,
    field_manager: &str,
) -> Option<Vec<ImportedEnv>> {
    let view = find_container(workload, container)?;
    let path = view.path(workload);
    let list_manager = list_managers(workload, view.list, view.name)
        .into_iter()
        .find(|manager| *manager != field_manager);
    Some(
        view.env_from
            .iter()
            .enumerate()
            .filter_map(|(index, import)| {
                let (source, key_count, meta) = import_source(snapshot, objects, import)?;
                let read_only_reason = list_manager
                    .map(|manager| ReadOnlyReason::ListManagedBy {
                        manager: manager.to_owned(),
                    })
                    .or_else(|| {
                        meta.and_then(detect_tool)
                            .map(|tool| ReadOnlyReason::SourceManagedBy { tool })
                    });
                Some(ImportedEnv {
                    source,
                    prefix: import.prefix.clone().filter(|prefix| !prefix.is_empty()),
                    key_count,
                    read_only: read_only_reason.is_some(),
                    read_only_reason,
                    pending_restart: None,
                    origin: workload.origin(format!("{path}.envFrom[{index}]")),
                })
            })
            .collect(),
    )
}

/// The imported object, how many keys it holds, and its metadata when it is in the snapshot.
fn import_source<'a>(
    snapshot: &Snapshot,
    objects: &ByName<'a>,
    import: &EnvFromSource,
) -> Option<(Source, Option<usize>, Option<&'a ObjectMeta>)> {
    // kube-apiserver requires one of the two references; an entry with neither imports nothing.
    let config_map = import.config_map_ref.as_ref().map(|reference| {
        let object = objects.config_map(&reference.name);
        let source = Source {
            name: Some(reference.name.clone()),
            optional: reference.optional == Some(true),
            ..Source::new(SourceKind::ConfigMap)
        };
        (
            source,
            object.map(key_count),
            object.map(|object| &object.metadata),
        )
    });
    config_map.or_else(|| {
        import.secret_ref.as_ref().map(|reference| {
            let source = Source {
                name: Some(reference.name.clone()),
                optional: reference.optional == Some(true),
                ..Source::new(SourceKind::Secret)
            };
            let count = snapshot.secret_key_counts.get(&reference.name).copied();
            let meta = objects
                .secret(&reference.name)
                .map(|object| &object.metadata);
            (source, count, meta)
        })
    })
}

/// How many keys a ConfigMap holds, text and binary.
pub(crate) fn key_count(config_map: &ConfigMap) -> usize {
    config_map.data.as_ref().map_or(0, BTreeMap::len)
        + config_map.binary_data.as_ref().map_or(0, BTreeMap::len)
}

/// The field managers that own a container's `envFrom` list, from the workload's
/// `managedFields`.
///
/// `managedFields` addresses an entry of a list merged by key with the key as a JSON object:
/// the container `app` is `k:{"name":"app"}` under `f:containers`.
fn list_managers<'a>(workload: Workload<'a>, list: &str, container: &str) -> Vec<&'a str> {
    let mut path: Vec<Step> = workload
        .kind()
        .pod_template_path()
        .iter()
        .chain(&["spec", list])
        .map(|field| Step::field(field))
        .collect();
    path.extend([Step::named(container), Step::field("envFrom")]);
    workload
        .meta()
        .managed_fields
        .iter()
        .flatten()
        .filter(|entry| {
            entry
                .fields_v1
                .as_ref()
                .is_some_and(|fields| reaches(&fields.0, &path))
        })
        .filter_map(|entry| entry.manager.as_deref())
        .collect()
}
