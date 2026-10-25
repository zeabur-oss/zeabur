//! What a restore into a new project must tell the person before it runs: the addresses that
//! still name the original namespace, and whether the cluster has room for a second copy.
//!
//! - **Addresses.** A Service is reached in the cluster as `<service>.<namespace>.svc` and
//!   `<service>.<namespace>.svc.cluster.local` [k8s-dns]. A copy keeps every value as it was, so a
//!   variable or a ConfigMap or Secret key that names `.<original>.svc` still reaches the original.
//!   A NetworkPolicy that selects the original by its `kubernetes.io/metadata.name` label
//!   [k8s-np-ns] still lets in, or out to, the original. Each is listed. A plan does not read the
//!   values of Secrets, so it names the Secrets to check; the person reads them only by asking to
//!   change the addresses.
//! - **Room.** The copy's pods request what the original's running pods request. The room is what
//!   every ready, schedulable node has left, as a plan's fit counts it.
//!
//! [k8s-dns]: https://kubernetes.io/docs/concepts/services-networking/dns-pod-service/#namespaces-of-services
//! [k8s-np-ns]: https://kubernetes.io/docs/concepts/services-networking/network-policies/#targeting-a-namespace-by-its-name

use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_kube::k8s_openapi::api::core::v1::Node;
use zeabur_kube::{Cluster, ClusterExt as _, ClusterPod, DynamicObject, ResourceRef};

use crate::cluster::capacity::{Amount, pod_amounts};
use crate::projects::writes::preview::node_room;

/// The label kube-apiserver sets on every Namespace to its name.
const NAME_LABEL: &str = "kubernetes.io/metadata.name";
/// The most pods read to count the room.
const PODS_READ: usize = 5000;

/// A value of the original project that names the original namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = RestoreReference)]
pub struct Reference {
    /// `Deployment`, `StatefulSet`, `DaemonSet`, `CronJob`, `ConfigMap` or `NetworkPolicy`.
    pub kind: String,
    /// The object.
    pub name: String,
    /// The container, for a variable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// The variable or key; the rule list (`ingress` or `egress`) of a NetworkPolicy.
    pub key: String,
    /// The value, for a variable or a ConfigMap key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// Whether the cluster has room for the copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = RestoreFit)]
pub enum Fit {
    /// The nodes have room for what the original's pods request.
    Fits,
    /// They do not.
    Short,
    /// Nodes or pods could not be read.
    Unknown,
}

/// The room the copy needs and the room there is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = RestoreRoom)]
pub struct Room {
    /// The verdict.
    pub fit: Fit,
    /// What the original's running pods request: millicores.
    pub need_cpu: u64,
    /// Bytes.
    pub need_memory: u64,
    /// What the nodes have left: millicores.
    pub free_cpu: u64,
    /// Bytes.
    pub free_memory: u64,
}

/// What a restore into a new project reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = RestoreCopy)]
pub struct Copy {
    /// The namespace restored.
    pub source: String,
    /// The namespace it is restored into.
    pub into: String,
    /// The original no longer exists, so the backup goes back under its own name, routes
    /// included.
    pub same_name: bool,
    /// The hostnames the original serves, which stay with it until they are moved.
    pub hosts: Vec<String>,
    /// The values that name the original namespace; empty under the same name.
    pub references: Vec<Reference>,
    /// The Secrets whose values were not read, which may name it too.
    pub secrets: Vec<String>,
    /// The kinds that could not be read to look for them.
    pub unchecked: Vec<String>,
    /// Room for a second copy; absent under the same name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room: Option<Room>,
}

fn names_source(value: &str, source: &str) -> bool {
    value.contains(&format!(".{source}.svc"))
}

fn pod_spec<'a>(kind: &str, object: &'a Value) -> Option<&'a Value> {
    match kind {
        "CronJob" => object.pointer("/spec/jobTemplate/spec/template/spec"),
        _ => object.pointer("/spec/template/spec"),
    }
}

/// The Secrets a person made: not a service account's token, not a Helm release.
fn own_secrets(secrets: &[DynamicObject]) -> Vec<String> {
    let mut out: Vec<String> = secrets
        .iter()
        .filter(|s| {
            !matches!(
                s.data.get("type").and_then(Value::as_str),
                Some("kubernetes.io/service-account-token" | "helm.sh/release.v1")
            )
        })
        .filter_map(|s| s.metadata.name.clone())
        .collect();
    out.sort();
    out
}

