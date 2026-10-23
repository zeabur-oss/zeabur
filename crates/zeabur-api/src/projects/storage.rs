//! What storage a project can use and what its volumes are on: the cluster's `StorageClasses`
//! and, for each PersistentVolumeClaim, its class and the PersistentVolume behind it.
//!
//! Both are cluster-scoped and read as the person. A refusal leaves the classes empty, or the
//! claims without their volume's node, and names what was refused in `unreadable`.
//!
//! How a class behaves comes from its provisioner [k8s-sc]: k3s's local-path provisioner keeps a
//! volume in a directory of one node and pins it there with the PersistentVolume's node affinity
//! [local-path]; Longhorn keeps replicas on several nodes and runs a volume's engine on the node
//! of the pod that uses it [longhorn].
//!
//! [k8s-sc]: https://kubernetes.io/docs/concepts/storage/storage-classes/
//! [local-path]: https://github.com/rancher/local-path-provisioner/blob/master/README.md
//! [longhorn]: https://longhorn.io/docs/1.9.0/concepts/

use axum::Json;
use axum::extract::Path;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::PersistentVolumeClaim;
use zeabur_kube::k8s_openapi::api::storage::v1::StorageClass;
use zeabur_kube::{Cluster, ClusterExt, ResourceRef};
use zeabur_model::Origin;

use crate::error::ApiError;
use crate::handlers::Person;
use crate::views::UnreadableView;

/// The annotation marking the cluster's default `StorageClass` [k8s-default-class].
///
/// [k8s-default-class]: https://kubernetes.io/docs/tasks/administer-cluster/change-default-storage-class/
const DEFAULT_CLASS: &str = "storageclass.kubernetes.io/is-default-class";

/// The node label a local volume's node affinity names its node by.
const HOSTNAME: &str = "kubernetes.io/hostname";

/// What kind of disk a `StorageClass` gives, from its provisioner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum StorageKind {
    /// A directory or disk of one node (`rancher.io/local-path`, `openebs.io/local`,
    /// `kubernetes.io/no-provisioner`): a volume stays on the node it was made on.
    Local,
    /// Longhorn (`driver.longhorn.io`): copies on several nodes; a volume follows its pod.
    Replicated,
    /// Any other provisioner: what it does is up to its driver.
    Other,
}

impl StorageKind {
    fn of(provisioner: &str) -> Self {
        match provisioner {
            "rancher.io/local-path" | "openebs.io/local" | "kubernetes.io/no-provisioner" => {
                Self::Local
            }
            "driver.longhorn.io" => Self::Replicated,
            _ => Self::Other,
        }
    }
}

/// A `StorageClass`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StorageClassView {
    /// The `StorageClass`.
    pub origin: Origin,
    /// Its name.
    pub name: String,
    /// `provisioner`.
    pub provisioner: String,
    /// What kind of disk it gives.
    pub kind: StorageKind,
    /// The cluster's default (`storageclass.kubernetes.io/is-default-class`).
    pub default: bool,
    /// `allowVolumeExpansion`: its claims may grow.
    pub allow_expansion: bool,
    /// `reclaimPolicy`: what becomes of a volume when its claim is deleted (`Delete`, `Retain`).
    pub reclaim_policy: String,
    /// `volumeBindingMode`: `WaitForFirstConsumer` makes the volume where its first pod runs,
    /// `Immediate` when the claim is made.
    pub binding_mode: String,
}

/// Where one claim's data is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClaimStorage {
    /// The PersistentVolumeClaim.
    pub origin: Origin,
    /// Its name.
    pub claim: String,
    /// `spec.storageClassName`; absent when it names none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_class: Option<String>,
    /// The size it has (`status.capacity.storage`), once bound.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<String>,
    /// Its PersistentVolume (`spec.volumeName`), once bound.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    /// Whether the PersistentVolume may be used only on some nodes (a required node affinity);
    /// absent when it could not be read or there is none yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    /// The one node the PersistentVolume's node affinity names, by `kubernetes.io/hostname`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
}

/// A project's storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStorage {
    /// `projects/{project}/storage`.
    pub id: String,
    /// The cluster's `StorageClasses`, the default first; empty when they may not be listed.
    pub classes: Vec<StorageClassView>,
    /// The project's PersistentVolumeClaims, by name.
    pub claims: Vec<ClaimStorage>,
    /// What could not be read: `storageclasses.storage.k8s.io`, `persistentvolumeclaims`,
    /// `persistentvolumes`.
    pub unreadable: Vec<UnreadableView>,
}

