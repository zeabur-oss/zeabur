//! Generated namespaces of any size, shared by `projection-bench` and the size tests.
//!
//! Each Deployment has its own ConfigMap, Secret, PVC, Service and Ingress, and imports one
//! ConfigMap they all share, so a namespace of `n` Deployments holds `6n + 1` objects. Each
//! Ingress has one backend, its own Service.

use serde_json::json;
use zeabur_graph::Snapshot;

/// Which workloads each Service selects.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Distribution {
    /// A Service's selector matches its own Deployment only.
    OneToOne,
    /// Every Service's selector matches every Deployment, so the relations grow with the square
    /// of the Deployments.
    Overlap,
}

/// The snapshot of `deployments` Deployments in `distribution`.
///
/// # Errors
///
/// Never for the objects it builds; [`Snapshot::insert`]'s error is passed on.
pub(crate) fn snapshot(
    distribution: Distribution,
    deployments: usize,
) -> Result<Snapshot, serde_json::Error> {
    let mut snapshot = Snapshot {
        namespace: "bench".to_owned(),
        ..Snapshot::default()
    };
    let mut objects = vec![json!({
        "apiVersion": "v1", "kind": "ConfigMap",
        "metadata": {"name": "shared", "namespace": "bench"},
        "data": {"REGION": "eu"}
    })];
    for i in 0..deployments {
        let name = format!("app-{i:05}");
        let selector = match distribution {
            Distribution::OneToOne => json!({"app": name}),
            Distribution::Overlap => json!({"tier": "web"}),
        };
        objects.push(json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": name, "namespace": "bench"},
            "spec": {
                "selector": {"matchLabels": {"app": name}},
                "template": {
                    "metadata": {"labels": {"app": name, "tier": "web"}},
                    "spec": {
                        "containers": [{
                            "name": "app",
                            "image": "registry.example/app:1.0",
                            "env": [
                                {"name": "MODE", "value": "production"},
                                {"name": "PASSWORD", "valueFrom": {"secretKeyRef": {"name": format!("{name}-secret"), "key": "password"}}}
                            ],
                            "envFrom": [
                                {"configMapRef": {"name": format!("{name}-config")}},
                                {"configMapRef": {"name": "shared"}}
                            ],
                            "volumeMounts": [{"name": "data", "mountPath": "/data"}]
                        }],
                        "volumes": [{"name": "data", "persistentVolumeClaim": {"claimName": format!("{name}-data")}}]
                    }
                }
            }
        }));
        objects.push(json!({
            "apiVersion": "v1", "kind": "ConfigMap",
            "metadata": {"name": format!("{name}-config"), "namespace": "bench"},
            "data": {"NAME": name}
        }));
        objects.push(json!({
            "apiVersion": "v1", "kind": "Secret",
            "metadata": {"name": format!("{name}-secret"), "namespace": "bench"},
            "type": "Opaque",
            "data": {"password": "c2VjcmV0"}
        }));
        objects.push(json!({
            "apiVersion": "v1", "kind": "PersistentVolumeClaim",
            "metadata": {"name": format!("{name}-data"), "namespace": "bench"},
            "spec": {"accessModes": ["ReadWriteOnce"], "resources": {"requests": {"storage": "1Gi"}}}
        }));
        objects.push(json!({
            "apiVersion": "v1", "kind": "Service",
            "metadata": {"name": name, "namespace": "bench"},
            "spec": {"selector": selector, "ports": [{"name": "http", "port": 80}]}
        }));
        objects.push(json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
            "metadata": {"name": name, "namespace": "bench"},
            "spec": {"rules": [{"host": format!("{name}.example.com"), "http": {"paths": [{
                "path": "/", "pathType": "Prefix",
                "backend": {"service": {"name": name, "port": {"number": 80}}}
            }]}}]}
        }));
    }
    for object in objects {
        snapshot.insert(object)?;
    }
    Ok(snapshot)
}
