//! A development server: the API over an in-memory cluster holding a few example projects (one
//! with a crash-looping worker, one whose image cannot be pulled, one with a failed CronJob run,
//! one rolling out, a large healthy one and a small one), and the embedded web interface. Never
//! shipped; for working on and trying the interface without a cluster.
//!
//! ```sh
//! scripts/build-ui.sh && ENGAWA_UI_DIST=$PWD/web/dist \
//!   cargo run -p engawa-api --example demo        # interface and API on http://127.0.0.1:8080
//!                                                 # (ENGAWA_DEMO_ADDR picks another address)
//! (cd web && bun run dev)                          # or Vite's dev server on :5173, proxying /api
//! ```
//!
//! `ENGAWA_DEMO_FORBID=events.events.k8s.io,pods` refuses those cluster-wide reads (`nodes`,
//! `pods`, `nodes.metrics.k8s.io`, `events.events.k8s.io`, `plans.upgrade.cattle.io`), to see a
//! section's permission state; the listed kinds but `pods` are also refused in every namespace
//! (`secrets`, `httproutes.gateway.networking.k8s.io`: a project's pages show them refused in
//! place), and `pods/exec` refuses every terminal. With `ENGAWA_DEMO_ELEVATION=1` the demo says
//! admin rights are offered, so refusals show "View as admin" or "Retry as admin" (the demo has
//! no `/auth/elevate` to grant them). `services/proxy` in `ENGAWA_DEMO_FORBID` refuses the
//! sample metrics store, whose history is made up like the live usage. A system-upgrade-controller
//! Plan resolves a newer patch, so the cluster page shows one; the demo makes no outbound request.

#![expect(
    clippy::expect_used,
    reason = "fixed example data; a typo should stop the demo"
)]
#![expect(
    clippy::too_many_lines,
    reason = "the example data is long, literal and read top to bottom"
)]

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::http::request::Parts;
use engawa_api::access::{Access, ClusterAccess, Elevation};
use engawa_core::problem::Problem;
use engawa_kube::k8s_openapi::api::apps::v1::{Deployment, StatefulSet};
use engawa_kube::k8s_openapi::api::batch::v1::{CronJob, Job};
use engawa_kube::k8s_openapi::api::core::v1::{
    ConfigMap, Namespace, Node, PersistentVolumeClaim, Pod, Secret, Service,
};
use engawa_kube::k8s_openapi::api::events::v1::Event;
use engawa_kube::k8s_openapi::api::networking::v1::Ingress;
use engawa_kube::k8s_openapi::jiff::{SignedDuration, Timestamp};
use engawa_kube::memory::{MemoryCluster, State};
use engawa_kube::{
    Cluster, ClusterFacts, DynamicObject, Identity, NamespaceObjects, SecretSummary, ServedResource,
};
use serde_json::{Value, json};

fn from<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("valid example object")
}

/// A Deployment in `shop` with one container, its selector `app=<name>`.
fn deployment(name: &str, image: &str, replicas: i32, ready: i32, env: &Value) -> Deployment {
    deployment_in("shop", name, image, replicas, ready, env)
}

/// A Deployment in `namespace` with one container, its selector `app=<name>`.
fn deployment_in(
    namespace: &str,
    name: &str,
    image: &str,
    replicas: i32,
    ready: i32,
    env: &Value,
) -> Deployment {
    from(json!({
        "metadata": {"name": name, "namespace": namespace, "generation": 3,
            "labels": {"app.kubernetes.io/name": name, "app.kubernetes.io/part-of": namespace}},
        "spec": {"replicas": replicas, "selector": {"matchLabels": {"app": name}},
            "template": {"metadata": {"labels": {"app": name}}, "spec": {"containers": [{
                "name": name, "image": image,
                "ports": [{"name": "http", "containerPort": 3000}],
                "env": env,
                "resources": {"requests": {"cpu": "100m", "memory": "128Mi"}, "limits": {"memory": "256Mi"}},
                "readinessProbe": {"httpGet": {"path": "/healthz", "port": "http"}}
            }]}}},
        "status": {"observedGeneration": 3, "replicas": replicas, "readyReplicas": ready,
            "updatedReplicas": replicas, "availableReplicas": ready}
    }))
}

/// A StatefulSet in `shop` with one data volume, its selector `app=<name>`.
fn stateful_set(name: &str, image: &str, size: &str, env: &Value) -> StatefulSet {
    stateful_set_in("shop", name, image, size, env)
}

/// A StatefulSet in `namespace` with one data volume, its selector `app=<name>`.
fn stateful_set_in(
    namespace: &str,
    name: &str,
    image: &str,
    size: &str,
    env: &Value,
) -> StatefulSet {
    from(json!({
        "metadata": {"name": name, "namespace": namespace,
            "labels": {"app.kubernetes.io/name": name, "app.kubernetes.io/component": "database"}},
        "spec": {"replicas": 1, "serviceName": name, "selector": {"matchLabels": {"app": name}},
            "template": {"metadata": {"labels": {"app": name}}, "spec": {"containers": [{
                "name": name, "image": image, "env": env,
                "resources": {"requests": {"cpu": "250m", "memory": "512Mi"}, "limits": {"memory": "1Gi"}},
                "volumeMounts": [{"name": "data", "mountPath": "/data"}]
            }]}},
            "volumeClaimTemplates": [{"metadata": {"name": "data"},
                "spec": {"accessModes": ["ReadWriteOnce"], "resources": {"requests": {"storage": size}}}}]},
        "status": {"replicas": 1, "readyReplicas": 1}
    }))
}

/// A Pod of `app` in `shop`; `owner` is set for StatefulSet pods (Deployment pods match by
/// selector). One that is not ready crash-loops.
fn pod(name: &str, app: &str, ready: bool, restarts: i32, owner: Option<&str>) -> Pod {
    let status = if ready {
        running(app, restarts)
    } else {
        crash_looping(app, restarts)
    };
    pod_in(
        "shop",
        name,
        app,
        owner.map(|o| ("StatefulSet", o)),
        &status,
    )
}