/// The `StorageClasses` a new volume of the project can use, and what each of its claims is
/// on: the class, and the node its PersistentVolume is pinned to. Read as you: listing
/// `StorageClasses` and getting PersistentVolumes are cluster-wide rights, and a refusal is named
/// in `unreadable` instead of failing the answer.
#[utoipa::path(get, path = "/api/v1/projects/{project}/storage", tag = "projects",
    operation_id = "getProjectStorage",
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = ProjectStorage),
              (status = 400, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<ProjectStorage>, ApiError> {
    if !zeabur_plan::checks::is_dns_label(&project) {
        return Err(ApiError::from(Problem::new(
            Kind::BadRequest,
            format!("{project:?} is not a project's name."),
        )));
    }
    read(cluster.as_ref(), &project).await.map(Json)
}

async fn read(cluster: &dyn Cluster, project: &str) -> Result<ProjectStorage, ApiError> {
    let mut unreadable = Vec::new();
    let mut classes = match cluster.list::<StorageClass>(None).await {
        Ok(list) => list.iter().map(view).collect(),
        Err(error) => {
            unreadable.push(refused(error, "storageclasses.storage.k8s.io")?);
            Vec::new()
        }
    };
    classes.sort_by(|a: &StorageClassView, b| {
        b.default.cmp(&a.default).then_with(|| a.name.cmp(&b.name))
    });
    let mut claims = match cluster.list::<PersistentVolumeClaim>(Some(project)).await {
        Ok(list) => list.iter().map(|c| claim(project, c)).collect(),
        Err(error) => {
            unreadable.push(refused(error, "persistentvolumeclaims")?);
            Vec::new()
        }
    };
    claims.sort_by(|a: &ClaimStorage, b| a.claim.cmp(&b.claim));
    let volumes = ResourceRef::new("", "v1", "persistentvolumes", "PersistentVolume");
    for claim in &mut claims {
        let Some(name) = claim.volume.clone() else {
            continue;
        };
        match cluster.find_object(&volumes, None, &name).await {
            Ok(Some(volume)) => {
                let (pinned, node) = placement(&volume.data);
                claim.pinned = Some(pinned);
                claim.node = node;
            }
            Ok(None) => {}
            Err(error) => {
                unreadable.push(refused(error, "persistentvolumes")?);
                break;
            }
        }
    }
    Ok(ProjectStorage {
        id: format!("projects/{project}/storage"),
        classes,
        claims,
        unreadable,
    })
}

fn refused(error: zeabur_kube::ClusterError, resource: &str) -> Result<UnreadableView, ApiError> {
    error
        .into_unreadable(resource)
        .map(UnreadableView::from)
        .map_err(|e| ApiError::from(e.into_problem(resource)))
}

/// Whether `class` is the cluster's default.
pub(crate) fn is_default(class: &StorageClass) -> bool {
    class
        .metadata
        .annotations
        .as_ref()
        .and_then(|a| a.get(DEFAULT_CLASS))
        .is_some_and(|v| v == "true")
}

fn view(class: &StorageClass) -> StorageClassView {
    let name = class.metadata.name.clone().unwrap_or_default();
    StorageClassView {
        origin: Origin::complete("storage.k8s.io/v1", "StorageClass", None, &name),
        name,
        kind: StorageKind::of(&class.provisioner),
        provisioner: class.provisioner.clone(),
        default: is_default(class),
        allow_expansion: class.allow_volume_expansion == Some(true),
        reclaim_policy: class
            .reclaim_policy
            .clone()
            .unwrap_or_else(|| "Delete".to_owned()),
        binding_mode: class
            .volume_binding_mode
            .clone()
            .unwrap_or_else(|| "Immediate".to_owned()),
    }
}

fn claim(project: &str, claim: &PersistentVolumeClaim) -> ClaimStorage {
    let name = claim.metadata.name.clone().unwrap_or_default();
    let spec = claim.spec.as_ref();
    ClaimStorage {
        origin: Origin::complete("v1", "PersistentVolumeClaim", Some(project), &name),
        claim: name,
        storage_class: spec.and_then(|s| s.storage_class_name.clone()),
        capacity: claim
            .status
            .as_ref()
            .and_then(|s| s.capacity.as_ref())
            .and_then(|c| c.get("storage"))
            .map(|q| q.0.clone()),
        volume: spec.and_then(|s| s.volume_name.clone()),
        pinned: None,
        node: None,
    }
}

/// Whether a PersistentVolume is pinned by a required node affinity, and the node, when its only
/// term names one by hostname.
fn placement(volume: &Value) -> (bool, Option<String>) {
    let terms = volume
        .pointer("/spec/nodeAffinity/required/nodeSelectorTerms")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let node = match terms {
        [term] => term["matchExpressions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["key"] == HOSTNAME && e["operator"] == "In")
            .and_then(|e| match e["values"].as_array().map(Vec::as_slice) {
                Some([one]) => one.as_str().map(str::to_owned),
                _ => None,
            }),
        _ => None,
    };
    (!terms.is_empty(), node)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_class_kind_follows_its_provisioner() {
        assert_eq!(StorageKind::of("rancher.io/local-path"), StorageKind::Local);
        assert_eq!(
            StorageKind::of("driver.longhorn.io"),
            StorageKind::Replicated
        );
        assert_eq!(StorageKind::of("ebs.csi.aws.com"), StorageKind::Other);
    }

    #[test]
    fn a_local_volume_names_its_node_and_a_zonal_one_only_says_it_is_pinned() {
        let local = json!({"spec": {"nodeAffinity": {"required": {"nodeSelectorTerms": [
            {"matchExpressions": [{"key": HOSTNAME, "operator": "In", "values": ["vps-3"]}]}]}}}});
        assert_eq!(placement(&local), (true, Some("vps-3".to_owned())));
        let zonal = json!({"spec": {"nodeAffinity": {"required": {"nodeSelectorTerms": [
            {"matchExpressions": [{"key": "topology.kubernetes.io/zone", "operator": "In",
                "values": ["a"]}]}]}}}});
        assert_eq!(placement(&zonal), (true, None));
        assert_eq!(placement(&json!({"spec": {"csi": {}}})), (false, None));
    }
}