fn selects_source(rules: Option<&Value>, peers: &str, source: &str) -> bool {
    rules
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|rule| rule.get(peers).and_then(Value::as_array))
        .flatten()
        .filter_map(|peer| peer.pointer("/namespaceSelector"))
        .any(|selector| {
            selector.pointer(&format!("/matchLabels/{}", NAME_LABEL.replace('/', "~1")))
                == Some(&Value::from(source))
                || selector["matchExpressions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|e| {
                        e["key"] == NAME_LABEL
                            && e["values"]
                                .as_array()
                                .is_some_and(|v| v.iter().any(|v| v == source))
                    })
        })
}

/// The values of `objects`, each with its kind, that name the namespace `source`.
#[must_use]
pub fn references(objects: &[(String, DynamicObject)], source: &str) -> Vec<Reference> {
    let mut out = Vec::new();
    for (kind, object) in objects {
        let Ok(object) = serde_json::to_value(object) else {
            continue;
        };
        let kind = kind.clone();
        let name = object
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let found = |container: Option<&str>, key: &str, value: Option<&str>| Reference {
            kind: kind.clone(),
            name: name.clone(),
            container: container.map(str::to_owned),
            key: key.to_owned(),
            value: value.map(str::to_owned),
        };
        match kind.as_str() {
            "ConfigMap" => {
                for (key, value) in object
                    .get("data")
                    .and_then(Value::as_object)
                    .into_iter()
                    .flatten()
                {
                    if let Some(value) = value.as_str().filter(|v| names_source(v, source)) {
                        out.push(found(None, key, Some(value)));
                    }
                }
            }
            "NetworkPolicy" => {
                for (rules, peers) in [("ingress", "from"), ("egress", "to")] {
                    if selects_source(object.pointer(&format!("/spec/{rules}")), peers, source) {
                        out.push(found(None, rules, None));
                    }
                }
            }
            _ => {
                let Some(spec) = pod_spec(&kind, &object) else {
                    continue;
                };
                let containers = ["initContainers", "containers"]
                    .into_iter()
                    .filter_map(|list| spec[list].as_array())
                    .flatten();
                for container in containers {
                    let container_name = container["name"].as_str();
                    for env in container["env"].as_array().into_iter().flatten() {
                        if let (Some(key), Some(value)) =
                            (env["name"].as_str(), env["value"].as_str())
                            && names_source(value, source)
                        {
                            out.push(found(container_name, key, Some(value)));
                        }
                    }
                }
            }
        }
    }
    out
}

/// The room the running pods of `source` need against what `room` has left.
#[must_use]
pub(crate) fn room(pods: &[ClusterPod], source: &str, room: Option<&[(Amount, u64)]>) -> Room {
    let mut need = Amount::default();
    for pod in pods.iter().filter(|p| p.namespace == source) {
        if !matches!(pod.phase.as_deref(), Some("Succeeded" | "Failed")) {
            need.add(pod_amounts(pod).0);
        }
    }
    let mut free = Amount::default();
    for (left, _) in room.unwrap_or_default() {
        free.add(*left);
    }
    let fit = match room {
        None => Fit::Unknown,
        Some(_) if need.cpu > free.cpu || need.memory > free.memory => Fit::Short,
        Some(_) => Fit::Fits,
    };
    Room {
        fit,
        need_cpu: need.cpu,
        need_memory: need.memory,
        free_cpu: free.cpu,
        free_memory: free.memory,
    }
}

/// The kinds whose values may name the namespace.
fn searched() -> [ResourceRef; 6] {
    [
        ResourceRef::new("apps", "v1", "deployments", "Deployment"),
        ResourceRef::new("apps", "v1", "statefulsets", "StatefulSet"),
        ResourceRef::new("apps", "v1", "daemonsets", "DaemonSet"),
        ResourceRef::new("batch", "v1", "cronjobs", "CronJob"),
        ResourceRef::new("", "v1", "configmaps", "ConfigMap"),
        ResourceRef::new(
            "networking.k8s.io",
            "v1",
            "networkpolicies",
            "NetworkPolicy",
        ),
    ]
}

/// The routes of a namespace.
pub(crate) fn routes() -> [ResourceRef; 2] {
    [
        ResourceRef::new("networking.k8s.io", "v1", "ingresses", "Ingress"),
        ResourceRef::new("gateway.networking.k8s.io", "v1", "httproutes", "HTTPRoute"),
    ]
}