/// A Pod of `app` in `namespace` with `status`; `owner` is `(kind, name)` for pods a StatefulSet
/// or Job owns (Deployment pods match by selector).
fn pod_in(
    namespace: &str,
    name: &str,
    app: &str,
    owner: Option<(&str, &str)>,
    status: &Value,
) -> Pod {
    let owners = owner.map_or_else(
        || json!([]),
        |(kind, o)| {
            let api = if kind == "Job" { "batch/v1" } else { "apps/v1" };
            json!([{"apiVersion": api, "kind": kind, "name": o,
                "uid": "00000000-0000-0000-0000-000000000000", "controller": true}])
        },
    );
    // A node and an age that differ from pod to pod and stay the same from run to run.
    let seed = name.bytes().map(i64::from).sum::<i64>();
    let node = ["vps-1", "vps-3"]
        .get(usize::try_from(seed % 2).unwrap_or_default())
        .copied()
        .unwrap_or("vps-1");
    from(json!({
        "metadata": {"name": name, "namespace": namespace, "labels": {"app": app},
            "ownerReferences": owners, "creationTimestamp": minutes_ago(30 + seed % 4000)},
        "spec": {"containers": [{"name": app, "image": "example"}], "nodeName": node},
        "status": status
    }))
}

/// A running, ready pod's status.
fn running(container: &str, restarts: i32) -> Value {
    json!({"phase": "Running", "containerStatuses": [{"name": container, "ready": true,
        "restartCount": restarts, "image": "example", "imageID": "", "state": {"running": {}}}]})
}

/// The status of a pod whose container waits for `reason`.
fn waiting(container: &str, reason: &str, restarts: i32) -> Value {
    json!({"phase": "Running", "containerStatuses": [{"name": container, "ready": false,
        "restartCount": restarts, "image": "example", "imageID": "",
        "state": {"waiting": {"reason": reason}}}]})
}

/// The status of a pod whose container waits in `CrashLoopBackOff`, its previous run in
/// `lastState.terminated` as the kubelet reports it: exit code 2, a Go program's `panic`.
fn crash_looping(container: &str, restarts: i32) -> Value {
    json!({"phase": "Running", "containerStatuses": [{"name": container, "ready": false,
        "restartCount": restarts, "image": "example", "imageID": "",
        "state": {"waiting": {"reason": "CrashLoopBackOff"}},
        "lastState": {"terminated": {"exitCode": 2, "reason": "Error",
            "startedAt": minutes_ago(2), "finishedAt": minutes_ago(1)}}}]})
}

/// A Service in `shop` selecting `app=<name>`.
fn service(name: &str, port: i32, port_name: &str) -> Service {
    service_in("shop", name, port, port_name)
}

/// A Service in `namespace` selecting `app=<name>`. An `http` port sends to the container port
/// `http` (3000) the Deployments here declare; the data stores declare none, so the others send
/// to their own number.
fn service_in(namespace: &str, name: &str, port: i32, port_name: &str) -> Service {
    let target = if port_name == "http" {
        json!("http")
    } else {
        json!(port)
    };
    from(json!({"metadata": {"name": name, "namespace": namespace},
        "spec": {"selector": {"app": name},
            "ports": [{"name": port_name, "port": port, "targetPort": target}]}}))
}

/// An Ingress in `namespace` routing `host` to the Service `backend`, with TLS.
fn ingress(namespace: &str, host: &str, backend: &str) -> Ingress {
    from(
        json!({"metadata": {"name": backend, "namespace": namespace},
        "spec": {"rules": [{"host": host, "http": {"paths": [{"path": "/", "pathType": "Prefix",
            "backend": {"service": {"name": backend, "port": {"name": "http"}}}}]}}],
            "tls": [{"hosts": [host], "secretName": format!("{backend}-tls")}]}}),
    )
}

fn shop() -> NamespaceObjects {
    let secret = |name: &str, key: &str| json!({"secretKeyRef": {"name": name, "key": key}});
    let web = deployment(
        "web",
        "ghcr.io/example/shop-web:1.4.2",
        2,
        2,
        &json!([
            {"name": "NODE_ENV", "value": "production"},
            {"name": "API_URL", "value": "http://api.shop.svc.cluster.local:8080"},
            {"name": "POD_NAME", "valueFrom": {"fieldRef": {"fieldPath": "metadata.name"}}}
        ]),
    );
    let api = deployment(
        "api",
        "ghcr.io/example/shop-api:2.11.0",
        3,
        3,
        &json!([
            {"name": "DATABASE_HOST", "value": "db"},
            {"name": "DATABASE_PASSWORD", "valueFrom": secret("db-credentials", "password")},
            {"name": "REDIS_URL", "value": "redis://redis:6379/0"},
            {"name": "LOG_LEVEL", "value": "info"}
        ]),
    );
    let worker = deployment(
        "worker",
        "ghcr.io/example/shop-worker:2.11.0",
        2,
        1,
        &json!([
            {"name": "QUEUE_URL", "value": "redis://redis:6379/1"},
            {"name": "DATABASE_HOST", "value": "db"},
            {"name": "DATABASE_PASSWORD", "valueFrom": secret("db-credentials", "password")}
        ]),
    );
    let mut web = web;
    if let Some(container) = web
        .spec
        .as_mut()
        .and_then(|s| s.template.spec.as_mut())
        .and_then(|s| s.containers.first_mut())
    {
        container.env_from = Some(vec![from(json!({"configMapRef": {"name": "web-config"}}))]);
    }
    let db = stateful_set(
        "db",
        "postgres:18",
        "20Gi",
        &json!([
            {"name": "POSTGRES_PASSWORD", "valueFrom": secret("db-credentials", "password")}
        ]),
    );
    let redis = stateful_set("redis", "redis:8", "2Gi", &json!([]));
    let nightly: CronJob = from(json!({
        "metadata": {"name": "nightly-report", "namespace": "shop"},
        "spec": {"schedule": "0 2 * * *", "jobTemplate": {"spec": {"template": {"spec": {
            "restartPolicy": "OnFailure",
            "containers": [{"name": "report", "image": "ghcr.io/example/shop-api:2.11.0",
                "args": ["report", "--yesterday"],
                "env": [{"name": "DATABASE_HOST", "value": "db"},
                        {"name": "DATABASE_PASSWORD", "valueFrom": secret("db-credentials", "password")}]}]}}}}},
        "status": {"lastScheduleTime": "2026-10-14T02:00:00Z", "lastSuccessfulTime": "2026-10-14T02:03:12Z"}
    }));
    NamespaceObjects {
        namespace: "shop".to_owned(),
        deployments: vec![web, api, worker],
        stateful_sets: vec![db, redis],
        cron_jobs: vec![nightly],
        pods: vec![
            pod("web-7c9d8-abcde", "web", true, 0, None),
            pod("web-7c9d8-fghij", "web", true, 2, None),
            pod("api-5f6b7-k2m4p", "api", true, 0, None),
            pod("api-5f6b7-q8r1s", "api", true, 0, None),
            pod("api-5f6b7-x9z3t", "api", true, 1, None),
            pod("worker-84c2d-h7j6k", "worker", true, 0, None),
            pod("worker-84c2d-w2e5r", "worker", false, 14, None),
            pod("db-0", "db", true, 0, Some("db")),
            pod("redis-0", "redis", true, 0, Some("redis")),
        ],
        services: vec![
            service("web", 80, "http"),
            service("api", 8080, "http"),
            service("db", 5432, "postgres"),
            service("redis", 6379, "redis"),
        ],
        config_maps: vec![from::<ConfigMap>(
            json!({"metadata": {"name": "web-config", "namespace": "shop"},
            "data": {"LOG_LEVEL": "info", "FEATURE_CHECKOUT_V2": "true"}}),
        )],
        secrets: vec![SecretSummary {
            secret: Secret {
                metadata: from(json!({"name": "db-credentials", "namespace": "shop"})),
                type_: Some("Opaque".to_owned()),
                ..Secret::default()
            },
            key_count: 2,
        }],
        persistent_volume_claims: vec![
            from::<PersistentVolumeClaim>(
                json!({"metadata": {"name": "data-db-0", "namespace": "shop"},
                "spec": {"accessModes": ["ReadWriteOnce"], "resources": {"requests": {"storage": "20Gi"}}},
                "status": {"phase": "Bound", "capacity": {"storage": "20Gi"}}}),
            ),
            from::<PersistentVolumeClaim>(
                json!({"metadata": {"name": "data-redis-0", "namespace": "shop"},
                "spec": {"accessModes": ["ReadWriteOnce"], "resources": {"requests": {"storage": "2Gi"}}},
                "status": {"phase": "Bound", "capacity": {"storage": "2Gi"}}}),
            ),
            from::<PersistentVolumeClaim>(
                json!({"metadata": {"name": "old-uploads", "namespace": "shop"},
                "spec": {"accessModes": ["ReadWriteOnce"], "resources": {"requests": {"storage": "5Gi"}}},
                "status": {"phase": "Bound", "capacity": {"storage": "5Gi"}}}),
            ),
        ],
        ingresses: vec![
            from::<Ingress>(json!({"metadata": {"name": "web", "namespace": "shop"},
                "spec": {"rules": [{"host": "shop.example.com", "http": {"paths": [{"path": "/", "pathType": "Prefix",
                    "backend": {"service": {"name": "web", "port": {"name": "http"}}}}]}}],
                    "tls": [{"hosts": ["shop.example.com"], "secretName": "shop-tls"}]}})),
            from::<Ingress>(json!({"metadata": {"name": "api", "namespace": "shop"},
                "spec": {"rules": [{"host": "api.shop.example.com", "http": {"paths": [{"path": "/", "pathType": "Prefix",
                    "backend": {"service": {"name": "api", "port": {"name": "http"}}}}]}}],
                    "tls": [{"hosts": ["api.shop.example.com"], "secretName": "api-tls"}]}})),
        ],
        ..NamespaceObjects::default()
    }
}

