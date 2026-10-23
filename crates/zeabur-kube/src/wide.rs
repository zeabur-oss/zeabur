//! Cluster-wide reads for the cluster overview: nodes, every pod (slim) and every event.
//!
//! # Pods are read in pages and decoded slim
//!
//! A cluster-wide pod list is the most expensive read Zeabur makes. It is read the way
//! `kubectl describe node` reads it [kubectl-describe]: with the field selector
//! `status.phase!=Succeeded,status.phase!=Failed` (finished pods hold no resources) and in
//! chunks of 500 with `limit`/`continue` [k8s-chunking], so kube-apiserver never builds one huge
//! answer. Each page is decoded straight into [`ClusterPod`], which keeps only the node, the
//! phase and the containers' requests and limits; the rest of each pod (environment, volumes,
//! status) is skipped by the decoder instead of being held as full `Pod` objects. The read
//! stops after a limit the caller gives (5,000 pods for the overview) and says so
//! ([`ClusterPods::truncated`]), so a very large cluster gives a partial sum rather than a slow
//! page.
//!
//! [kubectl-describe]: https://github.com/kubernetes/kubectl/blob/master/pkg/describe/describe.go
//! [k8s-chunking]: https://kubernetes.io/docs/reference/using-api/api-concepts/#retrieving-large-results-sets-in-chunks

use std::collections::BTreeMap;

use k8s_openapi::api::events::v1::Event;
use kube::api::ListParams;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::port::{ClusterError, ClusterEvents, ClusterPod, ClusterPods, ContainerResources};

/// The annotation the kubelet puts on a static pod's mirror pod.
pub(crate) const MIRROR: &str = "kubernetes.io/config.mirror";

/// Items per page of a cluster-wide list.
const PAGE: u32 = 500;

/// One page of a list, decoded into `T`.
#[derive(Deserialize)]
struct Page<T> {
    #[serde(default)]
    metadata: PageMeta,
    #[serde(default = "Vec::new")]
    items: Vec<T>,
}

#[derive(Deserialize, Default)]
struct PageMeta {
    #[serde(default, rename = "continue")]
    next: Option<String>,
}

/// Reads `path` (a collection) page by page until the end or until `limit` items; `true` when
/// the limit stopped it.
async fn paged<T: DeserializeOwned>(
    client: &kube::Client,
    path: &str,
    fields: Option<&str>,
    limit: usize,
) -> Result<(Vec<T>, bool), ClusterError> {
    let request = kube::core::Request::new(path);
    let mut items = Vec::new();
    let mut next: Option<String> = None;
    loop {
        let mut params = ListParams::default().limit(PAGE);
        if let Some(fields) = fields {
            params = params.fields(fields);
        }
        if let Some(token) = &next {
            params = params.continue_token(token);
        }
        let http = request
            .list(&params)
            .map_err(|e| ClusterError::Transport(e.to_string()))?;
        let page: Page<T> = client.request(http).await?;
        items.extend(page.items);
        next = page.metadata.next.filter(|t| !t.is_empty());
        // A page can hold more than the limit (pages are `PAGE` long), so cut it here too.
        if items.len() > limit {
            items.truncate(limit);
            return Ok((items, true));
        }
        if next.is_none() {
            return Ok((items, false));
        }
        if items.len() == limit {
            return Ok((items, true));
        }
    }
}

