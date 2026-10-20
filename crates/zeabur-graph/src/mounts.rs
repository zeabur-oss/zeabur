//! A container's storage.

use k8s_openapi::api::core::v1::{
    PersistentVolumeClaim, PersistentVolumeClaimSpec, Volume, VolumeMount,
};
use zeabur_model::{Mount, MountCategory, NamespaceAttribution, ObjectRef, Origin};

use crate::snapshot::{ByName, Snapshot, name_of};
use crate::workload::{Workload, find_container};

/// The volume mounts of the container called `container`, in order; `None` when the workload
/// has no such container.
///
/// Each mount is followed to its volume, and a claim on to its PVC, so size, access modes,
/// storage class and status are shown in place. A StatefulSet's claim template has no volume in
/// the pod template (the controller adds one per pod): it is described by the first ordinal's
/// PVC once that exists, and by the template before then, and keeps the template's name as its
/// claim name. A mount whose volume does not exist is kept, of kind `Unknown`, rather than
/// dropped.
#[must_use]
pub fn mounts(
    snapshot: &Snapshot,
    attribution: &NamespaceAttribution,
    workload: Workload<'_>,
    container: &str,
) -> Option<Vec<Mount>> {
    mounts_in(&ByName::new(snapshot), attribution, workload, container)
}

/// [`mounts`], looking PVCs up in `objects`.
pub(crate) fn mounts_in(
    objects: &ByName<'_>,
    attribution: &NamespaceAttribution,
    workload: Workload<'_>,
    container: &str,
) -> Option<Vec<Mount>> {
    let view = find_container(workload, container)?;
    let path = view.path(workload);
    let volumes = workload
        .pod_spec()
        .and_then(|spec| spec.volumes.as_deref())
        .unwrap_or_default();
    Some(
        view.volume_mounts
            .iter()
            .map(|mount| {
                let origin = workload.origin(format!(
                    "{path}.volumeMounts[mountPath={}]",
                    mount.mount_path
                ));
                let mut out = new_mount(mount, origin);
                if let Some(volume) = volumes.iter().find(|volume| volume.name == mount.name) {
                    describe_volume(&mut out, volume, objects, attribution);
                } else if let Some(template) = claim_template(workload, &mount.name) {
                    describe_claim_template(&mut out, template, workload, objects, attribution);
                }
                out
            })
            .collect(),
    )
}

/// A mount of unknown kind, until its volume is found.
fn new_mount(mount: &VolumeMount, origin: Origin) -> Mount {
    Mount {
        name: mount.name.clone(),
        mount_path: mount.mount_path.clone(),
        sub_path: mount.sub_path.clone().filter(|path| !path.is_empty()),
        read_only: mount.read_only == Some(true),
        kind: "Unknown".to_owned(),
        category: MountCategory::Ephemeral,
        claim_name: None,
        object: None,
        size: None,
        size_bytes: None,
        access_modes: Vec::new(),
        storage_class: None,
        volume_mode: None,
        status: None,
        attribution: None,
        origin,
    }
}

/// The StatefulSet claim template called `name`.
fn claim_template<'a>(workload: Workload<'a>, name: &str) -> Option<&'a PersistentVolumeClaim> {
    let Workload::StatefulSet(set) = workload else {
        return None;
    };
    set.spec
        .as_ref()?
        .volume_claim_templates
        .as_ref()?
        .iter()
        .find(|template| name_of(&template.metadata) == name)
}

fn describe_volume(
    mount: &mut Mount,
    volume: &Volume,
    objects: &ByName<'_>,
    attribution: &NamespaceAttribution,
) {
    let (kind, category) = if let Some(source) = &volume.persistent_volume_claim {
        mount.claim_name = Some(source.claim_name.clone());
        describe_claim(mount, &source.claim_name, objects, attribution);
        ("PersistentVolumeClaim", MountCategory::Persistent)
    } else if let Some(source) = &volume.config_map {
        mount.object = Some(ObjectRef::new("ConfigMap", &source.name));
        mount.attribution = attribution.config_maps.get(&source.name).cloned();
        ("ConfigMap", MountCategory::Config)
    } else if let Some(source) = &volume.secret {
        if let Some(name) = source.secret_name.as_deref() {
            mount.object = Some(ObjectRef::new("Secret", name));
            mount.attribution = attribution.secrets.get(name).cloned();
        }
        ("Secret", MountCategory::Config)
    } else if volume.projected.is_some() {
        ("Projected", MountCategory::Config)
    } else if volume.downward_api.is_some() {
        ("DownwardAPI", MountCategory::Config)
    } else if let Some(source) = &volume.empty_dir {
        mount.set_size(
            source
                .size_limit
                .as_ref()
                .map(|quantity| quantity.0.clone()),
        );
        ("EmptyDir", MountCategory::Ephemeral)
    } else if let Some(source) = &volume.ephemeral {
        // A generic ephemeral volume is a PVC that lives and dies with the pod.
        if let Some(template) = &source.volume_claim_template {
            describe_claim_spec(mount, &template.spec);
        }
        ("Ephemeral", MountCategory::Ephemeral)
    } else {
        ("Other", MountCategory::Ephemeral)
    };
    kind.clone_into(&mut mount.kind);
    mount.category = category;
}

/// A claim-template mount: from the first ordinal's PVC once it exists, else from the template.
fn describe_claim_template(
    mount: &mut Mount,
    template: &PersistentVolumeClaim,
    workload: Workload<'_>,
    objects: &ByName<'_>,
    attribution: &NamespaceAttribution,
) {
    "PersistentVolumeClaim".clone_into(&mut mount.kind);
    mount.category = MountCategory::Persistent;
    let template_name = name_of(&template.metadata);
    mount.claim_name = Some(template_name.to_owned());
    let first = format!("{template_name}-{}-0", workload.name());
    if objects.persistent_volume_claim(&first).is_some() {
        describe_claim(mount, &first, objects, attribution);
    } else if let Some(spec) = &template.spec {
        describe_claim_spec(mount, spec);
    }
}

/// Size, modes, class and status of the PVC called `name`, and who uses it.
fn describe_claim(
    mount: &mut Mount,
    name: &str,
    objects: &ByName<'_>,
    attribution: &NamespaceAttribution,
) {
    mount.object = Some(ObjectRef::new("PersistentVolumeClaim", name));
    mount.attribution = attribution.persistent_volume_claims.get(name).cloned();
    let Some(claim) = objects.persistent_volume_claim(name) else {
        return;
    };
    if let Some(spec) = &claim.spec {
        describe_claim_spec(mount, spec);
    }
    mount.status = claim
        .status
        .as_ref()
        .and_then(|status| status.phase.clone());
}

fn describe_claim_spec(mount: &mut Mount, spec: &PersistentVolumeClaimSpec) {
    mount.set_size(
        spec.resources
            .as_ref()
            .and_then(|resources| resources.requests.as_ref())
            .and_then(|requests| requests.get("storage"))
            .map(|quantity| quantity.0.clone()),
    );
    mount.access_modes = spec.access_modes.clone().unwrap_or_default();
    mount.storage_class.clone_from(&spec.storage_class_name);
    mount.volume_mode.clone_from(&spec.volume_mode);
}