/// A blog whose new image cannot be pulled, and its database.
fn blog() -> NamespaceObjects {
    let ns = "blog";
    NamespaceObjects {
        namespace: ns.to_owned(),
        deployments: vec![deployment_in(
            ns,
            "web",
            "ghcr.io/example/blog:2.4.1",
            1,
            0,
            &json!([]),
        )],
        stateful_sets: vec![stateful_set_in(ns, "db", "postgres:18", "10Gi", &json!([]))],
        pods: vec![
            pod_in(
                ns,
                "web-6d5f9c7b8-p4k2z",
                "web",
                None,
                &waiting("web", "ImagePullBackOff", 0),
            ),
            pod_in(
                ns,
                "db-0",
                "db",
                Some(("StatefulSet", "db")),
                &running("db", 0),
            ),
        ],
        services: vec![
            service_in(ns, "web", 80, "http"),
            service_in(ns, "db", 5432, "postgres"),
        ],
        ingresses: vec![ingress(ns, "blog.example.com", "web")],
        ..NamespaceObjects::default()
    }
}

/// Billing: an API, a database and a nightly CronJob whose last run failed.
fn billing() -> NamespaceObjects {
    let ns = "billing";
    let run = "invoice-run-29342880";
    let cron_job: CronJob = from(json!({
        "metadata": {"name": "invoice-run", "namespace": ns},
        "spec": {"schedule": "0 2 * * *", "jobTemplate": {"spec": {"template": {"spec": {
            "restartPolicy": "Never",
            "containers": [{"name": "invoice-run", "image": "ghcr.io/example/billing:3.2.0",
                "args": ["invoices", "--run"]}]}}}}},
        "status": {"lastScheduleTime": "2026-10-14T02:00:00Z"}
    }));
    let job: Job = from(json!({
        "metadata": {"name": run, "namespace": ns, "ownerReferences": [{"apiVersion": "batch/v1",
            "kind": "CronJob", "name": "invoice-run",
            "uid": "00000000-0000-0000-0000-000000000000", "controller": true}]},
        "spec": {"backoffLimit": 0, "template": {"spec": {"restartPolicy": "Never",
            "containers": [{"name": "invoice-run", "image": "ghcr.io/example/billing:3.2.0"}]}}},
        "status": {"failed": 1, "conditions": [{"type": "Failed", "status": "True",
            "reason": "BackoffLimitExceeded", "message": "Job has reached the specified backoff limit"}]}
    }));
    let failed = json!({"phase": "Failed", "containerStatuses": [{"name": "invoice-run",
        "ready": false, "restartCount": 0, "image": "example", "imageID": "",
        "state": {"terminated": {"exitCode": 1, "reason": "Error"}}}]});
    NamespaceObjects {
        namespace: ns.to_owned(),
        deployments: vec![deployment_in(
            ns,
            "api",
            "ghcr.io/example/billing:3.2.0",
            2,
            2,
            &json!([]),
        )],
        stateful_sets: vec![stateful_set_in(ns, "db", "postgres:18", "20Gi", &json!([]))],
        cron_jobs: vec![cron_job],
        jobs: vec![job],
        pods: vec![
            pod_in(ns, "api-5b7d9-c3v8n", "api", None, &running("api", 0)),
            pod_in(ns, "api-5b7d9-m1x4q", "api", None, &running("api", 0)),
            pod_in(
                ns,
                "db-0",
                "db",
                Some(("StatefulSet", "db")),
                &running("db", 0),
            ),
            pod_in(
                ns,
                &format!("{run}-7xk2p"),
                "invoice-run",
                Some(("Job", run)),
                &failed,
            ),
        ],
        services: vec![
            service_in(ns, "api", 8080, "http"),
            service_in(ns, "db", 5432, "postgres"),
        ],
        ingresses: vec![ingress(ns, "billing.example.com", "api")],
        ..NamespaceObjects::default()
    }
}

