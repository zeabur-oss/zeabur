//! What reading a project's overview costs, run by hand (see the `zeabur-overview-cost` skill)
//! and never in a pull request's checks.
//!
//! # The request set
//!
//! One overview read of a namespace is `namespace_objects` (twelve LISTs: Deployments,
//! StatefulSets, DaemonSets, Jobs, CronJobs, Pods, Services, ConfigMaps, the Secrets Table,
//! PersistentVolumeClaims, Ingresses and HTTPRoutes) and the namespace's Events
//! (`list::<Event>`). The custom resources the page also asks about (builds, network policies)
//! are left out, since a cluster without them answers a `404` that costs almost nothing.
//!
//! # The layout
//!
//! A fake kube-apiserver on a loopback port answers each LIST with a prebuilt body, so the
//! numbers are Zeabur's side of the read and not the server's. The baseline namespace holds
//! [`BASELINE`] objects of each kind, every object about the size `kubectl get -o json` prints
//! (with `managedFields`). The 10x run multiplies each count by ten.
//!
//! # The allocation scope
//!
//! Everything the thread that drives the client allocates between the start and the end of the
//! read: the request, the response bytes, the decoding and the typed results, which are dropped
//! at the end. The fake server runs on other threads and is not counted. Peak is the most bytes
//! alive at one moment.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Write as _;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    use secrecy::SecretString;
    use serde_json::{Value, json};
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use zeabur_kube::k8s_openapi::api::events::v1::Event;
    use zeabur_kube::recording::Recording;
    use zeabur_kube::{Cluster, ClusterEndpoint, ClusterExt as _, UserClient, UserToken};
    use zeabur_test_support::allocations::{Allocations, measure};

    const NAMESPACE: &str = "shop";

    /// How many objects of each kind the baseline namespace holds.
    const BASELINE: [(&str, usize); 13] = [
        ("deployments", 12),
        ("statefulsets", 3),
        ("daemonsets", 1),
        ("jobs", 6),
        ("cronjobs", 3),
        ("pods", 40),
        ("services", 20),
        ("configmaps", 20),
        ("secrets", 15),
        ("persistentvolumeclaims", 8),
        ("ingresses", 6),
        ("httproutes", 6),
        ("events", 100),
    ];

    fn meta(name: &str, kind: &str, i: usize) -> Value {
        json!({
            "name": format!("{name}-{i}"),
            "namespace": NAMESPACE,
            "uid": format!("6f1c2d3e-0000-4000-8000-{i:012}"),
            "resourceVersion": (100_000 + i).to_string(),
            "creationTimestamp": "2026-11-01T08:30:00Z",
            "labels": {
                "app.kubernetes.io/name": name,
                "app.kubernetes.io/instance": format!("{name}-{i}"),
                "app.kubernetes.io/managed-by": "Helm",
                "zeabur.dev/project": NAMESPACE,
            },
            "annotations": {
                "meta.helm.sh/release-name": name,
                "meta.helm.sh/release-namespace": NAMESPACE,
                "deployment.kubernetes.io/revision": "3",
            },
            "managedFields": [{
                "manager": "helm", "operation": "Update", "apiVersion": "v1",
                "time": "2026-11-01T08:30:00Z", "fieldsType": "FieldsV1",
                "fieldsV1": {"f:metadata": {"f:labels": {".": {}, "f:app.kubernetes.io/name": {}},
                    "f:annotations": {".": {}, "f:meta.helm.sh/release-name": {}}},
                    "f:spec": {"f:replicas": {}, "f:selector": {}, "f:template": {"f:spec": {
                        "f:containers": {"k:{\"name\":\"app\"}": {".": {}, "f:image": {}, "f:env": {}}}}}}}
            }],
            "ownerReferences": [{"apiVersion": "v1", "kind": kind, "name": name, "uid": "owner", "controller": true}],
        })
    }

    fn container(name: &str) -> Value {
        json!({
            "name": "app", "image": format!("registry.example.com/{name}:1.4.2"),
            "ports": [{"containerPort": 8080, "protocol": "TCP"}],
            "env": (0..6).map(|n| json!({"name": format!("SETTING_{n}"), "value": format!("value-{n}")})).collect::<Vec<_>>(),
            "resources": {"requests": {"cpu": "100m", "memory": "128Mi"}, "limits": {"memory": "256Mi"}},
            "readinessProbe": {"httpGet": {"path": "/healthz", "port": 8080}, "periodSeconds": 10},
            "volumeMounts": [{"name": "data", "mountPath": "/data"}],
            "imagePullPolicy": "IfNotPresent",
        })
    }

    fn pod_spec(name: &str) -> Value {
        json!({
            "containers": [container(name)],
            "volumes": [{"name": "data", "persistentVolumeClaim": {"claimName": format!("{name}-data")}}],
            "restartPolicy": "Always", "terminationGracePeriodSeconds": 30, "dnsPolicy": "ClusterFirst",
            "serviceAccountName": "default", "schedulerName": "default-scheduler",
        })
    }

    fn workload(kind: &str, api: &str, name: &str, i: usize) -> Value {
        json!({
            "metadata": meta(name, kind, i),
            "spec": {
                "replicas": 2,
                "selector": {"matchLabels": {"app.kubernetes.io/name": name}},
                "template": {"metadata": {"labels": {"app.kubernetes.io/name": name}}, "spec": pod_spec(name)},
            },
            "status": {"replicas": 2, "readyReplicas": 2, "availableReplicas": 2, "updatedReplicas": 2,
                "observedGeneration": 3,
                "conditions": [{"type": "Available", "status": "True", "lastUpdateTime": "2026-11-01T08:31:00Z",
                    "lastTransitionTime": "2026-11-01T08:31:00Z", "reason": "MinimumReplicasAvailable",
                    "message": "Deployment has minimum availability."}]},
            "apiVersion": api, "kind": kind,
        })
    }

    fn item(kind: &str, i: usize) -> Value {
        let name = format!("{}-{}", kind.to_lowercase(), i % 7);
        match kind {
            "deployments" => workload("Deployment", "apps/v1", &name, i),
            "statefulsets" => workload("StatefulSet", "apps/v1", &name, i),
            "daemonsets" => workload("DaemonSet", "apps/v1", &name, i),
            "jobs" => {
                let mut job = workload("Job", "batch/v1", &name, i);
                job["spec"] = json!({"completions": 1, "parallelism": 1, "backoffLimit": 4,
                    "template": {"spec": pod_spec(&name)}});
                job["status"] = json!({"succeeded": 1, "completionTime": "2026-11-01T09:00:00Z",
                    "startTime": "2026-11-01T08:59:00Z"});
                job
            }
            "cronjobs" => {
                let mut cron = workload("CronJob", "batch/v1", &name, i);
                cron["spec"] = json!({"schedule": "*/15 * * * *", "concurrencyPolicy": "Forbid",
                    "jobTemplate": {"spec": {"template": {"spec": pod_spec(&name)}}}});
                cron["status"] = json!({"lastScheduleTime": "2026-11-01T09:00:00Z"});
                cron
            }
            "pods" => json!({
                "metadata": meta(&name, "ReplicaSet", i),
                "spec": pod_spec(&name),
                "status": {"phase": "Running", "hostIP": "10.0.0.5", "podIP": "10.42.0.17",
                    "startTime": "2026-11-01T08:30:00Z",
                    "conditions": [{"type": "Ready", "status": "True", "lastTransitionTime": "2026-11-01T08:31:00Z"}],
                    "containerStatuses": [{"name": "app", "ready": true, "restartCount": 0, "started": true,
                        "image": format!("registry.example.com/{name}:1.4.2"),
                        "imageID": "registry.example.com/app@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                        "containerID": "containerd://0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                        "state": {"running": {"startedAt": "2026-11-01T08:30:30Z"}}}]},
                "apiVersion": "v1", "kind": "Pod",
            }),
            "services" => json!({
                "metadata": meta(&name, "Service", i),
                "spec": {"type": "ClusterIP", "clusterIP": "10.43.0.20", "selector": {"app.kubernetes.io/name": name},
                    "ports": [{"name": "http", "port": 80, "targetPort": 8080, "protocol": "TCP"}]},
                "status": {"loadBalancer": {}}, "apiVersion": "v1", "kind": "Service",
            }),
            "configmaps" => json!({
                "metadata": meta(&name, "ConfigMap", i),
                "data": {"app.conf": "x".repeat(1500), "mode": "fast"},
                "apiVersion": "v1", "kind": "ConfigMap",
            }),
            "persistentvolumeclaims" => json!({
                "metadata": meta(&name, "PersistentVolumeClaim", i),
                "spec": {"accessModes": ["ReadWriteOnce"], "resources": {"requests": {"storage": "10Gi"}},
                    "storageClassName": "local-path", "volumeName": format!("pvc-{i}"), "volumeMode": "Filesystem"},
                "status": {"phase": "Bound", "accessModes": ["ReadWriteOnce"], "capacity": {"storage": "10Gi"}},
                "apiVersion": "v1", "kind": "PersistentVolumeClaim",
            }),
            "ingresses" => json!({
                "metadata": meta(&name, "Ingress", i),
                "spec": {"ingressClassName": "traefik", "rules": [{"host": format!("{name}.example.com"),
                    "http": {"paths": [{"path": "/", "pathType": "Prefix",
                        "backend": {"service": {"name": name, "port": {"number": 80}}}}]}}],
                    "tls": [{"hosts": [format!("{name}.example.com")], "secretName": format!("{name}-tls")}]},
                "status": {"loadBalancer": {"ingress": [{"ip": "10.0.0.5"}]}},
                "apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
            }),
            "httproutes" => json!({
                "metadata": meta(&name, "HTTPRoute", i),
                "spec": {"parentRefs": [{"name": "gateway", "namespace": "gateway-system"}],
                    "hostnames": [format!("{name}.example.com")],
                    "rules": [{"matches": [{"path": {"type": "PathPrefix", "value": "/"}}],
                        "backendRefs": [{"name": name, "port": 80}]}]},
                "status": {"parents": [{"parentRef": {"name": "gateway"}, "controllerName": "example.com/gateway",
                    "conditions": [{"type": "Accepted", "status": "True", "reason": "Accepted",
                        "lastTransitionTime": "2026-11-01T08:31:00Z", "observedGeneration": 1, "message": ""}]}]},
                "apiVersion": "gateway.networking.k8s.io/v1", "kind": "HTTPRoute",
            }),
            "events" => json!({
                "metadata": {"name": format!("{name}.{i:x}"), "namespace": NAMESPACE,
                    "uid": format!("7a1c2d3e-0000-4000-8000-{i:012}"), "resourceVersion": (200_000 + i).to_string(),
                    "creationTimestamp": "2026-11-01T08:30:00Z"},
                "eventTime": "2026-11-01T08:30:00.000000Z", "reportingController": "kubelet",
                "reportingInstance": "node-1", "action": "Pulling", "reason": "Pulled", "type": "Normal",
                "regarding": {"kind": "Pod", "namespace": NAMESPACE, "name": name, "uid": "owner",
                    "apiVersion": "v1", "resourceVersion": "1", "fieldPath": "spec.containers{app}"},
                "note": "Successfully pulled image \"registry.example.com/app:1.4.2\" in 1.2s (1.2s including waiting)",
                "apiVersion": "events.k8s.io/v1", "kind": "Event",
            }),
            other => unreachable!("{other}"),
        }
    }

    fn list_body(kind: &str, count: usize, api_version: &str) -> Vec<u8> {
        let items: Vec<Value> = (0..count).map(|i| item(kind, i)).collect();
        serde_json::to_vec(&json!({
            "apiVersion": api_version, "kind": "List", "metadata": {"resourceVersion": "100000"}, "items": items,
        }))
        .unwrap()
    }

    /// The Secrets Table `includeObject=Metadata` answers: no data, a count in a column.
    fn secrets_table(count: usize) -> Vec<u8> {
        let rows: Vec<Value> = (0..count)
            .map(|i| {
                json!({
                    "cells": [format!("secret-{i}"), "Opaque", 3, "10d"],
                    "object": {"kind": "PartialObjectMetadata", "apiVersion": "meta.k8s.io/v1",
                        "metadata": meta("secret", "Secret", i)},
                })
            })
            .collect();
        serde_json::to_vec(&json!({
            "kind": "Table", "apiVersion": "meta.k8s.io/v1", "metadata": {"resourceVersion": "100000"},
            "columnDefinitions": [{"name": "Name"}, {"name": "Type"}, {"name": "Data"}, {"name": "Age"}],
            "rows": rows,
        }))
        .unwrap()
    }

    fn routes(scale: usize) -> HashMap<String, Arc<Vec<u8>>> {
        let count = |kind: &str| {
            BASELINE
                .iter()
                .find(|(k, _)| *k == kind)
                .map_or(0, |(_, n)| n * scale)
        };
        let ns = format!("/namespaces/{NAMESPACE}");
        let mut routes = HashMap::new();
        for (path, kind, api) in [
            (
                format!("/apis/apps/v1{ns}/deployments"),
                "deployments",
                "apps/v1",
            ),
            (
                format!("/apis/apps/v1{ns}/statefulsets"),
                "statefulsets",
                "apps/v1",
            ),
            (
                format!("/apis/apps/v1{ns}/daemonsets"),
                "daemonsets",
                "apps/v1",
            ),
            (format!("/apis/batch/v1{ns}/jobs"), "jobs", "batch/v1"),
            (
                format!("/apis/batch/v1{ns}/cronjobs"),
                "cronjobs",
                "batch/v1",
            ),
            (format!("/api/v1{ns}/pods"), "pods", "v1"),
            (format!("/api/v1{ns}/services"), "services", "v1"),
            (format!("/api/v1{ns}/configmaps"), "configmaps", "v1"),
            (
                format!("/api/v1{ns}/persistentvolumeclaims"),
                "persistentvolumeclaims",
                "v1",
            ),
            (
                format!("/apis/networking.k8s.io/v1{ns}/ingresses"),
                "ingresses",
                "networking.k8s.io/v1",
            ),
            (
                format!("/apis/gateway.networking.k8s.io/v1{ns}/httproutes"),
                "httproutes",
                "gateway.networking.k8s.io/v1",
            ),
            (
                format!("/apis/events.k8s.io/v1{ns}/events"),
                "events",
                "events.k8s.io/v1",
            ),
        ] {
            routes.insert(path, Arc::new(list_body(kind, count(kind), api)));
        }
        routes.insert(
            format!("/api/v1{ns}/secrets"),
            Arc::new(secrets_table(count("secrets"))),
        );
        routes
    }

    struct Server {
        uri: http::Uri,
        requests: Arc<AtomicUsize>,
        bytes: Arc<AtomicUsize>,
    }

    /// Serves `routes` on a loopback port from threads of its own.
    fn serve(routes: HashMap<String, Arc<Vec<u8>>>) -> Server {
        let (requests, bytes) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let (ready, port) = std::sync::mpsc::channel();
        let (counted, sized) = (Arc::clone(&requests), Arc::clone(&bytes));
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                ready.send(listener.local_addr().unwrap().port()).unwrap();
                let routes = Arc::new(routes);
                loop {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let (routes, counted, sized) =
                        (Arc::clone(&routes), Arc::clone(&counted), Arc::clone(&sized));
                    tokio::spawn(async move {
                        let mut pending = Vec::new();
                        let mut buffer = [0; 4096];
                        loop {
                            while !pending.windows(4).any(|w| w == b"\r\n\r\n") {
                                let n = socket.read(&mut buffer).await.unwrap_or(0);
                                if n == 0 {
                                    return;
                                }
                                pending.extend_from_slice(&buffer[..n]);
                            }
                            let end = pending.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
                            let head = String::from_utf8_lossy(&pending[..end]).to_string();
                            pending.drain(..end);
                            counted.fetch_add(1, Ordering::Relaxed);
                            let path = head
                                .split_whitespace()
                                .nth(1)
                                .and_then(|p| p.split('?').next())
                                .unwrap_or_default();
                            let body = routes.get(path).map_or_else(
                                || b"{\"kind\":\"Status\",\"code\":404,\"reason\":\"NotFound\",\"status\":\"Failure\"}".to_vec(),
                                |body| body.as_ref().clone(),
                            );
                            let status = if routes.contains_key(path) { "200 OK" } else { "404 Not Found" };
                            sized.fetch_add(body.len(), Ordering::Relaxed);
                            let head = format!(
                                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n",
                                body.len()
                            );
                            if socket.write_all(head.as_bytes()).await.is_err()
                                || socket.write_all(&body).await.is_err()
                            {
                                return;
                            }
                        }
                    });
                }
            });
        });
        let port = port.recv().unwrap();
        Server {
            uri: format!("http://127.0.0.1:{port}").parse().unwrap(),
            requests,
            bytes,
        }
    }

    struct Run {
        allocations: Allocations,
        elapsed: std::time::Duration,
        calls: Vec<String>,
        requests: usize,
        response_bytes: usize,
        objects: usize,
    }

    fn run(scale: usize) -> Run {
        let server = serve(routes(scale));
        let endpoint = ClusterEndpoint::new(server.uri.clone(), None);
        let token = UserToken::new(SecretString::from("cost-run")).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _entered = runtime.enter();
        let cluster = Recording::new(UserClient::with_token(&endpoint, &token).unwrap());
        // One warm-up read opens the connections, so the measured read is the steady cost.
        runtime.block_on(async {
            cluster.namespace_objects(NAMESPACE).await.unwrap();
        });
        let (requests, bytes) = (
            server.requests.load(Ordering::Relaxed),
            server.bytes.load(Ordering::Relaxed),
        );
        let cluster = Recording::new(UserClient::with_token(&endpoint, &token).unwrap());
        let (objects, allocations) = measure(&runtime, overview(&cluster));
        let calls = cluster.calls();
        let requests = server.requests.load(Ordering::Relaxed) - requests;
        let response_bytes = server.bytes.load(Ordering::Relaxed) - bytes;
        // Time is the median of nine reads, outside the counting allocator.
        let mut times: Vec<_> = (0..9)
            .map(|_| {
                let started = Instant::now();
                runtime.block_on(overview(&cluster));
                started.elapsed()
            })
            .collect();
        times.sort();
        Run {
            allocations,
            elapsed: times[4],
            calls,
            requests,
            response_bytes,
            objects,
        }
    }

    /// One overview read: the namespace's objects and its events. Returns how many objects came
    /// back.
    async fn overview(cluster: &Recording<UserClient>) -> usize {
        let (objects, events) = tokio::join!(
            cluster.namespace_objects(NAMESPACE),
            cluster.list::<Event>(Some(NAMESPACE)),
        );
        let objects = objects.unwrap();
        objects.deployments.len()
            + objects.stateful_sets.len()
            + objects.daemon_sets.len()
            + objects.jobs.len()
            + objects.cron_jobs.len()
            + objects.pods.len()
            + objects.services.len()
            + objects.config_maps.len()
            + objects.secrets.len()
            + objects.persistent_volume_claims.len()
            + objects.ingresses.len()
            + objects.http_routes.len()
            + events.unwrap().len()
    }

    #[test]
    #[ignore = "a manual cost run, not a check"]
    fn print_what_an_overview_read_costs() {
        let mut out = std::io::stdout().lock();
        writeln!(out, "| Layout | Objects | Cluster calls | HTTP requests | Response bytes | Bytes allocated | Peak bytes | Allocations | Time (median of 9) |").unwrap();
        writeln!(out, "|---|---:|---:|---:|---:|---:|---:|---:|---:|").unwrap();
        for (label, scale) in [("baseline (1x)", 1), ("10x", 10)] {
            let run = run(scale);
            writeln!(
                out,
                "| {label} | {} | {} | {} | {} | {} | {} | {} | {:.1} ms |",
                run.objects,
                run.calls.len(),
                run.requests,
                run.response_bytes,
                run.allocations.bytes_total,
                run.allocations.bytes_peak,
                run.allocations.count,
                run.elapsed.as_secs_f64() * 1000.0,
            )
            .unwrap();
            writeln!(out, "| calls: {} |||||||||", run.calls.join(", ")).unwrap();
        }
    }
}