/// A pod as the slim decoder sees it; every other field is skipped.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RawPod {
    #[serde(default)]
    metadata: RawMeta,
    #[serde(default)]
    spec: RawPodSpec,
    #[serde(default)]
    status: RawPodStatus,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawMeta {
    #[serde(default)]
    name: String,
    #[serde(default)]
    namespace: String,
    #[serde(default)]
    owner_references: Vec<RawOwner>,
    #[serde(default)]
    annotations: BTreeMap<String, String>,
    #[serde(default)]
    deletion_timestamp: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawOwner {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    controller: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawPodSpec {
    #[serde(default)]
    node_name: Option<String>,
    #[serde(default)]
    containers: Vec<RawContainer>,
    #[serde(default)]
    init_containers: Vec<RawContainer>,
    #[serde(default)]
    overhead: BTreeMap<String, String>,
    #[serde(default)]
    resources: Option<RawResources>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawContainer {
    #[serde(default)]
    resources: RawResources,
    #[serde(default)]
    restart_policy: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawResources {
    #[serde(default)]
    requests: BTreeMap<String, String>,
    #[serde(default)]
    limits: BTreeMap<String, String>,
}

#[derive(Deserialize, Default)]
struct RawPodStatus {
    #[serde(default)]
    phase: Option<String>,
}

impl From<RawPod> for ClusterPod {
    fn from(raw: RawPod) -> Self {
        let container = |c: RawContainer| ContainerResources {
            requests: c.resources.requests,
            limits: c.resources.limits,
            sidecar: c.restart_policy.as_deref() == Some("Always"),
        };
        let controller = raw
            .metadata
            .owner_references
            .into_iter()
            .find(|o| o.controller == Some(true))
            .map(|o| o.kind);
        Self {
            controller,
            mirror: raw.metadata.annotations.contains_key(MIRROR),
            terminating: raw.metadata.deletion_timestamp.is_some(),
            namespace: raw.metadata.namespace,
            name: raw.metadata.name,
            node_name: raw.spec.node_name.filter(|n| !n.is_empty()),
            phase: raw.status.phase,
            containers: raw.spec.containers.into_iter().map(container).collect(),
            init_containers: raw
                .spec
                .init_containers
                .into_iter()
                .map(container)
                .collect(),
            overhead: raw.spec.overhead,
            pod_resources: raw.spec.resources.map(|r| ContainerResources {
                requests: r.requests,
                limits: r.limits,
                sidecar: false,
            }),
        }
    }
}

/// A pod as JSON (from kube-apiserver or a test) reduced to a [`ClusterPod`].
///
/// # Errors
///
/// When the JSON is not a pod.
#[cfg(any(test, feature = "memory"))]
pub(crate) fn slim_pod(value: serde_json::Value) -> Result<ClusterPod, ClusterError> {
    serde_json::from_value::<RawPod>(value)
        .map(ClusterPod::from)
        .map_err(|e| ClusterError::Decode(e.to_string()))
}

pub(crate) async fn cluster_pods(
    client: &kube::Client,
    limit: usize,
) -> Result<ClusterPods, ClusterError> {
    let (pods, truncated) = paged::<RawPod>(
        client,
        "/api/v1/pods",
        Some("status.phase!=Succeeded,status.phase!=Failed"),
        limit,
    )
    .await?;
    Ok(ClusterPods {
        pods: pods.into_iter().map(ClusterPod::from).collect(),
        truncated,
    })
}

pub(crate) async fn cluster_events(
    client: &kube::Client,
    limit: usize,
) -> Result<ClusterEvents, ClusterError> {
    let (events, truncated) =
        paged::<Event>(client, "/apis/events.k8s.io/v1/events", None, limit).await?;
    Ok(ClusterEvents { events, truncated })
}

/// Whether kube-apiserver serves `group_version` (`GET /apis/{group}/{version}`, or
/// `/api/{version}` for the core group): `Some(false)`
/// for a `404`, `None` when discovery itself is refused.
///
/// kube-apiserver authorizes a request before it looks the resource up, so listing a resource
/// whose API is not installed answers `403`, not `404`, to someone without the permission
/// [authz]. Discovery is readable by every authenticated person through the default
/// `system:discovery` role [discovery-rbac], so asking it first tells "not installed" apart from
/// "not allowed".
///
/// [authz]: https://kubernetes.io/docs/reference/access-authn-authz/authorization/
/// [discovery-rbac]: https://kubernetes.io/docs/reference/access-authn-authz/rbac/#discovery-roles
pub(crate) async fn serves(
    client: &kube::Client,
    group_version: &str,
) -> Result<Option<bool>, ClusterError> {
    // The core group has no group name and lives under `/api/{version}`.
    let answer = if group_version.contains('/') {
        client.list_api_group_resources(group_version).await
    } else {
        client.list_core_api_resources(group_version).await
    };
    match answer {
        Ok(_) => Ok(Some(true)),
        Err(error) => match ClusterError::from(error) {
            error if error.is_not_found() => Ok(Some(false)),
            error if error.is_refused() => Ok(None),
            other => Err(other),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pod_is_decoded_slim_with_sidecars_and_pod_level_resources() {
        let pod = slim_pod(serde_json::json!({
            "metadata": {"name": "web-1", "namespace": "shop", "labels": {"app": "web"}},
            "spec": {
                "nodeName": "node-1",
                "containers": [{"name": "web", "image": "nginx",
                    "env": [{"name": "A", "value": "b"}],
                    "resources": {"requests": {"cpu": "100m", "memory": "128Mi"}}}],
                "initContainers": [
                    {"name": "migrate", "image": "x", "resources": {"requests": {"cpu": "1"}}},
                    {"name": "proxy", "image": "y", "restartPolicy": "Always",
                     "resources": {"requests": {"cpu": "50m"}}}
                ],
                "overhead": {"cpu": "10m"},
                "resources": {"limits": {"memory": "1Gi"}}
            },
            "status": {"phase": "Running", "containerStatuses": []}
        }))
        .unwrap();
        assert_eq!(pod.node_name.as_deref(), Some("node-1"));
        assert_eq!(pod.containers[0].requests["cpu"], "100m");
        assert!(!pod.init_containers[0].sidecar && pod.init_containers[1].sidecar);
        assert_eq!(pod.overhead["cpu"], "10m");
        assert_eq!(pod.pod_resources.unwrap().limits["memory"], "1Gi");
        assert_eq!(
            (pod.controller, pod.mirror, pod.terminating),
            (None, false, false)
        );
    }

    #[test]
    fn a_pods_controller_mirror_and_deletion_are_kept() {
        let pod = slim_pod(serde_json::json!({
            "metadata": {"name": "canal-x", "namespace": "kube-system",
                "deletionTimestamp": "2026-10-23T10:00:00Z",
                "annotations": {MIRROR: "abc"},
                "ownerReferences": [
                    {"apiVersion": "v1", "kind": "Node", "name": "n", "uid": "1"},
                    {"apiVersion": "apps/v1", "kind": "DaemonSet", "name": "canal", "uid": "2",
                     "controller": true}
                ]},
            "spec": {"containers": []}
        }))
        .unwrap();
        assert_eq!(pod.controller.as_deref(), Some("DaemonSet"));
        assert!(pod.mirror && pod.terminating);
    }
}