/// Payments, with a rollout of its API under way.
fn payments() -> NamespaceObjects {
    let ns = "payments";
    let mut api = deployment_in(
        ns,
        "api",
        "ghcr.io/example/payments:5.0.1",
        3,
        2,
        &json!([]),
    );
    api.metadata.generation = Some(4);
    api.status = Some(from(
        json!({"observedGeneration": 4, "replicas": 3, "readyReplicas": 2,
        "updatedReplicas": 2, "availableReplicas": 2,
        "conditions": [{"type": "Progressing", "status": "True", "reason": "ReplicaSetUpdated"}]}),
    ));
    NamespaceObjects {
        namespace: ns.to_owned(),
        deployments: vec![
            api,
            deployment_in(
                ns,
                "webhooks",
                "ghcr.io/example/payments:5.0.0",
                2,
                2,
                &json!([]),
            ),
        ],
        stateful_sets: vec![stateful_set_in(ns, "db", "postgres:18", "20Gi", &json!([]))],
        pods: vec![
            pod_in(ns, "api-6f5d4-a8b2c", "api", None, &running("api", 0)),
            pod_in(ns, "api-6f5d4-d4e6f", "api", None, &running("api", 0)),
            pod_in(
                ns,
                "api-6f5d4-g7h9j",
                "api",
                None,
                &waiting("api", "ContainerCreating", 0),
            ),
            pod_in(
                ns,
                "webhooks-8c2e1-k5l7m",
                "webhooks",
                None,
                &running("webhooks", 0),
            ),
            pod_in(
                ns,
                "webhooks-8c2e1-n3p6q",
                "webhooks",
                None,
                &running("webhooks", 0),
            ),
            pod_in(
                ns,
                "db-0",
                "db",
                Some(("StatefulSet", "db")),
                &running("db", 0),
            ),
        ],
        services: vec![
            service_in(ns, "api", 8080, "http"),
            service_in(ns, "webhooks", 8080, "http"),
            service_in(ns, "db", 5432, "postgres"),
        ],
        ingresses: vec![ingress(ns, "pay.example.com", "api")],
        ..NamespaceObjects::default()
    }
}

/// A large, healthy project: 30 small services, two stores and a dashboard behind a domain.
fn analytics() -> NamespaceObjects {
    let ns = "analytics";
    let names = [
        "dashboard",
        "collector",
        "ingest",
        "enricher",
        "sessionizer",
        "geoip",
        "useragent",
        "dedupe",
        "sampler",
        "aggregator",
        "rollup-hourly",
        "rollup-daily",
        "exporter",
        "alerts",
        "notifier",
        "scheduler",
        "query",
        "query-cache",
        "funnel",
        "retention",
        "cohorts",
        "paths",
        "heatmaps",
        "replay",
        "uploader",
        "compactor",
        "archiver",
        "janitor",
        "metrics",
        "tracer",
    ];
    let mut deployments = Vec::new();
    let mut pods = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let replicas = match i % 5 {
            0 => 2,
            3 => 3,
            _ => 1,
        };
        deployments.push(deployment_in(
            ns,
            name,
            &format!("ghcr.io/example/analytics-{name}:1.8.0"),
            replicas,
            replicas,
            &json!([]),
        ));
        for r in 0..replicas {
            pods.push(pod_in(
                ns,
                &format!("{name}-7d4c9-{i:02}{r}x"),
                name,
                None,
                &running(name, 0),
            ));
        }
    }
    let stores = [
        ("clickhouse", "clickhouse/clickhouse-server:25.8"),
        ("redis", "redis:8"),
        ("db", "postgres:18"),
    ];
    let mut stateful_sets = Vec::new();
    for (name, image) in stores {
        stateful_sets.push(stateful_set_in(ns, name, image, "50Gi", &json!([])));
        pods.push(pod_in(
            ns,
            &format!("{name}-0"),
            name,
            Some(("StatefulSet", name)),
            &running(name, 0),
        ));
    }
    NamespaceObjects {
        namespace: ns.to_owned(),
        deployments,
        stateful_sets,
        pods,
        services: vec![service_in(ns, "dashboard", 80, "http")],
        ingresses: vec![ingress(ns, "stats.example.com", "dashboard")],
        ..NamespaceObjects::default()
    }
}

/// Internal tools without a public domain.
fn internal_tools() -> NamespaceObjects {
    let ns = "internal-tools";
    let tools = [
        ("wiki", "ghcr.io/example/wiki:2.5.0"),
        ("chat", "ghcr.io/example/chat:7.1.0"),
        ("grafana", "grafana/grafana:12.2.0"),
    ];
    NamespaceObjects {
        namespace: ns.to_owned(),
        deployments: tools
            .iter()
            .map(|(name, image)| deployment_in(ns, name, image, 1, 1, &json!([])))
            .collect(),
        pods: tools
            .iter()
            .map(|(name, _)| {
                pod_in(
                    ns,
                    &format!("{name}-5c6d7-r2s4t"),
                    name,
                    None,
                    &running(name, 0),
                )
            })
            .collect(),
        services: tools
            .iter()
            .map(|(name, _)| service_in(ns, name, 80, "http"))
            .collect(),
        ..NamespaceObjects::default()
    }
}