/// Reads, as the person, what `source` holds that names it, and the room for a copy, into
/// `copy`.
pub(crate) async fn read(cluster: &dyn Cluster, copy: &mut Copy) {
    let source = copy.source.clone();
    let source = source.as_str();
    let mut objects = Vec::new();
    for resource in searched() {
        match cluster.list_objects(&resource, Some(source)).await {
            Ok(Some(listed)) => {
                objects.extend(listed.into_iter().map(|o| (resource.kind.clone(), o)));
            }
            Ok(None) => {}
            Err(_) => copy.unchecked.push(resource.key()),
        }
    }
    let secrets = ResourceRef::new("", "v1", "secrets", "Secret");
    match cluster.list_objects(&secrets, Some(source)).await {
        Ok(listed) => copy.secrets = own_secrets(&listed.unwrap_or_default()),
        Err(_) => copy.unchecked.push(secrets.key()),
    }
    let (nodes, pods) =
        futures_util::join!(cluster.list::<Node>(None), cluster.cluster_pods(PODS_READ),);
    let fit = match (nodes, pods) {
        (Ok(nodes), Ok(pods)) if !pods.truncated => {
            let left = node_room(&nodes, &pods.pods);
            room(&pods.pods, source, Some(&left))
        }
        (_, Ok(pods)) => room(&pods.pods, source, None),
        _ => room(&[], source, None),
    };
    copy.references = references(&objects, source);
    copy.room = Some(fit);
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zeabur_kube::ContainerResources;

    use super::*;

    fn object(value: Value) -> (String, DynamicObject) {
        let kind = value["kind"].as_str().unwrap().to_owned();
        (kind, serde_json::from_value(value).unwrap())
    }

    #[test]
    fn values_that_name_the_original_namespace_are_found() {
        let objects = [
            object(json!({
                "apiVersion": "apps/v1", "kind": "Deployment", "metadata": {"name": "web"},
                "spec": {"template": {"spec": {"containers": [{"name": "app", "env": [
                    {"name": "DB_HOST", "value": "postgres.blog.svc.cluster.local"},
                    {"name": "OTHER", "value": "postgres.myblog.svc"},
                    {"name": "FROM", "valueFrom": {"secretKeyRef": {"name": "s", "key": "k"}}}
                ]}]}}}
            })),
            object(json!({
                "apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "conf"},
                "data": {"url": "http://api.blog.svc:8080", "plain": "hello"}
            })),
            object(json!({
                "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
                "metadata": {"name": "allow"},
                "spec": {"ingress": [{"from": [{"namespaceSelector": {"matchLabels":
                    {"kubernetes.io/metadata.name": "blog"}}}]}]}
            })),
        ];
        let found = references(&objects, "blog");
        let keys: Vec<(&str, &str, Option<&str>)> = found
            .iter()
            .map(|r| (r.kind.as_str(), r.key.as_str(), r.value.as_deref()))
            .collect();
        assert_eq!(
            keys,
            [
                (
                    "Deployment",
                    "DB_HOST",
                    Some("postgres.blog.svc.cluster.local")
                ),
                ("ConfigMap", "url", Some("http://api.blog.svc:8080")),
                ("NetworkPolicy", "ingress", None),
            ]
        );
        assert_eq!(found[0].container.as_deref(), Some("app"));
    }

    fn pod(namespace: &str, cpu: &str, memory: &str, phase: &str) -> ClusterPod {
        ClusterPod {
            namespace: namespace.to_owned(),
            phase: Some(phase.to_owned()),
            containers: vec![ContainerResources {
                requests: [("cpu", cpu), ("memory", memory)]
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
                    .collect(),
                ..ContainerResources::default()
            }],
            ..ClusterPod::default()
        }
    }

    #[test]
    fn a_copy_needs_what_the_running_pods_request_and_fits_only_in_the_room_left() {
        let pods = [
            pod("blog", "500m", "1Gi", "Running"),
            pod("blog", "4", "8Gi", "Succeeded"),
            pod("shop", "2", "2Gi", "Running"),
        ];
        let roomy = [(
            Amount {
                cpu: 1000,
                memory: 4 << 30,
            },
            10,
        )];
        let tight = [(
            Amount {
                cpu: 400,
                memory: 4 << 30,
            },
            10,
        )];
        let fits = room(&pods, "blog", Some(&roomy));
        assert_eq!(
            (fits.fit, fits.need_cpu, fits.need_memory),
            (Fit::Fits, 500, 1 << 30)
        );
        assert_eq!(room(&pods, "blog", Some(&tight)).fit, Fit::Short);
        assert_eq!(room(&pods, "blog", None).fit, Fit::Unknown);
    }
}
