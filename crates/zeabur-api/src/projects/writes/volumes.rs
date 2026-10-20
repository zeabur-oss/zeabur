//! What a plan reads before it adds or grows a volume: the cluster's `StorageClasses`.

use zeabur_kube::k8s_openapi::api::storage::v1::StorageClass;
use zeabur_plan::StorageClassInfo;

/// The annotation marking the cluster's default `StorageClass` [k8s-default-class].
///
/// [k8s-default-class]: https://kubernetes.io/docs/tasks/administer-cluster/change-default-storage-class/
const DEFAULT_CLASS: &str = "storageclass.kubernetes.io/is-default-class";

pub(super) fn storage_class(class: &StorageClass) -> StorageClassInfo {
    StorageClassInfo {
        name: class.metadata.name.clone().unwrap_or_default(),
        allow_expansion: class.allow_volume_expansion == Some(true),
        default: class
            .metadata
            .annotations
            .as_ref()
            .and_then(|a| a.get(DEFAULT_CLASS))
            .is_some_and(|v| v == "true"),
    }
}