fn state() -> State {
    let namespace = |name: &str, adopted: bool| -> Namespace {
        let labels = if adopted {
            json!({"engawa.engawahq.dev/project": "true"})
        } else {
            json!({})
        };
        from(json!({"metadata": {"name": name, "labels": labels}}))
    };
    let event_in = |namespace: &str,
                    name: &str,
                    minute: u32,
                    kind: &str,
                    object: &str,
                    kind_type: &str,
                    reason: &str,
                    note: &str,
                    count: i32|
     -> Event {
        from(json!({
            "metadata": {"name": name, "namespace": namespace},
            // Relative to now, so they stay in the cluster overview's last hour.
            "eventTime": minutes_ago(35 - i64::from(minute)),
            "regarding": {"kind": kind, "name": object, "namespace": namespace, "apiVersion": "v1"},
            "type": kind_type, "reason": reason, "note": note,
            "reportingController": "kubelet", "reportingInstance": "node-1", "action": reason,
            "deprecatedCount": count
        }))
    };
    let event = |name: &str,
                 minute: u32,
                 kind: &str,
                 object: &str,
                 kind_type: &str,
                 reason: &str,
                 note: &str,
                 count: i32| {
        event_in(
            "shop", name, minute, kind, object, kind_type, reason, note, count,
        )
    };
    let blog_events = vec![
        event_in(
            "blog",
            "b1",
            23,
            "Pod",
            "web-6d5f9c7b8-p4k2z",
            "Warning",
            "Failed",
            "Failed to pull image \"ghcr.io/example/blog:2.4.1\": not found",
            8,
        ),
        event_in(
            "blog",
            "b2",
            24,
            "Pod",
            "web-6d5f9c7b8-p4k2z",
            "Warning",
            "BackOff",
            "Back-off pulling image \"ghcr.io/example/blog:2.4.1\"",
            31,
        ),
    ];
    let billing_events = vec![event_in(
        "billing",
        "i1",
        2,
        "Job",
        "invoice-run-29342880",
        "Warning",
        "BackoffLimitExceeded",
        "Job has reached the specified backoff limit",
        1,
    )];
    let events = vec![
        event(
            "e1",
            1,
            "Deployment",
            "api",
            "Normal",
            "ScalingReplicaSet",
            "Scaled up replica set api-5f6b7 to 3",
            1,
        ),
        event(
            "e2",
            5,
            "Pod",
            "web-7c9d8-fghij",
            "Warning",
            "BackOff",
            "Back-off restarting failed container web",
            2,
        ),
        event(
            "e3",
            12,
            "Pod",
            "api-5f6b7-x9z3t",
            "Normal",
            "Pulled",
            "Container image ghcr.io/example/shop-api:2.11.0 already present",
            1,
        ),
        event(
            "e4",
            21,
            "Pod",
            "worker-84c2d-w2e5r",
            "Warning",
            "BackOff",
            "Back-off restarting failed container worker",
            14,
        ),
        event(
            "e5",
            22,
            "Pod",
            "worker-84c2d-w2e5r",
            "Warning",
            "Unhealthy",
            "Readiness probe failed: connection refused",
            9,
        ),
        event(
            "e6",
            30,
            "CronJob",
            "nightly-report",
            "Normal",
            "SawCompletedJob",
            "Saw completed job: nightly-report-29342880",
            1,
        ),
    ];
    let mut facts = ClusterFacts {
        version: VERSION.to_owned(),
        nodes: Some(3),
        ..ClusterFacts::default()
    };
    facts
        .ingress_controllers
        .insert("traefik.io/ingress-controller".to_owned());
    facts.k3s_helm_charts.insert(
        "kube-system/traefik".to_owned(),
        "https://%{KUBERNETES_API}%/static/charts/traefik-41.6.101+up41.6.1.tgz".to_owned(),
    );
    facts
        .api_services
        .insert("v1beta1.metrics.k8s.io".to_owned());
    facts.crds.insert("certificates.cert-manager.io".to_owned());
    facts.workload_labels.push((
        "Deployment".to_owned(),
        BTreeMap::from([
            (
                "app.kubernetes.io/name".to_owned(),
                "cert-manager".to_owned(),
            ),
            (
                "helm.sh/chart".to_owned(),
                "cert-manager-v1.19.1".to_owned(),
            ),
        ]),
    ));
    let mut objects = BTreeMap::new();
    for mut project in [
        shop(),
        blog(),
        billing(),
        payments(),
        analytics(),
        internal_tools(),
        kube_system(),
    ] {
        place(&mut project.pods);
        objects.insert(project.namespace.clone(), project);
    }
    let pods = demo_pods(&objects);
    let live = pod_metrics(&pods);
    PODS.get_or_init(|| pods);
    let answer: engawa_kube::memory::ProxyAnswer = store;
    let system_events = vec![from::<Event>(json!({
        "metadata": {"name": "s1", "namespace": "kube-system"},
        "eventTime": minutes_ago(12),
        "regarding": {"kind": "Node", "name": "vps-2", "apiVersion": "v1"},
        "type": "Warning", "reason": "NodeNotSchedulable",
        "note": "Node vps-2 status is now: NodeNotSchedulable",
        "reportingController": "node-controller", "reportingInstance": "vps-1",
        "action": "NodeNotSchedulable", "deprecatedCount": 1
    }))];
    let log = (1..=40)
        .map(|i| {
            format!(
                "2026-10-14T08:{i:02}:00Z \u{1b}[32mINFO\u{1b}[0m GET /products 200 {}ms",
                i % 7 + 3
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    State {
        identity: Identity {
            username: "alice@example.com".to_owned(),
            groups: vec!["developers".to_owned()],
        },
        namespaces: vec![
            namespace("shop", true),
            namespace("blog", true),
            namespace("billing", true),
            namespace("payments", true),
            namespace("analytics", true),
            namespace("internal-tools", true),
            namespace("default", false),
            namespace("kube-system", false),
        ],
        objects,
        events: BTreeMap::from([
            ("shop".to_owned(), events),
            ("blog".to_owned(), blog_events),
            ("billing".to_owned(), billing_events),
            ("kube-system".to_owned(), system_events),
        ]),
        nodes: nodes(),
        // metrics-server and system-upgrade-controller, read through their modules; and
        // NetworkPolicy, so a ready-made policy shows among the objects and can be deleted.
        served: vec![
            ServedResource::readable("metrics.k8s.io", "v1beta1", "nodes", "NodeMetrics", false),
            ServedResource::readable("metrics.k8s.io", "v1beta1", "pods", "PodMetrics", true),
            ServedResource::readable(
                "networking.k8s.io",
                "v1",
                "networkpolicies",
                "NetworkPolicy",
                true,
            ),
            ServedResource::readable("upgrade.cattle.io", "v1", "plans", "Plan", true),
        ],
        dynamic: [
            usage("vps-1", "1420m", "1530Mi"),
            usage("vps-2", "310m", "2210Mi"),
            usage("vps-3", "180m", "940Mi"),
            from(json!({
                "apiVersion": "upgrade.cattle.io/v1", "kind": "Plan",
                "metadata": {"name": "server", "namespace": "system-upgrade"},
                "spec": {"channel": "https://update.k3s.io/v1-release/channels/v1.34"},
                "status": {"latestVersion": "v1.34.12+k3s1"}
            })),
        ]
        .into_iter()
        .chain(live)
        .collect(),
        // The metrics store where Engawa's recommendation installs it.
        proxied: BTreeMap::from([(
            engawa_api::settings::default_metrics_store().to_string(),
            answer,
        )]),
        forbidden_resources: std::env::var("ENGAWA_DEMO_FORBID")
            .map_or_default(|list| list.split(',').map(str::to_owned).collect()),
        // The same kinds in every namespace too, but pods: a project without its pods says
        // little, and `pods` here is for the cluster page's requests.
        forbidden_in_namespaces: std::env::var("ENGAWA_DEMO_FORBID").map_or_default(|list| {
            list.split(',')
                .filter(|r| *r != "pods")
                .map(str::to_owned)
                .collect()
        }),
        logs: BTreeMap::from([
            ("shop/web-7c9d8-abcde".to_owned(), log.clone()),
            ("shop/web-7c9d8-fghij".to_owned(), log.clone()),
            ("shop/api-5f6b7-k2m4p".to_owned(), log.clone()),
            ("shop/api-5f6b7-q8r1s".to_owned(), log.clone()),
            ("shop/api-5f6b7-x9z3t".to_owned(), log),
            ("shop/worker-84c2d-h7j6k".to_owned(), "\u{1b}[32mINFO\u{1b}[0m consumed 42 jobs".to_owned()),
            ("shop/worker-84c2d-w2e5r".to_owned(), "\u{1b}[31mERROR\u{1b}[0m connect redis:6379: connection refused\npanic: queue unavailable".to_owned()),
            ("shop/redis-0".to_owned(), "Ready to accept connections tcp".to_owned()),
            (
                "shop/db-0".to_owned(),
                "database system is ready to accept connections".to_owned(),
            ),
        ]),
        name: "k3d-demo".to_owned(),
        // The crash-looping worker's run before the last restart; the other pods have none.
        previous_logs: BTreeMap::from([(
            "shop/worker-84c2d-w2e5r".to_owned(),
            "\u{1b}[31mERROR\u{1b}[0m connect redis:6379: connection refused\npanic: queue unavailable".to_owned(),
        )]),
        secret_values: BTreeMap::from([(
            "shop/db-credentials".to_owned(),
            BTreeMap::from([
                ("username".to_owned(), b"shop".to_vec()),
                (
                    "password".to_owned(),
                    b"correct-horse-battery-staple".to_vec(),
                ),
            ]),
        )]),
        facts,
        ..State::default()
    }
}

/// The cluster's version: a k3s whose minor is in maintenance and has a newer patch.
const VERSION: &str = "v1.34.10+k3s1";

/// `minutes` before now, as an event time.
fn minutes_ago(minutes: i64) -> String {
    let now = Timestamp::now();
    now.checked_sub(SignedDuration::from_mins(minutes))
        .unwrap_or(now)
        .to_string()
}

/// metrics-server's `NodeMetrics` for `node`.
fn usage(node: &str, cpu: &str, memory: &str) -> DynamicObject {
    from(json!({
        "apiVersion": "metrics.k8s.io/v1beta1", "kind": "NodeMetrics",
        "metadata": {"name": node},
        "usage": {"cpu": cpu, "memory": memory}
    }))
}

// ── Usage and history ────────────────────────────────────────────────────────────────────────
//
// Sample numbers, the same on every run: metrics-server's `PodMetrics` for every running pod, and
// a metrics store behind the service proxy that answers Engawa's range queries. The worker runs
// close to its memory limit and, over a day, leaks memory until it is killed for it every nine
// hours; a Deployment rolled out 30 hours ago, so a week shows the pods it replaced.

const MIB: f64 = 1_048_576.0;

/// What a pod of `app` uses, as shares of its template's CPU request and memory limit.
fn shares(app: &str) -> (f64, f64) {
    match app {
        "web" => (0.35, 0.42),
        "api" => (1.6, 0.68),
        "worker" => (0.9, 0.93),
        "db" => (0.4, 0.61),
        "redis" => (0.08, 0.22),
        _ => (0.3, 0.4),
    }
}

/// The demo templates' CPU request (millicores) and memory limit (bytes).
fn room(stateful: bool) -> (f64, f64) {
    if stateful {
        (250.0, 1024.0 * MIB)
    } else {
        (100.0, 256.0 * MIB)
    }
}

/// FNV-1a of `text`.
fn hash(text: &str) -> u64 {
    text.bytes().fold(0xCBF2_9CE4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3)
    })
}

/// Numbers from 0 to 1, the same sequence for the same seed (`SplitMix64`).
struct Noise(u64);

impl Noise {
    fn new(seed: &str) -> Self {
        Self(hash(seed))
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "53 bits of a random number are plenty for sample data"
    )]
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1_u64 << 53) as f64
    }
}

