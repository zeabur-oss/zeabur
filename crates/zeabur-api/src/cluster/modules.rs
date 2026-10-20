//! Reading what the integrations (`zeabur-modules`) need.
//!
//! Modules declare the resources they read and never read themselves: this module reads them as
//! the person, cluster-wide, discovery first (see `zeabur_kube::objects`), and hands a module the
//! objects, "not installed", or the refusal as an [`crate::views::UnreadableView`].

use zeabur_kube::{Cluster, ClusterError};

/// Reads one resource a module needs, cluster-wide, as the person.
///
/// # Errors
///
/// When kube-apiserver cannot be reached or answers with an error other than a refusal.
pub(crate) async fn read(
    cluster: &dyn Cluster,
    read: &zeabur_modules::Read,
) -> Result<zeabur_modules::Reading<crate::views::UnreadableView>, ClusterError> {
    read_in(cluster, read, None).await
}

/// Reads one resource a module needs, in `namespace` (cluster-wide for `None`), as the person.
///
/// # Errors
///
/// When kube-apiserver cannot be reached or answers with an error other than a refusal.
pub(crate) async fn read_in(
    cluster: &dyn Cluster,
    read: &zeabur_modules::Read,
    namespace: Option<&str>,
) -> Result<zeabur_modules::Reading<crate::views::UnreadableView>, ClusterError> {
    let resource = read.resource();
    match cluster.list_objects(&resource, namespace).await {
        Ok(Some(objects)) => Ok(zeabur_modules::Reading::Objects(objects)),
        Ok(None) => Ok(zeabur_modules::Reading::NotInstalled),
        Err(error) => Ok(zeabur_modules::Reading::Refused(
            error.into_unreadable(&resource.key())?.into(),
        )),
    }
}
