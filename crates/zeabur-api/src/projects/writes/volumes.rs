//! What a plan reads before it adds or grows a volume: the cluster's `StorageClasses`.

use zeabur_kube::k8s_openapi::api::storage::v1::StorageClass;
use zeabur_plan::StorageClassInfo;

use crate::projects::storage::is_default;

pub(super) fn storage_class(class: &StorageClass) -> StorageClassInfo {
    StorageClassInfo {
        name: class.metadata.name.clone().unwrap_or_default(),
        allow_expansion: class.allow_volume_expansion == Some(true),
        default: is_default(class),
    }
}