/// A demo pod, as the sample store knows it.
#[derive(Debug, Clone)]
struct DemoPod {
    namespace: String,
    name: String,
    app: String,
    stateful: bool,
    ready: bool,
    restarts: i32,
}

/// Every pod of the demo, for the sample store.
static PODS: std::sync::OnceLock<Vec<DemoPod>> = std::sync::OnceLock::new();

fn demo_pods(objects: &BTreeMap<String, NamespaceObjects>) -> Vec<DemoPod> {
    objects
        .values()
        .flat_map(|project| &project.pods)
        .map(|pod| {
            let statuses = pod
                .status
                .as_ref()
                .and_then(|s| s.container_statuses.clone())
                .unwrap_or_default();
            DemoPod {
                namespace: pod.metadata.namespace.clone().unwrap_or_default(),
                name: pod.metadata.name.clone().unwrap_or_default(),
                app: pod
                    .metadata
                    .labels
                    .as_ref()
                    .and_then(|l| l.get("app"))
                    .cloned()
                    .unwrap_or_default(),
                stateful: pod
                    .metadata
                    .owner_references
                    .iter()
                    .flatten()
                    .any(|o| o.kind == "StatefulSet"),
                ready: pod.status.as_ref().and_then(|s| s.phase.as_deref()) == Some("Running")
                    && !statuses.is_empty()
                    && statuses.iter().all(|c| c.ready),
                restarts: statuses.iter().map(|c| c.restart_count).sum(),
            }
        })
        .collect()
}

/// metrics-server's `PodMetrics` for every running, ready pod: it has no sample of the others.
fn pod_metrics(pods: &[DemoPod]) -> Vec<DynamicObject> {
    pods.iter()
        .filter(|pod| pod.ready)
        .map(|pod| {
            let (cpu_request, memory_limit) = room(pod.stateful);
            let (cpu_share, memory_share) = shares(&pod.app);
            let mut noise = Noise::new(&format!("live:{}", pod.name));
            let cpu = cpu_request * cpu_share * noise.next().mul_add(0.6, 0.7);
            let memory = memory_limit * memory_share * noise.next().mul_add(0.04, 0.95);
            from(json!({
                "apiVersion": "metrics.k8s.io/v1beta1", "kind": "PodMetrics",
                "metadata": {"name": pod.name, "namespace": pod.namespace},
                "timestamp": minutes_ago(0), "window": "15.012s",
                "containers": [{"name": pod.app, "usage": {
                    "cpu": format!("{:.0}n", cpu * 1e6),
                    "memory": format!("{:.0}Ki", memory / 1024.0)
                }}]
            }))
        })
        .collect()
}

/// The service a pod pattern of `engawa_modules::prometheus_api::pod_pattern` names.
fn pattern_name(pattern: &str) -> &str {
    [
        "-[a-z0-9]{1,10}-[a-z0-9]{5}",
        "-[0-9]+-[a-z0-9]{5}",
        "-[0-9]+",
        "-[a-z0-9]{5}",
    ]
    .iter()
    .find_map(|suffix| pattern.strip_suffix(suffix))
    .unwrap_or(pattern)
}

/// The text of `promql` between `start` and the next `"`.
fn label<'a>(promql: &'a str, start: &str) -> &'a str {
    promql
        .split_once(start)
        .and_then(|(_, rest)| rest.split_once('"'))
        .map_or("", |(value, _)| value)
}

/// A pod's samples over a range: when it existed, and its values.
struct Run {
    name: String,
    from: i64,
    to: i64,
    pod: DemoPod,
}

/// What the sample store answers: Engawa's probe, and its range queries for the demo's pods.
/// `ENGAWA_DEMO_FORBID=services/proxy` refuses it, as kube-apiserver does without the right.
#[expect(
    clippy::cast_precision_loss,
    reason = "Unix seconds and indexes are far below 2^52"
)]
#[expect(
    clippy::cast_possible_truncation,
    reason = "sample restart times, rounded to a second"
)]
fn store(path: &str) -> Result<Value, engawa_core::problem::KubernetesStatus> {
    if std::env::var("ENGAWA_DEMO_FORBID")
        .is_ok_and(|list| list.split(',').any(|r| r == "services/proxy"))
    {
        return Err(engawa_core::problem::KubernetesStatus::new(
            403,
            "Forbidden",
            "services \"vmsingle-vm-victoria-metrics-k8s-stack:8428\" is forbidden: User \
             \"alice@example.com\" cannot get resource \"services/proxy\" in API group \"\" in \
             the namespace \"observability\"",
        ));
    }
    let (route, query) = path.split_once('?').unwrap_or((path, ""));
    if route == "/api/v1/status/buildinfo" {
        return Ok(json!({"status": "success", "data": {"version": "1.153.0"}}));
    }
    let params: BTreeMap<String, String> = form_urlencoded::parse(query.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let number = |key: &str| {
        params
            .get(key)
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0)
    };
    let (start, end, step) = (number("start"), number("end"), number("step").max(1));
    let promql = params.get("query").map_or("", String::as_str);
    let namespace = label(promql, "namespace=\"");
    let service = pattern_name(label(promql, "pod=~\"")).replace("[.]", ".");
    let rollout = end - 30 * 3600;
    let mut runs = Vec::new();
    for (i, pod) in PODS
        .get()
        .into_iter()
        .flatten()
        .filter(|p| p.namespace == namespace && p.app == service)
        .enumerate()
    {
        let rolled = !pod.stateful && start < rollout;
        runs.push(Run {
            name: pod.name.clone(),
            from: if rolled { rollout } else { i64::MIN },
            to: end,
            pod: pod.clone(),
        });
        if rolled {
            let tail = format!("{:05x}", hash(&format!("old:{}", pod.name)) % 0xF_FFFF);
            runs.push(Run {
                name: format!("{service}-5b1a{i}-{tail}"),
                from: i64::MIN,
                to: rollout,
                pod: pod.clone(),
            });
        }
    }
    let leaks = service == "worker";
    let period = 9 * 3600;
    let result: Vec<Value> = runs
        .iter()
        .map(|run| {
            let (cpu_request, memory_limit) = room(run.pod.stateful);
            let (cpu_share, memory_share) = shares(&run.pod.app);
            let mut noise = Noise::new(&format!("history:{}:{}", run.name, end - start));
            let phase = (Noise::new(&format!("phase:{}", run.name)).next() * period as f64) as i64;
            let mut walk: f64 = 1.0;
            let restarted: Vec<i64> = (0..run.pod.restarts.min(4))
                .map(|r| {
                    let ago =
                        (f64::from(r).mul_add(0.3, 0.15) * (end - start).min(86_400) as f64) as i64;
                    (end - ago) / step * step
                })
                .collect();
            let mut values = Vec::new();
            let mut first_oom = None;
            for t in (start..=end).step_by(usize::try_from(step).unwrap_or(1)) {
                if t < run.from || t > run.to || (!run.pod.ready && t > end - 40 * 60) {
                    continue;
                }
                walk = (noise.next() - 0.5).mul_add(0.25, walk).clamp(0.3, 1.8);
                let daily = 0.35_f64.mul_add(
                    ((t % 86_400) as f64 / 86_400.0)
                        .mul_add(std::f64::consts::TAU, -1.2)
                        .sin(),
                    1.0,
                );
                let cycle = (t + phase) / period;
                let killed = leaks && (t - step + phase) / period != cycle;
                if killed && first_oom.is_none() {
                    first_oom = Some(t);
                }
                let value = if promql.contains("container_cpu_usage_seconds_total") {
                    cpu_request * cpu_share * walk * daily / 1000.0
                } else if promql.contains("container_memory_working_set_bytes") {
                    if leaks {
                        let into = ((t + phase) % period) as f64 / period as f64;
                        memory_limit * into.mul_add(0.64, 0.35) * noise.next().mul_add(0.03, 0.98)
                    } else {
                        memory_limit
                            * memory_share
                            * noise.next().mul_add(0.08, 0.9)
                            * daily.mul_add(0.05, 0.97)
                    }
                } else if promql.contains("restarts_total") {
                    f64::from(u8::from(killed || (!leaks && restarted.contains(&t))))
                } else {
                    f64::from(u8::from(first_oom.is_some()))
                };
                values.push(json!([t, value.to_string()]));
            }
            json!({"metric": {"pod": run.name}, "values": values})
        })
        .collect();
    Ok(json!({"status": "success", "data": {"resultType": "matrix", "result": result}}))
}

/// Three nodes: a small control plane, a cordoned worker and a worker one minor behind.
fn nodes() -> Vec<Node> {
    let node = |name: &str,
                roles: &[&str],
                kubelet: &str,
                cpu: &str,
                memory: &str,
                allocatable_memory: &str,
                cordoned: bool,
                age_days: i64|
     -> Node {
        let mut labels = serde_json::Map::new();
        labels.insert("kubernetes.io/hostname".to_owned(), json!(name));
        for role in roles {
            labels.insert(format!("node-role.kubernetes.io/{role}"), json!("true"));
        }
        let created = Timestamp::now()
            .checked_sub(SignedDuration::from_hours(24 * age_days))
            .unwrap_or_else(|_| Timestamp::now())
            .to_string();
        let taints = if cordoned {
            json!([{"key": "node.kubernetes.io/unschedulable", "effect": "NoSchedule"}])
        } else {
            json!([])
        };
        from(json!({
            "metadata": {"name": name, "labels": labels, "creationTimestamp": created},
            "spec": {"unschedulable": cordoned, "taints": taints},
            "status": {
                "capacity": {"cpu": cpu, "memory": memory, "pods": "110"},
                "allocatable": {"cpu": cpu, "memory": allocatable_memory, "pods": "110"},
                "conditions": [
                    {"type": "Ready", "status": "True", "reason": "KubeletReady"},
                    {"type": "MemoryPressure", "status": "False"},
                    {"type": "DiskPressure", "status": "False"},
                    {"type": "PIDPressure", "status": "False"}
                ],
                "nodeInfo": {
                    "kubeletVersion": kubelet, "kubeProxyVersion": "",
                    "osImage": "Debian GNU/Linux 13 (trixie)", "kernelVersion": "6.12.48+deb13-arm64",
                    "architecture": "arm64", "operatingSystem": "linux",
                    "containerRuntimeVersion": "containerd://2.1.4-k3s1",
                    "machineID": "", "systemUUID": "", "bootID": ""
                }
            }
        }))
    };
    vec![
        node(
            "vps-1",
            &["control-plane", "etcd"],
            VERSION,
            "2",
            "2048Mi",
            "1900Mi",
            false,
            210,
        ),
        node("vps-2", &[], VERSION, "4", "8Gi", "7800Mi", true, 96),
        node(
            "vps-3",
            &[],
            "v1.33.13+k3s1",
            "4",
            "8Gi",
            "7800Mi",
            false,
            31,
        ),
    ]
}

/// k3s's own pods in kube-system.
fn kube_system() -> NamespaceObjects {
    let pod = |name: &str, cpu: &str, memory: &str| -> Pod {
        from(
            json!({"metadata": {"name": name, "namespace": "kube-system", "labels": {"app": name}},
            "spec": {"containers": [{"name": "main", "image": "example",
                "resources": {"requests": {"cpu": cpu, "memory": memory}}}]},
            "status": {"phase": "Running", "containerStatuses": [{"name": "main", "ready": true,
                "restartCount": 0, "image": "example", "imageID": "", "state": {"running": {}}}]}}),
        )
    };
    NamespaceObjects {
        namespace: "kube-system".to_owned(),
        pods: vec![
            pod("coredns-7b98449c4-x2kqp", "100m", "70Mi"),
            pod("traefik-5c8f7d6b9-ltm4z", "100m", "128Mi"),
            pod("metrics-server-6f4c6675d5-9hq2r", "100m", "70Mi"),
            pod("local-path-provisioner-595dcfc56f-pk7jw", "50m", "64Mi"),
        ],
        ..NamespaceObjects::default()
    }
}

/// Puts each pod on a node (control plane first) and gives the example pods the requests of
/// their workloads.
fn place(pods: &mut [Pod]) {
    let requests = |app: &str| match app {
        "db" | "redis" => Some(("250m", "512Mi")),
        "web" | "api" | "worker" => Some(("100m", "128Mi")),
        _ => None,
    };
    for (i, pod) in pods.iter_mut().enumerate() {
        let Some(spec) = pod.spec.as_mut() else {
            continue;
        };
        spec.node_name = Some(if i % 3 == 2 { "vps-3" } else { "vps-1" }.to_owned());
        let app = pod
            .metadata
            .labels
            .as_ref()
            .and_then(|l| l.get("app"))
            .cloned()
            .unwrap_or_default();
        if let (Some((cpu, memory)), Some(container)) =
            (requests(&app), spec.containers.first_mut())
        {
            container.resources = Some(from(
                json!({"requests": {"cpu": cpu, "memory": memory}, "limits": {"memory": "1Gi"}}),
            ));
        }
    }
}

/// One connection for every request, offering admin rights when `offered`.
struct Demo {
    cluster: Arc<dyn Cluster>,
    offered: bool,
}

impl ClusterAccess for Demo {
    fn for_request(&self, _parts: &Parts) -> Result<Access, Box<Problem>> {
        Ok(Access {
            cluster: Arc::clone(&self.cluster),
            elevation: if self.offered {
                Elevation::Available
            } else {
                Elevation::Unavailable
            },
        })
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cluster = MemoryCluster::new(state());
    if std::env::var("ENGAWA_DEMO_FORBID")
        .is_ok_and(|list| list.split(',').any(|r| r == "pods/exec"))
    {
        cluster.update(|s| {
            let pods: Vec<String> = s
                .objects
                .iter()
                .flat_map(|(namespace, objects)| {
                    objects.pods.iter().filter_map(move |pod| {
                        Some(format!("{namespace}/{}", pod.metadata.name.as_deref()?))
                    })
                })
                .collect();
            s.exec_refused.extend(pods);
        });
    }
    let access = Demo {
        cluster: Arc::new(cluster),
        offered: std::env::var_os("ENGAWA_DEMO_ELEVATION").is_some(),
    };
    // Settings are kept in an `engawa` namespace, as on the in-cluster server.
    let settings = engawa_api::Settings {
        namespace: Some("engawa".to_owned()),
        ..engawa_api::Settings::default()
    };
    let app = engawa_api::router_with_settings(Arc::new(access), settings)
        .fallback_service(engawa_ui::router());
    // Another port when 8080 is taken: ENGAWA_DEMO_ADDR=127.0.0.1:8090.
    let addr = std::env::var("ENGAWA_DEMO_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await
}
