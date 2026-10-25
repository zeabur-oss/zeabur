//! A development server: the API over an in-memory cluster holding a few example projects (one
//! with a crash-looping worker, one whose image cannot be pulled, one with a failed CronJob run,
//! one rolling out, a large healthy one and a small one), and the embedded web interface. Never
//! shipped; for working on and trying the interface without a cluster.
//!
//! ```sh
//! scripts/build-ui.sh && ZEABUR_UI_DIST=$PWD/web/dist \
//!   cargo run -p zeabur-api --example demo        # interface and API on http://127.0.0.1:8080
//!                                                 # (ZEABUR_DEMO_ADDR picks another address)
//! (cd web && bun run dev)                          # or Vite's dev server on :5173, proxying /api
//! ```
//!
//! `ZEABUR_DEMO_FORBID=events.events.k8s.io,pods` refuses those cluster-wide reads (`nodes`,
//! `pods`, `nodes.metrics.k8s.io`, `events.events.k8s.io`, `plans.upgrade.cattle.io`), to see a
//! section's permission state; the listed kinds but `pods` are also refused in every namespace
//! (`secrets`, `httproutes.gateway.networking.k8s.io`: a project's pages show them refused in
//! place), and `pods/exec` refuses every terminal. `services/proxy` in `ZEABUR_DEMO_FORBID` refuses
//! the sample metrics store, whose history and disk use are made up like the live usage. A
//! system-upgrade-controller Plan resolves a newer patch, so the cluster page shows one; the demo
//! makes no outbound request: domains resolve from a fixed table (`dns`), and every image is
//! read from a made-up registry (`Registry`) that says it listens on 8080 as user 101.
//!
//! Velero runs with a location on R2, a daily schedule and a week of backups, the newest failed:
//! `ZEABUR_DEMO_VELERO=none` leaves it uninstalled, `location` keeps only the location. Each backup
//! holds the projects as the demo starts and `wiki`, deleted since. A store on 127.0.0.1 answers
//! the `DownloadRequest`s, so restore plans list what a backup holds.
//!
//! K8up backs up shop's Redis volume nightly to R2; the newest run failed. `ZEABUR_DEMO_K8UP=none`
//! leaves it uninstalled. The cluster lists one S3 destination, `r2-backups`.
//!
//! Alerting sends shop's deploy failures to Slack and the cluster's node alerts to Telegram; a
//! second shop channel was rejected by Alertmanager. `ZEABUR_DEMO_ALERTING=none` leaves it
//! uninstalled.
//!
//! `storageclasses.storage.k8s.io` and `persistentvolumes` in `ZEABUR_DEMO_FORBID` refuse
//! reading the storage classes (local-path, the default, and Longhorn) and the volumes behind
//! claims.
//!
//! `ZEABUR_DEMO_KEEL=1` installs Keel, so a project offers Deploy from Git.
//!
//! `ZEABUR_DEMO_GITHUB=1` starts a fake GitHub on 127.0.0.1, so Cluster → GitHub App goes through
//! the manifest flow without leaving the demo. `ZEABUR_DEMO_GITHUB=installed` also makes the App
//! at the start: it is installed on `acme` and `bob`, and `acme` is granted to `shop`.
//!
//! The Error pages template has a made-up idle use of 9 MiB, so turning on a domain's error pages
//! says the cluster has room; `ZEABUR_DEMO_TEMPLATE_IDLE=large` makes it 24 GiB, to see it tight.
//!
//! `namespaces` in `ZEABUR_DEMO_FORBID` refuses creating projects. New public URLs are suggested
//! under `apps.example.com` with HTTPS from the `ClusterIssuer` `letsencrypt`;
//! `ZEABUR_DEMO_NO_DOMAINS=1` leaves both unset.
//!
//! `ZEABUR_DEMO_RKE2=1` makes the cluster an RKE2 one installed by Zeabur Installer: the nodes
//! have addresses and a location, and the control plane runs as static pods.
//!
//! The `web` pods have no shell, so their terminal offers a debug container, which starts a moment
//! after it is added, as do tools pods and node shells. With `ZEABUR_DEMO_RKE2=1`, `vps-2` also has
//! a node shell Bob left behind, which ended when its time ran out; a node shell's terminal is the
//! in-memory cluster's echo shell, with a root prompt. `ZEABUR_DEMO_TOOLS=1` hands the session a
//! made-up token, so the kubectl terminal is offered: a project's, and the whole cluster's from the
//! Cluster page's header.
//!
//! Node maintenance: `vps-3` holds Zeabur itself, a bare pod (`lab/debug-toolbox`) and `shop`'s
//! `db-0` on a local-path volume pinned to it, so the node list counts what a drain would move and
//! what it would leave.

#![expect(
    clippy::expect_used,
    reason = "fixed example data; a typo should stop the demo"
)]
#![expect(
    clippy::too_many_lines,
    reason = "the example data is long, literal and read top to bottom"
)]

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::http::request::Parts;
use secrecy::SecretString;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use zeabur_api::access::{ClusterAccess, Handoff};
use zeabur_api::images::registry::{Answer, Get, Transport};
use zeabur_core::problem::Problem;
use zeabur_kube::k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use zeabur_kube::k8s_openapi::api::batch::v1::{CronJob, Job};
use zeabur_kube::k8s_openapi::api::core::v1::{
    ConfigMap, Namespace, Node, PersistentVolumeClaim, Pod, Secret, Service,
};
use zeabur_kube::k8s_openapi::api::events::v1::Event;
use zeabur_kube::k8s_openapi::api::networking::v1::Ingress;
use zeabur_kube::k8s_openapi::api::storage::v1::StorageClass;
use zeabur_kube::k8s_openapi::jiff::{SignedDuration, Timestamp};
use zeabur_kube::memory::{MemoryCluster, State};
use zeabur_kube::{
    Cluster, ClusterFacts, DynamicObject, Identity, NamespaceObjects, SecretSummary, ServedResource,
};
use zeabur_modules::alertmanager as am;

fn from<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("valid example object")
}

/// One of Zeabur Identity's objects in `zeabur-identity`, labelled with its user when it has one.
fn identity_object(
    kind: &str,
    name: &str,
    created: &str,
    spec: &Value,
    extra: &Value,
) -> DynamicObject {
    let labels = spec.get("user").and_then(Value::as_str).map_or_else(
        || json!({}),
        |user| json!({"identity.zeabur.com/user": user}),
    );
    let annotations = extra
        .get("annotations")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let metadata = json!({"name": name, "namespace": "zeabur-identity",
        "uid": format!("identity-{name}"), "creationTimestamp": created,
        "labels": labels, "annotations": annotations});
    let status = extra.get("status").cloned().unwrap_or_else(|| json!({}));
    from(
        json!({"apiVersion": "identity.zeabur.com/v1alpha1", "kind": kind,
        "metadata": metadata, "spec": spec, "status": status}),
    )
}

/// Zeabur Identity's people: Alice (an admin with two passkeys), Bob (one passkey and a link
/// that adds another), Carol (GitHub and a passkey), and Dan, invited and not yet signed in.
fn people() -> Vec<DynamicObject> {
    let now = Timestamp::now().as_second();
    let passkey = |name: &str, user: &str, created: &str, label: &str, extra: &Value| {
        identity_object(
            "Credential",
            name,
            created,
            &json!({"user": user, "type": "Passkey",
            "passkey": {"credentialId": name, "record": "{}", "backupEligible": false,
                        "label": label}}),
            extra,
        )
    };
    let user = |name: &str, display: &str, admin: bool, created: &str| {
        let account_type = if admin { "Admin" } else { "Regular" };
        identity_object(
            "User",
            name,
            created,
            &json!({"username": name, "displayName": display,
            "groups": [], "accountType": account_type}),
            &json!({}),
        )
    };
    let grant = |name: &str, user: &str, ago: i64| {
        identity_object(
            "Grant",
            name,
            "2026-10-01T00:00:00Z",
            &json!({"user": user,
            "client": "zeabur", "audience": "zeabur", "authTime": now - ago,
            "expiresAt": now + 86_400}),
            &json!({}),
        )
    };
    let invitation = |name: &str, user: &str, days: i64| {
        identity_object(
            "Invitation",
            name,
            "2026-10-20T09:00:00Z",
            &json!({"user": user,
            "tokenHash": "sha256:0000", "expiresAt": now + days * 86_400}),
            &json!({}),
        )
    };
    vec![
        user("alice", "Alice Wang", true, "2026-03-02T08:00:00Z"),
        user("bob", "Bob Chen", false, "2026-05-11T10:30:00Z"),
        user("carol", "Carol Lin", false, "2026-07-21T04:12:00Z"),
        user("dan", "", false, "2026-10-20T09:00:00Z"),
        passkey(
            "cred-alice-mac",
            "alice",
            "2026-03-02T08:01:00Z",
            "MacBook Pro",
            &json!({"status": {"lastUsedAt": now - 600}}),
        ),
        passkey(
            "cred-alice-phone",
            "alice",
            "2026-03-02T08:05:00Z",
            "iPhone",
            &json!({}),
        ),
        passkey(
            "cred-bob-key",
            "bob",
            "2026-05-11T10:31:00Z",
            "YubiKey 5C",
            &json!({}),
        ),
        passkey(
            "cred-carol-laptop",
            "carol",
            "2026-07-21T04:13:00Z",
            "",
            &json!({}),
        ),
        identity_object(
            "Credential",
            "cred-carol-github",
            "2026-07-22T02:00:00Z",
            &json!({"user": "carol", "type": "GitHub",
                    "external": {"issuer": "https://github.com", "subject": "1048576"}}),
            &json!({}),
        ),
        grant("grant-alice", "alice", 600),
        grant("grant-bob", "bob", 3 * 86_400),
        invitation("inv-3f9a1c0d7e2b4a6f", "dan", 6),
        invitation("inv-8c2e5b1a9d4f7e30", "bob", 1),
    ]
}

/// What Bob and Carol may do: Bob develops `shop` and views `blog`, Carol views five projects.
fn member_roles() -> Vec<DynamicObject> {
    use zeabur_api::members::access::{ProjectRole, baseline_binding, role_binding};
    let roles = [
        ("bob", "shop", ProjectRole::Developer),
        ("bob", "blog", ProjectRole::Viewer),
        ("carol", "shop", ProjectRole::Viewer),
        ("carol", "blog", ProjectRole::Viewer),
        ("carol", "billing", ProjectRole::Viewer),
        ("carol", "payments", ProjectRole::Viewer),
        ("carol", "analytics", ProjectRole::Viewer),
    ];
    roles
        .into_iter()
        .map(|(user, project, role)| from(role_binding(project, user, role)))
        .chain(["alice", "bob", "carol", "dan"].map(|user| {
            // The memory cluster keeps a cluster-scoped object in namespace "".
            let mut binding = baseline_binding(user);
            if let Some(metadata) = binding.get_mut("metadata").and_then(Value::as_object_mut) {
                metadata.insert("namespace".to_owned(), "".into());
            }
            from(binding)
        }))
        .collect()
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
            "creationTimestamp": created(name),
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
        "metadata": {"name": name, "namespace": namespace, "creationTimestamp": created(name),
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

/// A member of a database in `shop`, labelled as its operator labels it, with no workload.
fn member(name: &str, container: &str, labels: &[(&str, &str)]) -> Pod {
    let labels: serde_json::Map<String, Value> = labels
        .iter()
        .map(|(k, v)| ((*k).to_owned(), Value::from(*v)))
        .collect();
    from(json!({
        "metadata": {"name": name, "namespace": "shop", "labels": labels,
            "creationTimestamp": minutes_ago(2900)},
        "spec": {"containers": [{"name": container, "image": "example"}], "nodeName": "vps-1"},
        "status": running(container, 0)
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

/// The address Traefik writes into every Ingress's status.
const ENTRY: &str = "203.0.113.10";

/// An Ingress in `namespace` routing `host` to the Service `backend`, with TLS.
fn ingress(namespace: &str, host: &str, backend: &str) -> Ingress {
    from(
        json!({"metadata": {"name": backend, "namespace": namespace},
        "spec": {"rules": [{"host": host, "http": {"paths": [{"path": "/", "pathType": "Prefix",
            "backend": {"service": {"name": backend, "port": {"name": "http"}}}}]}}],
            "tls": [{"hosts": [host], "secretName": format!("{backend}-tls")}]},
        "status": {"loadBalancer": {"ingress": [{"ip": ENTRY}]}}}),
    )
}

/// A cert-manager Certificate for the Secret `secret`, with `conditions`.
fn certificate(namespace: &str, secret: &str, host: &str, conditions: &Value) -> DynamicObject {
    from(json!({
        "apiVersion": "cert-manager.io/v1", "kind": "Certificate",
        "metadata": {"name": secret, "namespace": namespace},
        "spec": {"secretName": secret, "dnsNames": [host]},
        "status": {"conditions": conditions, "notAfter": "2027-01-21T08:00:00Z"}
    }))
}

/// A registry holding every image, one platform each, listening on 8080 as user 101.
struct Registry {
    manifest: Vec<u8>,
    config: Vec<u8>,
    digest: String,
}

impl Registry {
    fn new() -> Self {
        let config = serde_json::to_vec(&json!({
            "architecture": "arm64", "os": "linux",
            "config": {"ExposedPorts": {"8080/tcp": {}}, "User": "101"},
        }))
        .expect("example config");
        let digest = format!("sha256:{}", hex::encode(Sha256::digest(&config)));
        let manifest = serde_json::to_vec(&json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
            "config": {"digest": digest, "size": config.len()},
            "layers": [],
        }))
        .expect("example manifest");
        Self {
            manifest,
            config,
            digest,
        }
    }
}

impl Transport for Registry {
    fn get<'a>(
        &'a self,
        get: Get,
    ) -> Pin<Box<dyn Future<Output = Result<Answer, String>> + Send + 'a>> {
        let body = if get.url.contains("/manifests/") {
            Some(self.manifest.clone())
        } else if get.url.ends_with(&format!("/blobs/{}", self.digest)) {
            Some(self.config.clone())
        } else {
            None
        };
        let answer = body.map_or_else(
            || Answer {
                status: 404,
                ..Answer::default()
            },
            |body| Answer {
                status: 200,
                body,
                ..Answer::default()
            },
        );
        Box::pin(async move { Ok(answer) })
    }
}

/// What the demo's DNS answers: shop and the app domain point at the cluster, its API not yet,
/// billing behind a CDN, an old sslip.io address elsewhere, and the rest nowhere.
fn dns() -> zeabur_api::resolve::Fixed {
    zeabur_api::resolve::Fixed(BTreeMap::from(
        [
            ("shop.example.com", ENTRY),
            ("blog.example.com", ENTRY),
            ("billing.example.com", "198.51.100.24"),
            ("pay.example.com", ENTRY),
            ("stats.example.com", ENTRY),
            ("*.apps.example.com", ENTRY),
            ("*.203-0-113-7.sslip.io", "203.0.113.7"),
        ]
        .map(|(name, address)| (name.to_owned(), vec![address.to_owned()])),
    ))
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
            {"name": "ORDERS_DATABASE_URL", "valueFrom": secret("orders-app", "uri")},
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
    let mut api = api;
    if let Some(status) = api.status.as_mut() {
        status.conditions = Some(from(json!([
            {"type": "Available", "status": "True", "reason": "MinimumReplicasAvailable",
                "message": "Deployment has minimum availability.",
                "lastTransitionTime": minutes_ago(50)},
            {"type": "Progressing", "status": "True", "reason": "NewReplicaSetAvailable",
                "message": "ReplicaSet \"api-5f6b7\" has successfully progressed.",
                "lastTransitionTime": minutes_ago(52)}
        ])));
    }
    let mut web = web;
    if let Some(container) = web
        .spec
        .as_mut()
        .and_then(|s| s.template.spec.as_mut())
        .and_then(|s| s.containers.first_mut())
    {
        container.env_from = Some(vec![
            from(json!({"configMapRef": {"name": "web-config"}})),
            from(json!({"configMapRef": {"name": "shared-config"}})),
        ]);
    }
    if let Some(container) = api
        .spec
        .as_mut()
        .and_then(|s| s.template.spec.as_mut())
        .and_then(|s| s.containers.first_mut())
    {
        container.env_from = Some(vec![from(
            json!({"configMapRef": {"name": "shared-config"}}),
        )]);
        container.env.get_or_insert_with(Vec::new).push(from(
            json!({"name": "SENTRY_DSN", "valueFrom": secret("shared-keys", "SENTRY_DSN")}),
        ));
    }
    web.metadata.annotations = Some(BTreeMap::from([(
        zeabur_model::ConfigReload::AUTO.to_owned(),
        "true".to_owned(),
    )]));
    let mut db = stateful_set(
        "db",
        "postgres:18",
        "20Gi",
        &json!([
            {"name": "POSTGRES_PASSWORD", "valueFrom": secret("db-credentials", "password")}
        ]),
    );
    if let Some(spec) = db.spec.as_mut().and_then(|s| s.template.spec.as_mut()) {
        spec.containers.push(from(json!({
            "name": "backup-agent",
            "image": "ghcr.io/example/backup-agent:1.4.0",
            "env": [
                {"name": "BACKUP_SCHEDULE", "value": "0 3 * * *"},
                {"name": "BACKUP_DATABASE_PASSWORD", "valueFrom": secret("db-credentials", "password")}
            ],
            "envFrom": [{"secretRef": {"name": "db-credentials"}}]
        })));
    }
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
        deployments: vec![
            web,
            api,
            worker,
            deployment(
                "error-pages",
                "docker.io/nginxinc/nginx-unprivileged:1.30.5-alpine-slim",
                1,
                1,
                &json!([]),
            ),
        ],
        stateful_sets: vec![db, redis],
        cron_jobs: vec![nightly],
        pods: vec![
            pod("web-7c9d8-abcde", "web", true, 0, None),
            pod("error-pages-5d4f7-k2m9x", "error-pages", true, 0, None),
            pod("web-7c9d8-fghij", "web", true, 2, None),
            pod("api-5f6b7-k2m4p", "api", true, 0, None),
            pod("api-5f6b7-q8r1s", "api", true, 0, None),
            pod("api-5f6b7-x9z3t", "api", true, 1, None),
            pod("worker-84c2d-h7j6k", "worker", true, 0, None),
            pod("worker-84c2d-w2e5r", "worker", false, 14, None),
            pod("db-0", "db", true, 0, Some("db")),
            pod("redis-0", "redis", true, 0, Some("redis")),
            member(
                "orders-1",
                "postgres",
                &[
                    ("cnpg.io/cluster", "orders"),
                    ("cnpg.io/instanceName", "orders-1"),
                    ("cnpg.io/instanceRole", "primary"),
                ],
            ),
            member(
                "sessions-rs0-0",
                "mongod",
                &[
                    ("app.kubernetes.io/instance", "sessions"),
                    ("app.kubernetes.io/component", "mongod"),
                ],
            ),
            member(
                "sessions-rs0-1",
                "mongod",
                &[
                    ("app.kubernetes.io/instance", "sessions"),
                    ("app.kubernetes.io/component", "mongod"),
                ],
            ),
            member(
                "sessions-rs0-2",
                "mongod",
                &[
                    ("app.kubernetes.io/instance", "sessions"),
                    ("app.kubernetes.io/component", "mongod"),
                ],
            ),
        ],
        services: vec![
            service("web", 80, "http"),
            service("api", 8080, "http"),
            service("db", 5432, "postgres"),
            from(
                json!({"metadata": {"name": "error-pages", "namespace": "shop",
                    "labels": {"zeabur.com/page-server": "true"}},
                "spec": {"selector": {"app": "error-pages"}, "ports": [
                    {"name": "pages", "port": 8080, "targetPort": "http"},
                    {"name": "maintenance", "port": 8081, "targetPort": "http"}]}}),
            ),
            service("redis", 6379, "redis"),
            from(
                json!({"metadata": {"name": "redis-public-6379", "namespace": "shop",
                    "labels": {"app.kubernetes.io/name": "redis"}},
                "spec": {"type": "LoadBalancer", "selector": {"app": "redis"},
                    "ports": [{"name": "redis", "port": 6379, "targetPort": 6379,
                        "protocol": "TCP", "nodePort": 31379}]},
                "status": {"loadBalancer": {"ingress": [{"ip": ENTRY}]}}}),
            ),
        ],
        config_maps: vec![
            from::<ConfigMap>(
                json!({"metadata": {"name": "error-pages", "namespace": "shop",
                    "labels": {"zeabur.com/page-server": "true"}},
                "data": {
                    "502.html": "<!doctype html><title>Shop</title><h1>Shop is not responding</h1>",
                    "503.html": "<!doctype html><title>Shop</title><h1>Shop is not available right now</h1>",
                    "504.html": "<!doctype html><title>Shop</title><h1>Shop took too long to respond</h1>",
                    "maintenance.html": "<!doctype html><title>Shop</title><h1>Shop is under maintenance</h1><p>We will be back soon.</p>"}}),
            ),
            from::<ConfigMap>(
                json!({"metadata": {"name": "web-config", "namespace": "shop"},
                "data": {"LOG_LEVEL": "info", "FEATURE_CHECKOUT_V2": "true"}}),
            ),
            from::<ConfigMap>(
                json!({"metadata": {"name": "shared-config", "namespace": "shop",
                    "labels": {"zeabur.com/shared-variables": "true"}},
                "data": {"APP_URL": "https://shop.example.com", "TZ": "Asia/Taipei"}}),
            ),
        ],
        secrets: vec![
            SecretSummary {
                secret: Secret {
                    metadata: from(json!({"name": "db-credentials", "namespace": "shop"})),
                    type_: Some("Opaque".to_owned()),
                    ..Secret::default()
                },
                key_count: 2,
            },
            SecretSummary {
                secret: Secret {
                    metadata: from(json!({"name": "shared-keys", "namespace": "shop",
                        "labels": {"zeabur.com/shared-variables": "true"}})),
                    type_: Some("Opaque".to_owned()),
                    ..Secret::default()
                },
                key_count: 2,
            },
        ],
        persistent_volume_claims: vec![
            from::<PersistentVolumeClaim>(
                json!({"metadata": {"name": "data-db-0", "namespace": "shop"},
                "spec": {"accessModes": ["ReadWriteOnce"], "resources": {"requests": {"storage": "20Gi"}}},
                "status": {"phase": "Bound", "capacity": {"storage": "20Gi"}}}),
            ),
            from::<PersistentVolumeClaim>(
                json!({"metadata": {"name": "data-redis-0", "namespace": "shop",
                    "labels": {"zeabur.com/backup-volume": "data-redis-0"},
                    "annotations": {"k8up.io/backup": "true"}},
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
            // www redirects to the apex, and both show the error pages, through Traefik
            // middlewares.
            from::<Ingress>(json!({"metadata": {"name": "web", "namespace": "shop",
                    "uid": "0e9b1c44-6a43-4c55-9d0b-7f3f2a1d0001",
                    "annotations": {"traefik.ingress.kubernetes.io/router.middlewares":
                        "shop-web-redirect-www-shop-example-com@kubernetescrd,shop-web-error-pages@kubernetescrd",
                        "cert-manager.io/cluster-issuer": "letsencrypt"}},
                "spec": {"rules": [{"host": "shop.example.com", "http": {"paths": [{"path": "/", "pathType": "Prefix",
                    "backend": {"service": {"name": "web", "port": {"name": "http"}}}}]}},
                    {"host": "www.shop.example.com", "http": {"paths": [{"path": "/", "pathType": "Prefix",
                    "backend": {"service": {"name": "web", "port": {"name": "http"}}}}]}}],
                    "tls": [{"hosts": ["shop.example.com", "www.shop.example.com"], "secretName": "shop-tls"}]},
                "status": {"loadBalancer": {"ingress": [{"ip": ENTRY}]}}})),
            // The API shows the maintenance page, through an Ingress of its own that outranks it.
            from::<Ingress>(json!({"metadata": {"name": "api", "namespace": "shop",
                    "uid": "0e9b1c44-6a43-4c55-9d0b-7f3f2a1d0002",
                    "annotations": {"cert-manager.io/cluster-issuer": "letsencrypt"}},
                "spec": {"rules": [{"host": "api.shop.example.com", "http": {"paths": [{"path": "/", "pathType": "Prefix",
                    "backend": {"service": {"name": "api", "port": {"name": "http"}}}}]}}],
                    "tls": [{"hosts": ["api.shop.example.com"], "secretName": "api-tls"}]},
                "status": {"loadBalancer": {"ingress": [{"ip": ENTRY}]}}})),
            from::<Ingress>(
                json!({"metadata": {"name": "api-api-shop-example-com-maintenance", "namespace": "shop",
                    "ownerReferences": [{"apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
                        "name": "api", "uid": "0e9b1c44-6a43-4c55-9d0b-7f3f2a1d0002"}],
                    "annotations": {"traefik.ingress.kubernetes.io/router.priority": "48"}},
                "spec": {"rules": [{"host": "api.shop.example.com", "http": {"paths": [{"path": "/", "pathType": "Prefix",
                    "backend": {"service": {"name": "error-pages", "port": {"name": "maintenance"}}}}]}}],
                    "tls": [{"hosts": ["api.shop.example.com"], "secretName": "api-tls"}]},
                "status": {"loadBalancer": {"ingress": [{"ip": ENTRY}]}}}),
            ),
        ],
        // A Gateway API route: redirects and maintenance work, a password and error pages don't.
        http_routes: vec![json!({
            "apiVersion": "gateway.networking.k8s.io/v1", "kind": "HTTPRoute",
            "metadata": {"name": "web-status", "namespace": "shop", "resourceVersion": "2"},
            "spec": {"parentRefs": [{"name": "public", "namespace": "gateway"}],
                "hostnames": ["status.shop.example.com"],
                "rules": [{"backendRefs": [{"name": "web", "port": 80}]}]}
        })],
        ..NamespaceObjects::default()
    }
}

/// A blog whose new image cannot be pulled, and its database.
/// Blog's docs, deployed from Git: its first image is still being built
/// (`ZEABUR_DEMO_BUILD_FAILED=1`: the build failed).
const DOCS_IMAGE: &str = "zot.zot.svc:5000/blog/docs:pending";

fn docs_build() -> [DynamicObject; 2] {
    let failed = std::env::var_os("ZEABUR_DEMO_BUILD_FAILED").is_some();
    let build = json!({
        "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
        "metadata": {"name": "docs", "namespace": "blog"},
        "spec": {"builderRef": "railpack",
            "source": {"git": {"url": "https://github.com/example/docs.git", "ref": "main",
                "contextDir": "site"}},
            "output": {"image": "zot.zot.svc:5000/blog/docs"},
            "env": [{"name": "NPM_TOKEN", "valueFrom": {"secretKeyRef":
                {"name": "docs-build-env", "key": "NPM_TOKEN"}}}],
            "notify": [{"type": "keel",
                "url": "http://keel.keel.svc.cluster.local:9300/v1/webhooks/native"}]},
        "status": if failed {
            json!({"conditions": [{"type": "Ready", "status": "False", "reason": "BuildFailed",
                "message": "railpack: no start command found"}]})
        } else {
            json!({})
        }
    });
    let run = json!({
        "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "BuildRun",
        "metadata": {"name": "docs-x7k2p", "namespace": "blog",
            "creationTimestamp": "2026-10-23T08:00:00Z"},
        "spec": {"buildRef": "docs", "revision": "3f9a1c2b7d4e5f60718293a4b5c6d7e8f9012345",
            "trigger": "poll"},
        "status": if failed {
            json!({"phase": "Failed", "reason": "PlanFailed",
                "message": "railpack: no start command found. Add a start script to package.json."})
        } else {
            json!({"phase": "Building", "startTime": "2026-10-23T08:00:05Z"})
        }
    });
    [
        serde_json::from_value(build).expect("a Build"),
        serde_json::from_value(run).expect("a BuildRun"),
    ]
}

fn blog() -> NamespaceObjects {
    let ns = "blog";
    NamespaceObjects {
        namespace: ns.to_owned(),
        deployments: vec![
            deployment_in(ns, "web", "ghcr.io/example/blog:2.4.1", 1, 0, &json!([])),
            deployment_in(ns, "docs", DOCS_IMAGE, 1, 0, &json!([])),
        ],
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
            pod_in(
                ns,
                "docs-5b7c9d8f6-m2x4q",
                "docs",
                None,
                &waiting("docs", "ImagePullBackOff", 0),
            ),
        ],
        services: vec![
            service_in(ns, "web", 80, "http"),
            service_in(ns, "docs", 3000, "http"),
            service_in(ns, "db", 5432, "postgres"),
        ],
        ingresses: vec![ingress(ns, "blog.example.com", "web")],
        secrets: vec![SecretSummary {
            secret: Secret {
                metadata: from(json!({"name": "docs-build-env", "namespace": ns})),
                type_: Some("Opaque".to_owned()),
                ..Secret::default()
            },
            key_count: 1,
        }],
        ..NamespaceObjects::default()
    }
}

/// A lab like the one people try Zeabur with first: four services, one crash-looping and one
/// waiting for a volume that does not exist, and a database of each engine with the pods,
/// StatefulSet and volumes their operators make.
fn lab() -> NamespaceObjects {
    let ns = "lab";
    let secret = |name: &str, key: &str| json!({"secretKeyRef": {"name": name, "key": key}});
    let mut editor = deployment_in(
        ns,
        "editor",
        "ghcr.io/example/editor:0.9.0",
        1,
        0,
        &json!([{"name": "MONGOPASSWORD", "valueFrom": secret("mongo-mvjrswvl-custom-user-secret", "app")}]),
    );
    if let Some(spec) = editor.spec.as_mut().and_then(|s| s.template.spec.as_mut()) {
        spec.volumes = Some(from(json!([{"name": "data",
            "persistentVolumeClaim": {"claimName": "editor-e2e-data"}}])));
        if let Some(container) = spec.containers.first_mut() {
            container.volume_mounts = Some(from(json!([{"name": "data", "mountPath": "/data"}])));
        }
    }
    let operated = |api: &str, kind: &str, name: &str| {
        json!([{"apiVersion": api, "kind": kind, "name": name,
            "uid": "00000000-0000-0000-0000-000000000001", "controller": true}])
    };
    let mongo: StatefulSet = from(json!({
        "metadata": {"name": "mongo-mvjrswvl-rs0", "namespace": ns,
            "creationTimestamp": created("mongo-mvjrswvl"),
            "labels": {"app.kubernetes.io/name": "percona-server-mongodb",
                "app.kubernetes.io/instance": "mongo-mvjrswvl",
                "app.kubernetes.io/replset": "rs0", "app.kubernetes.io/component": "mongod"},
            "ownerReferences": operated("psmdb.percona.com/v1", "PerconaServerMongoDB", "mongo-mvjrswvl")},
        "spec": {"replicas": 1, "serviceName": "mongo-mvjrswvl-rs0",
            "selector": {"matchLabels": {"app": "mongo-mvjrswvl-rs0"}},
            "template": {"metadata": {"labels": {"app": "mongo-mvjrswvl-rs0"}}, "spec": {"containers": [{
                "name": "mongod", "image": "percona/percona-server-mongodb:7.0.43-23",
                "volumeMounts": [{"name": "mongod-data", "mountPath": "/data/db"}]
            }]}},
            "volumeClaimTemplates": [{"metadata": {"name": "mongod-data"},
                "spec": {"accessModes": ["ReadWriteOnce"], "resources": {"requests": {"storage": "10Gi"}}}}]},
        "status": {"replicas": 1, "readyReplicas": 1}
    }));
    let pending = json!({"phase": "Pending", "conditions": [{"type": "PodScheduled",
        "status": "False", "reason": "Unschedulable", "lastTransitionTime": minutes_ago(118),
        "message": "0/1 nodes are available: persistentvolumeclaim \"editor-e2e-data\" not found. not found"}]});
    let claim = |name: &str, size: &str, owner: Value| {
        from::<PersistentVolumeClaim>(json!({"metadata": {"name": name, "namespace": ns,
                "ownerReferences": owner},
            "spec": {"accessModes": ["ReadWriteOnce"], "resources": {"requests": {"storage": size}}},
            "status": {"phase": "Bound", "capacity": {"storage": size}}}))
    };
    let member = |name: &str, labels: Value, owner: Value, container: &str| -> Pod {
        from(json!({
            "metadata": {"name": name, "namespace": ns, "labels": labels,
                "ownerReferences": owner, "creationTimestamp": minutes_ago(600)},
            "spec": {"containers": [{"name": container, "image": "example"}]},
            "status": running(container, 0)
        }))
    };
    NamespaceObjects {
        namespace: ns.to_owned(),
        deployments: vec![
            deployment_in(
                ns,
                "talker",
                "ghcr.io/example/talker:1.2.0",
                1,
                1,
                &json!([{"name": "DATABASE_URL", "valueFrom": secret("pg-mvjrswvl-app", "uri")}]),
            ),
            deployment_in(
                ns,
                "crasher",
                "ghcr.io/example/crasher:0.1.0",
                1,
                0,
                &json!([]),
            ),
            editor,
            deployment_in(ns, "hello", "nginx:1.29", 1, 1, &json!([])),
            deployment_in(
                ns,
                "storefront",
                "ghcr.io/example/storefront:1.0.0",
                1,
                1,
                &json!([
                    {"name": "MYSQLPASSWORD", "valueFrom": secret("mysql-mvjrswvl-app", "password")},
                    {"name": "DATABASE_URL",
                        "value": "mysql://app:$(MYSQLPASSWORD)@moco-mysql-mvjrswvl-primary.lab.svc:3306/app"}
                ]),
            ),
        ],
        stateful_sets: vec![mongo],
        pods: vec![
            pod_in(
                ns,
                "talker-7d9c6b5f4-k8m2p",
                "talker",
                None,
                &running("talker", 0),
            ),
            pod_in(
                ns,
                "crasher-6949b848bd-mv6fd",
                "crasher",
                None,
                &crash_looping("crasher", 541),
            ),
            from(json!({
                "metadata": {"name": "editor-5c8d9f7b6-q2w3e", "namespace": ns,
                    "labels": {"app": "editor"}, "creationTimestamp": minutes_ago(118)},
                "spec": {"containers": [{"name": "editor", "image": "ghcr.io/example/editor:0.9.0"}]},
                "status": pending
            })),
            pod_in(
                ns,
                "hello-5f7b8c9d6-x4z7c",
                "hello",
                None,
                &running("hello", 0),
            ),
            pod_in(
                ns,
                "mongo-mvjrswvl-rs0-0",
                "mongo-mvjrswvl-rs0",
                Some(("StatefulSet", "mongo-mvjrswvl-rs0")),
                &running("mongod", 0),
            ),
            pod_in(
                ns,
                "storefront-6d8f7c9b5-r4t6y",
                "storefront",
                None,
                &running("storefront", 0),
            ),
            member(
                "moco-mysql-mvjrswvl-0",
                json!({"app.kubernetes.io/name": "mysql", "app.kubernetes.io/instance": "mysql-mvjrswvl"}),
                operated("moco.cybozu.com/v1beta2", "MySQLCluster", "mysql-mvjrswvl"),
                "mysqld",
            ),
            member(
                "moco-mysql-mvjrswvl-restored-1022-0",
                json!({"app.kubernetes.io/name": "mysql",
                    "app.kubernetes.io/instance": "mysql-mvjrswvl-restored-1022"}),
                operated(
                    "moco.cybozu.com/v1beta2",
                    "MySQLCluster",
                    "mysql-mvjrswvl-restored-1022",
                ),
                "mysqld",
            ),
            member(
                "mariadb-mvjrswvl-0",
                json!({"app.kubernetes.io/name": "mariadb", "app.kubernetes.io/instance": "mariadb-mvjrswvl"}),
                operated("k8s.mariadb.com/v1alpha1", "MariaDB", "mariadb-mvjrswvl"),
                "mariadb",
            ),
            member(
                "dragonfly-mvjrswvl-0",
                json!({"app.kubernetes.io/name": "dragonfly",
                    "app.kubernetes.io/instance": "dragonfly-mvjrswvl", "role": "master"}),
                operated("dragonflydb.io/v1alpha1", "Dragonfly", "dragonfly-mvjrswvl"),
                "dragonfly",
            ),
            member(
                "files-mvjrswvl-s3-7d9f6c5b8-x2k4p",
                json!({"app.kubernetes.io/name": "seaweedfs",
                    "app.kubernetes.io/instance": "files-mvjrswvl", "app.kubernetes.io/component": "s3"}),
                operated("seaweed.seaweedfs.com/v1", "Seaweed", "files-mvjrswvl"),
                "s3",
            ),
            from(json!({
                "metadata": {"name": "pg-mvjrswvl-1", "namespace": ns,
                    "labels": {"cnpg.io/cluster": "pg-mvjrswvl", "cnpg.io/instanceRole": "primary"},
                    "ownerReferences": operated("postgresql.cnpg.io/v1", "Cluster", "pg-mvjrswvl"),
                    "creationTimestamp": created("pg-mvjrswvl")},
                "spec": {"containers": [{"name": "postgres",
                    "image": "ghcr.io/cloudnative-pg/postgresql:17-standard-trixie"}]},
                "status": running("postgres", 0)
            })),
        ],
        services: vec![
            service_in(ns, "talker", 80, "http"),
            service_in(ns, "hello", 80, "http"),
        ],
        secrets: [
            "pg-mvjrswvl-app",
            "mongo-mvjrswvl-custom-user-secret",
            "moco-mysql-mvjrswvl",
            "moco-mysql-mvjrswvl-restored-1022",
            "mysql-mvjrswvl-app",
            "mariadb-mvjrswvl-app",
            "dragonfly-mvjrswvl-auth",
            "files-mvjrswvl-app",
        ]
        .into_iter()
        .map(|name| SecretSummary {
            secret: Secret {
                metadata: from(json!({"name": name, "namespace": ns})),
                type_: Some("Opaque".to_owned()),
                ..Secret::default()
            },
            key_count: 3,
        })
        .collect(),
        persistent_volume_claims: vec![
            claim("mongod-data-mongo-mvjrswvl-rs0-0", "10Gi", json!([])),
            claim(
                "pg-mvjrswvl-1",
                "5Gi",
                operated("postgresql.cnpg.io/v1", "Cluster", "pg-mvjrswvl"),
            ),
        ],
        ingresses: vec![ingress(ns, "hello.example.com", "hello")],
        ..NamespaceObjects::default()
    }
}

/// Billing: an API, a database and a nightly CronJob whose last run failed.
fn billing() -> NamespaceObjects {
    let ns = "billing";
    let run = "invoice-run-29342880";
    let cron_job: CronJob = from(json!({
        "metadata": {"name": "invoice-run", "namespace": ns,
            "uid": "00000000-0000-0000-0000-000000000000",
            "labels": {"zeabur.com/task-of": "api"}},
        "spec": {"schedule": "0 2 * * *", "jobTemplate": {"spec": {"template": {"spec": {
            "restartPolicy": "Never",
            "containers": [{"name": "invoice-run", "image": "ghcr.io/example/billing:3.2.0",
                "args": ["invoices", "--run"]}]}}}}},
        "status": {"lastScheduleTime": "2026-10-14T02:00:00Z"}
    }));
    let job: Job = from(json!({
        "metadata": {"name": run, "namespace": ns, "creationTimestamp": "2026-10-14T02:00:00Z",
            "ownerReferences": [{"apiVersion": "batch/v1",
            "kind": "CronJob", "name": "invoice-run",
            "uid": "00000000-0000-0000-0000-000000000000", "controller": true}]},
        "spec": {"backoffLimit": 0, "template": {"spec": {"restartPolicy": "Never",
            "containers": [{"name": "invoice-run", "image": "ghcr.io/example/billing:3.2.0"}]}}},
        "status": {"failed": 1, "startTime": "2026-10-14T02:00:00Z",
            "conditions": [{"type": "Failed", "status": "True",
            "reason": "BackoffLimitExceeded", "message": "Job has reached the specified backoff limit"}]}
    }));
    // A run started by hand the day before, which succeeded.
    let manual: Job = from(json!({
        "metadata": {"name": "invoice-run-manual-1013091500", "namespace": ns,
            "creationTimestamp": "2026-10-13T09:15:00Z",
            "annotations": {"cronjob.kubernetes.io/instantiate": "manual"},
            "ownerReferences": [{"apiVersion": "batch/v1",
            "kind": "CronJob", "name": "invoice-run",
            "uid": "00000000-0000-0000-0000-000000000000", "controller": true}]},
        "spec": {"backoffLimit": 0, "template": {"spec": {"restartPolicy": "Never",
            "containers": [{"name": "invoice-run", "image": "ghcr.io/example/billing:3.2.0"}]}}},
        "status": {"succeeded": 1, "startTime": "2026-10-13T09:15:00Z",
            "completionTime": "2026-10-13T09:16:12Z",
            "conditions": [{"type": "Complete", "status": "True"}]}
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
        jobs: vec![job, manual],
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

/// A large, healthy project: 42 small services, two stores and a dashboard behind a domain.
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
        "profiler",
        "segmenter",
        "attribution",
        "experiments",
        "flags",
        "billing-sync",
        "warehouse-sync",
        "backfill",
        "reindexer",
        "snapshotter",
        "audit",
        "webhooks",
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
            .chain([pod_in(
                ns,
                "cadvisor-k7m2p",
                "cadvisor",
                Some(("DaemonSet", "cadvisor")),
                &running("cadvisor", 1),
            )])
            .collect(),
        services: tools
            .iter()
            .map(|(name, _)| service_in(ns, name, 80, "http"))
            .collect(),
        daemon_sets: vec![cadvisor(ns)],
        ..NamespaceObjects::default()
    }
}

/// A chart's release: two Deployments Helm manages. The gateway's update is stuck past its
/// deadline while the replica of the release before it still serves; the new one waits for its
/// volume.
fn edge() -> NamespaceObjects {
    let ns = "edge";
    let helm = |mut deployment: Deployment| {
        let labels = deployment
            .metadata
            .labels
            .get_or_insert_with(Default::default);
        labels.insert("app.kubernetes.io/managed-by".to_owned(), "Helm".to_owned());
        labels.insert("helm.sh/chart".to_owned(), "gateway-1.4.0".to_owned());
        deployment
    };
    let mut gateway = helm(deployment_in(
        ns,
        "demo-gateway",
        "nginx:1.27-alpine",
        1,
        1,
        &json!([]),
    ));
    gateway.metadata.generation = Some(2);
    gateway.status = Some(from(json!({"observedGeneration": 2, "replicas": 2,
        "readyReplicas": 1, "updatedReplicas": 1, "availableReplicas": 1,
        "unavailableReplicas": 1,
        "conditions": [{"type": "Progressing", "status": "False",
            "reason": "ProgressDeadlineExceeded",
            "message": "ReplicaSet \"demo-gateway-7c9f8\" has timed out progressing."}]})));
    let pending = json!({"phase": "Pending", "containerStatuses": [{"name": "demo-gateway",
        "ready": false, "restartCount": 0, "image": "nginx:1.27-alpine", "imageID": "",
        "state": {"waiting": {"reason": "ContainerCreating"}}}]});
    NamespaceObjects {
        namespace: ns.to_owned(),
        deployments: vec![
            gateway,
            helm(deployment_in(
                ns,
                "demo-worker",
                "ghcr.io/example/gateway-worker:1.4.0",
                1,
                1,
                &json!([]),
            )),
        ],
        pods: vec![
            pod_in(
                ns,
                "demo-gateway-5b6d4-q8w2e",
                "demo-gateway",
                None,
                &running("demo-gateway", 0),
            ),
            pod_in(
                ns,
                "demo-gateway-7c9f8-z3x5c",
                "demo-gateway",
                None,
                &pending,
            ),
            pod_in(
                ns,
                "demo-worker-6a1b2-m4n6b",
                "demo-worker",
                None,
                &running("demo-worker", 0),
            ),
        ],
        services: vec![service_in(ns, "demo-gateway", 80, "http")],
        ..NamespaceObjects::default()
    }
}

/// cAdvisor as a DaemonSet with one ready pod, the shape most node agents have.
fn cadvisor(namespace: &str) -> DaemonSet {
    from(json!({
        "metadata": {"name": "cadvisor", "namespace": namespace, "generation": 1,
            "creationTimestamp": created("cadvisor"),
            "labels": {"app.kubernetes.io/name": "cadvisor"}},
        "spec": {"selector": {"matchLabels": {"app": "cadvisor"}},
            "template": {"metadata": {"labels": {"app": "cadvisor"}}, "spec": {"containers": [{
                "name": "cadvisor", "image": "ghcr.io/google/cadvisor:v0.60.5",
                "ports": [{"name": "http", "containerPort": 8080}]
            }]}}},
        "status": {"observedGeneration": 1, "desiredNumberScheduled": 1,
            "currentNumberScheduled": 1, "numberReady": 1, "updatedNumberScheduled": 1,
            "numberAvailable": 1, "numberMisscheduled": 0}
    }))
}

fn state() -> State {
    let namespace = |name: &str, adopted: bool| -> Namespace {
        let labels = if adopted {
            json!({"zeabur.com/project": "true"})
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
    if std::env::var_os("ZEABUR_DEMO_KEEL").is_some() {
        facts.workload_labels.push((
            "Deployment".to_owned(),
            BTreeMap::from([
                ("app".to_owned(), "keel".to_owned()),
                ("helm.sh/chart".to_owned(), "keel-1.0.5".to_owned()),
            ]),
        ));
    }
    let mut objects = BTreeMap::new();
    for mut project in [
        shop(),
        blog(),
        lab(),
        billing(),
        payments(),
        analytics(),
        internal_tools(),
        edge(),
        kube_system(),
        cert_manager(),
    ] {
        place(&mut project.pods);
        objects.insert(project.namespace.clone(), project);
    }
    let pods = demo_pods(&objects);
    let live = pod_metrics(&pods);
    PODS.get_or_init(|| pods);
    let answer: zeabur_kube::memory::ProxyAnswer = store;
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
            username: "zeabur:alice".to_owned(),
            groups: vec!["zeabur:admins".to_owned()],
        },
        namespaces: vec![
            namespace("shop", true),
            namespace("blog", true),
            namespace("lab", true),
            namespace("billing", true),
            namespace("payments", true),
            namespace("analytics", true),
            namespace("internal-tools", true),
            namespace("edge", true),
            namespace("default", false),
            namespace("kube-system", false),
        ],
        objects,
        events: BTreeMap::from([
            ("shop".to_owned(), events),
            ("blog".to_owned(), blog_events),
            (
                "lab".to_owned(),
                vec![
                    event_in(
                        "lab",
                        "l1",
                        33,
                        "Pod",
                        "crasher-6949b848bd-mv6fd",
                        "Warning",
                        "BackOff",
                        "Back-off restarting failed container crasher in pod crasher-6949b848bd-mv6fd_lab",
                        541,
                    ),
                    event_in(
                        "lab",
                        "l2",
                        30,
                        "Pod",
                        "editor-5c8d9f7b6-q2w3e",
                        "Warning",
                        "FailedScheduling",
                        "0/1 nodes are available: persistentvolumeclaim \"editor-e2e-data\" not found. not found",
                        119,
                    ),
                ],
            ),
            ("billing".to_owned(), billing_events),
            ("kube-system".to_owned(), system_events),
            (
                "edge".to_owned(),
                vec![event_in(
                    "edge",
                    "e1",
                    20,
                    "Pod",
                    "demo-gateway-7c9f8-z3x5c",
                    "Warning",
                    "FailedAttachVolume",
                    "Multi-Attach error for volume \"pvc-3f1c\": Volume is already used by pod demo-gateway-5b6d4-q8w2e",
                    6,
                )],
            ),
        ]),
        nodes: nodes(),
        // Lab's releases, so its warnings and restarts reach Activity.
        replica_sets: BTreeMap::from([(
            "lab".to_owned(),
            [
                ("crasher", "6949b848bd", "ghcr.io/example/crasher:0.1.0"),
                ("editor", "5c8d9f7b6", "ghcr.io/example/editor:0.9.0"),
                ("talker", "7d9c6b5f4", "ghcr.io/example/talker:1.2.0"),
                ("hello", "5f7b8c9d6", "nginx:1.29"),
            ]
            .into_iter()
            .map(|(name, hash, image)| {
                from(json!({
                    "metadata": {"name": format!("{name}-{hash}"), "namespace": "lab",
                        "creationTimestamp": created(name),
                        "annotations": {"deployment.kubernetes.io/revision": "1"},
                        "ownerReferences": [{"apiVersion": "apps/v1", "kind": "Deployment",
                            "name": name, "uid": "00000000-0000-0000-0000-000000000002",
                            "controller": true}]},
                    "spec": {"selector": {"matchLabels": {"app": name}},
                        "template": {"metadata": {"labels": {"app": name}},
                            "spec": {"containers": [{"name": name, "image": image}]}}}
                }))
            })
            .collect(),
        )]),
        // metrics-server and system-upgrade-controller, read through their modules; and
        // NetworkPolicy, so a ready-made policy shows among the objects and can be deleted; k3s's
        // HelmChart, so a component installs from the catalogue.
        served: vec![
            ServedResource {
                verbs: ["create", "delete", "get", "list"]
                    .map(str::to_owned)
                    .to_vec(),
                ..ServedResource::readable("helm.cattle.io", "v1", "helmcharts", "HelmChart", true)
            },
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
            ServedResource {
                verbs: ["create", "delete", "get", "list", "patch"]
                    .map(str::to_owned)
                    .to_vec(),
                ..ServedResource::readable("traefik.io", "v1alpha1", "middlewares", "Middleware", true)
            },
            ServedResource::readable("cert-manager.io", "v1", "certificates", "Certificate", true),
            ServedResource::readable(
                "acme.cert-manager.io",
                "v1",
                "challenges",
                "Challenge",
                true,
            ),
            ServedResource::readable("postgresql.cnpg.io", "v1", "clusters", "Cluster", true),
            ServedResource::readable(
                "moco.cybozu.com",
                "v1beta2",
                "mysqlclusters",
                "MySQLCluster",
                true,
            ),
            ServedResource::readable(
                "moco.cybozu.com",
                "v1beta2",
                "backuppolicies",
                "BackupPolicy",
                true,
            ),
            ServedResource::readable("k8s.mariadb.com", "v1alpha1", "mariadbs", "MariaDB", true),
            ServedResource::readable("k8s.mariadb.com", "v1alpha1", "backups", "Backup", true),
            ServedResource::readable(
                "dragonflydb.io",
                "v1alpha1",
                "dragonflies",
                "Dragonfly",
                true,
            ),
            ServedResource::readable(
                "seaweed.seaweedfs.com",
                "v1",
                "seaweeds",
                "Seaweed",
                true,
            ),
            ServedResource::readable(
                "seaweed.seaweedfs.com",
                "v1",
                "buckets",
                "Bucket",
                true,
            ),
            ServedResource::readable(
                "seaweed.seaweedfs.com",
                "v1",
                "s3identities",
                "S3Identity",
                true,
            ),
            ServedResource::readable(
                "seaweed.seaweedfs.com",
                "v1",
                "s3credentials",
                "S3Credentials",
                true,
            ),
            ServedResource::readable(
                "seaweed.seaweedfs.com",
                "v1",
                "s3policies",
                "S3Policy",
                true,
            ),
            ServedResource::readable(
                "seaweed.seaweedfs.com",
                "v1",
                "s3policybindings",
                "S3PolicyBinding",
                true,
            ),
            ServedResource::readable("batch", "v1", "jobs", "Job", true),
            ServedResource::readable(
                "postgresql.cnpg.io",
                "v1",
                "scheduledbackups",
                "ScheduledBackup",
                true,
            ),
            ServedResource::readable("postgresql.cnpg.io", "v1", "backups", "Backup", true),
            ServedResource::readable(
                "barmancloud.cnpg.io",
                "v1",
                "objectstores",
                "ObjectStore",
                true,
            ),
            ServedResource::readable(
                "psmdb.percona.com",
                "v1",
                "perconaservermongodbs",
                "PerconaServerMongoDB",
                true,
            ),
            ServedResource::readable(
                "psmdb.percona.com",
                "v1",
                "perconaservermongodbbackups",
                "PerconaServerMongoDBBackup",
                true,
            ),
            ServedResource::readable("builder.zeabur.com", "v1alpha1", "builds", "Build", true),
            ServedResource::readable(
                "builder.zeabur.com",
                "v1alpha1",
                "buildruns",
                "BuildRun",
                true,
            ),
            ServedResource::readable("identity.zeabur.com", "v1alpha1", "users", "User", true),
            ServedResource::readable(
                "identity.zeabur.com",
                "v1alpha1",
                "credentials",
                "Credential",
                true,
            ),
            ServedResource::readable(
                "identity.zeabur.com",
                "v1alpha1",
                "invitations",
                "Invitation",
                true,
            ),
            ServedResource::readable("identity.zeabur.com", "v1alpha1", "grants", "Grant", true),
            ServedResource::readable(
                "rbac.authorization.k8s.io",
                "v1",
                "rolebindings",
                "RoleBinding",
                true,
            ),
            ServedResource::readable(
                "rbac.authorization.k8s.io",
                "v1",
                "clusterrolebindings",
                "ClusterRoleBinding",
                false,
            ),
            ServedResource::readable(
                "gateway.networking.k8s.io",
                "v1",
                "gateways",
                "Gateway",
                true,
            ),
        ]
        .into_iter()
        .chain(velero_served())
        .chain(k8up_served())
        .chain(alerting_served())
        .chain([ServedResource {
            verbs: ["create", "get", "list", "patch"].map(str::to_owned).to_vec(),
            ..ServedResource::readable(
                "cert-manager.io",
                "v1",
                "clusterissuers",
                "ClusterIssuer",
                false,
            )
        }])
        .collect(),
        dynamic: [
            // The Gateway of the shop's HTTPRoute, which accepts routes from every namespace.
            from(json!({
                "apiVersion": "gateway.networking.k8s.io/v1", "kind": "Gateway",
                "metadata": {"name": "public", "namespace": "gateway", "resourceVersion": "1"},
                "spec": {"gatewayClassName": "traefik", "listeners": [
                    {"name": "web", "port": 8000, "protocol": "HTTP",
                     "allowedRoutes": {"namespaces": {"from": "All"}}},
                    {"name": "websecure", "port": 8443, "protocol": "HTTPS",
                     "hostname": "*.shop.example.com",
                     "allowedRoutes": {"namespaces": {"from": "All"}}}
                ]}
            })),
            from(json!({
                "apiVersion": "traefik.io/v1alpha1", "kind": "Middleware",
                "metadata": {"name": "web-error-pages", "namespace": "shop",
                    "resourceVersion": "4",
                    "ownerReferences": [{"apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
                        "name": "web", "uid": "0e9b1c44-6a43-4c55-9d0b-7f3f2a1d0001"}]},
                "spec": {"errors": {"status": ["502-504"],
                    "service": {"name": "error-pages", "port": 8080}, "query": "/{status}.html"}}
            })),
            from(json!({
                "apiVersion": "traefik.io/v1alpha1", "kind": "Middleware",
                "metadata": {"name": "web-redirect-www-shop-example-com", "namespace": "shop",
                    "resourceVersion": "3",
                    "ownerReferences": [{"apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
                        "name": "web", "uid": "0e9b1c44-6a43-4c55-9d0b-7f3f2a1d0001"}]},
                "spec": {"redirectRegex": {
                    "regex": "^https?://www\\.shop\\.example\\.com(:[0-9]+)?/(.*)",
                    "replacement": "https://shop.example.com/${2}", "permanent": true}}
            })),
            certificate(
                "shop",
                "shop-tls",
                "shop.example.com",
                &json!([{"type": "Ready", "status": "True", "reason": "Ready"}]),
            ),
            certificate(
                "shop",
                "api-tls",
                "api.shop.example.com",
                &json!([{"type": "Ready", "status": "False", "reason": "DoesNotExist"},
                    {"type": "Issuing", "status": "True", "reason": "DoesNotExist"}]),
            ),
            from(json!({
                "apiVersion": "acme.cert-manager.io/v1", "kind": "Challenge",
                "metadata": {"name": "api-tls-1-3141592", "namespace": "shop"},
                "spec": {"dnsName": "api.shop.example.com", "type": "HTTP-01"},
                "status": {"state": "pending", "presented": true,
                    "reason": "Waiting for HTTP-01 challenge propagation: failed to perform self check GET request 'http://api.shop.example.com/.well-known/acme-challenge/x': dial tcp: lookup api.shop.example.com: no such host"}
            })),
            certificate(
                "billing",
                "api-tls",
                "billing.example.com",
                &json!([{"type": "Ready", "status": "True", "reason": "Ready"}]),
            ),
            certificate(
                "analytics",
                "dashboard-tls",
                "stats.example.com",
                &json!([{"type": "Ready", "status": "False", "reason": "DoesNotExist"},
                    {"type": "Issuing", "status": "False", "reason": "Failed",
                     "message": "The certificate request has failed to complete and will be retried: Failed to wait for order resource \"dashboard-tls-1-2718281\" to become ready: order is in \"errored\" state: Failed to create Order: 429 urn:ietf:params:acme:error:rateLimited: too many certificates already issued for this exact set of identifiers in the last 168h0m0s"}]),
            ),
            usage("vps-1", "1420m", "1730Mi"),
            usage("vps-2", "860m", "3900Mi"),
            usage("vps-3", "1240m", "4600Mi"),
            from(json!({
                "apiVersion": "apps/v1", "kind": "Deployment",
                "metadata": {"name": "reloader", "namespace": "reloader",
                    "labels": {"app.kubernetes.io/name": "reloader"}},
                "status": {"replicas": 1, "readyReplicas": 1, "availableReplicas": 1}
            })),
            // Shop's databases: a PostgreSQL the API reads, and an older MongoDB without an app
            // user.
            from(json!({
                "apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
                "metadata": {"name": "orders", "namespace": "shop",
                    "creationTimestamp": created("orders")},
                "spec": {"instances": 1, "imageName": "ghcr.io/cloudnative-pg/postgresql:17-standard-trixie",
                    "storage": {"size": "5Gi"}},
                "status": {"phase": "Cluster in healthy state", "readyInstances": 1,
                    "currentPrimary": "orders-1", "writeService": "orders-rw"}
            })),
            from(json!({
                "apiVersion": "psmdb.percona.com/v1", "kind": "PerconaServerMongoDB",
                "metadata": {"name": "sessions", "namespace": "shop",
                    "creationTimestamp": created("sessions")},
                "spec": {"image": "percona/percona-server-mongodb:8.0.12-4",
                    "replsets": [{"name": "rs0", "size": 3}]},
                "status": {"state": "ready", "mongoVersion": "8.0.12-4",
                    "replsets": {"rs0": {"ready": 3, "size": 3, "status": "ready"}}}
            })),
            from(json!({
                "apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
                "metadata": {"name": "pg-mvjrswvl", "namespace": "lab",
                    "creationTimestamp": created("pg-mvjrswvl")},
                "spec": {"instances": 1, "imageName": "ghcr.io/cloudnative-pg/postgresql:17-standard-trixie",
                    "storage": {"size": "5Gi"}},
                "status": {"phase": "Cluster in healthy state", "readyInstances": 1,
                    "instanceNames": ["pg-mvjrswvl-1"],
                    "currentPrimary": "pg-mvjrswvl-1", "writeService": "pg-mvjrswvl-rw"}
            })),
            from(json!({
                "apiVersion": "psmdb.percona.com/v1", "kind": "PerconaServerMongoDB",
                "metadata": {"name": "mongo-mvjrswvl", "namespace": "lab",
                    "creationTimestamp": created("mongo-mvjrswvl")},
                "spec": {"image": "percona/percona-server-mongodb:7.0.43-23",
                    "replsets": [{"name": "rs0", "size": 1}],
                    "users": [{"name": "app", "db": "app", "roles": [{"name": "readWrite", "db": "app"}]}]},
                "status": {"state": "ready", "mongoVersion": "7.0.43-23",
                    "replsets": {"rs0": {"ready": 1, "size": 1, "status": "ready"}}}
            })),
            // Lab's MySQL, backed up nightly and restored into a copy; a MariaDB and a Dragonfly.
            from(json!({
                "apiVersion": "moco.cybozu.com/v1beta2", "kind": "MySQLCluster",
                "metadata": {"name": "mysql-mvjrswvl", "namespace": "lab",
                    "creationTimestamp": created("mysql-mvjrswvl"),
                    "annotations": {"zeabur.com/app-database": "app",
                        "zeabur.com/app-secret": "mysql-mvjrswvl-app"}},
                "spec": {"replicas": 1, "backupPolicyName": "mysql-mvjrswvl-backups",
                    "podTemplate": {"spec": {"containers": [{"name": "mysqld",
                        "image": "ghcr.io/cybozu-go/moco/mysql:8.4.10",
                        "resources": {"requests": {"cpu": "100m", "memory": "512Mi"}, "limits": {"memory": "1Gi"}}}]}},
                    "volumeClaimTemplates": [{"metadata": {"name": "mysql-data"},
                        "spec": {"accessModes": ["ReadWriteOnce"], "storageClassName": "local-path",
                            "resources": {"requests": {"storage": "10Gi"}}}}]},
                "status": {"conditions": [{"type": "Initialized", "status": "True"},
                    {"type": "Available", "status": "True"}, {"type": "Healthy", "status": "True"}],
                    "currentPrimaryIndex": 0, "syncedReplicas": 1,
                    "backup": {"time": "2026-10-22T03:00:00Z", "elapsed": "41s", "sourceIndex": 0,
                        "sourceUUID": "x", "uuidSet": {}, "binlogFilename": "binlog.000003",
                        "gtidSet": "", "dumpSize": 1, "binlogSize": 1, "workDirUsage": 1, "warnings": []}}
            })),
            from(json!({
                "apiVersion": "moco.cybozu.com/v1beta2", "kind": "BackupPolicy",
                "metadata": {"name": "mysql-mvjrswvl-backups", "namespace": "lab"},
                "spec": {"schedule": "0 3 * * *", "jobConfig": {"serviceAccountName": "default",
                    "bucketConfig": {"bucketName": "lab-backups", "endpointURL": "https://s3.example.com", "usePathStyle": true},
                    "envFrom": [{"secretRef": {"name": "mysql-mvjrswvl-backups-s3"}}],
                    "workVolume": {"emptyDir": {}}}}
            })),
            from(json!({
                "apiVersion": "batch/v1", "kind": "Job",
                "metadata": {"name": "mysql-mvjrswvl-app-setup", "namespace": "lab"},
                "status": {"conditions": [{"type": "Complete", "status": "True"}],
                    "completionTime": "2026-09-21T10:03:00Z"}
            })),
            from(json!({
                "apiVersion": "batch/v1", "kind": "Job",
                "metadata": {"name": "moco-backup-mysql-mvjrswvl-29350260", "namespace": "lab",
                    "ownerReferences": [{"apiVersion": "batch/v1", "kind": "CronJob",
                        "name": "moco-backup-mysql-mvjrswvl", "uid": "00000000-0000-0000-0000-000000000002",
                        "controller": true}]},
                "status": {"startTime": "2026-10-22T03:00:00Z", "completionTime": "2026-10-22T03:00:41Z"}
            })),
            from(json!({
                "apiVersion": "moco.cybozu.com/v1beta2", "kind": "MySQLCluster",
                "metadata": {"name": "mysql-mvjrswvl-restored-1022", "namespace": "lab",
                    "creationTimestamp": created("mysql-mvjrswvl-restored-1022"),
                    "annotations": {"zeabur.com/app-database": "app",
                        "zeabur.com/app-secret": "mysql-mvjrswvl-app",
                        "zeabur.com/restored-from": "mysql-mvjrswvl",
                        "zeabur.com/restored-backup": "moco-backup-mysql-mvjrswvl-29350260",
                        "zeabur.com/restored-time": "2026-10-22T03:00:41Z"}},
                "spec": {"replicas": 1,
                    "podTemplate": {"spec": {"containers": [{"name": "mysqld",
                        "image": "ghcr.io/cybozu-go/moco/mysql:8.4.10",
                        "resources": {"requests": {"cpu": "100m", "memory": "512Mi"}, "limits": {"memory": "1Gi"}}}]}},
                    "volumeClaimTemplates": [{"metadata": {"name": "mysql-data"},
                        "spec": {"accessModes": ["ReadWriteOnce"], "storageClassName": "local-path",
                            "resources": {"requests": {"storage": "10Gi"}}}}]},
                "status": {"conditions": [{"type": "Initialized", "status": "True"},
                    {"type": "Available", "status": "True"}, {"type": "Healthy", "status": "True"}],
                    "currentPrimaryIndex": 0, "syncedReplicas": 1}
            })),
            from(json!({
                "apiVersion": "k8s.mariadb.com/v1alpha1", "kind": "MariaDB",
                "metadata": {"name": "mariadb-mvjrswvl", "namespace": "lab",
                    "creationTimestamp": created("mariadb-mvjrswvl")},
                "spec": {"image": "mariadb:11.8.3", "replicas": 1, "database": "app", "username": "app",
                    "passwordSecretKeyRef": {"name": "mariadb-mvjrswvl-app", "key": "password"},
                    "rootPasswordSecretKeyRef": {"name": "mariadb-mvjrswvl-root", "key": "password"},
                    "storage": {"size": "10Gi", "storageClassName": "local-path"},
                    "resources": {"requests": {"cpu": "100m", "memory": "256Mi"}, "limits": {"memory": "512Mi"}}},
                "status": {"conditions": [{"type": "Ready", "status": "True", "reason": "StatefulSetReady"}],
                    "currentPrimary": "mariadb-mvjrswvl-0"}
            })),
            from(json!({
                "apiVersion": "dragonflydb.io/v1alpha1", "kind": "Dragonfly",
                "metadata": {"name": "dragonfly-mvjrswvl", "namespace": "lab",
                    "creationTimestamp": created("dragonfly-mvjrswvl")},
                "spec": {"image": "docker.dragonflydb.io/dragonflydb/dragonfly:v1.34.1", "replicas": 1,
                    "authentication": {"passwordFromSecret": {"name": "dragonfly-mvjrswvl-auth", "key": "password"}},
                    "resources": {"requests": {"cpu": "100m", "memory": "256Mi"}, "limits": {"memory": "512Mi"}},
                    "snapshot": {"cron": "0 * * * *", "persistentVolumeClaimSpec": {
                        "accessModes": ["ReadWriteOnce"], "storageClassName": "local-path",
                        "resources": {"requests": {"storage": "2Gi"}}}}},
                "status": {"phase": "Ready"}
            })),
            from(json!({
                "apiVersion": "seaweed.seaweedfs.com/v1", "kind": "Seaweed",
                "metadata": {"name": "files-mvjrswvl", "namespace": "lab",
                    "creationTimestamp": created("files-mvjrswvl")},
                "spec": {"image": "chrislusf/seaweedfs:4.48", "volumeServerDiskCount": 1,
                    "master": {"replicas": 1, "persistence": {"enabled": true}},
                    "volume": {"replicas": 1, "requests": {"storage": "10Gi"}},
                    "filer": {"replicas": 1, "persistence": {"enabled": true}},
                    "s3": {"replicas": 1}},
                "status": {"conditions": [{"type": "Ready", "status": "True", "reason": "Ready",
                        "message": "Seaweed cluster is ready"}],
                    "master": {"replicas": 1, "readyReplicas": 1},
                    "volume": {"replicas": 1, "readyReplicas": 1},
                    "filer": {"replicas": 1, "readyReplicas": 1},
                    "s3": {"replicas": 1, "readyReplicas": 1}}
            })),
            from(json!({
                "apiVersion": "seaweed.seaweedfs.com/v1", "kind": "S3Credentials",
                "metadata": {"name": "files-mvjrswvl-app", "namespace": "lab"},
                "spec": {"seaweedRef": {"name": "files-mvjrswvl"},
                    "identityRef": {"name": "files-mvjrswvl-app"}},
                "status": {"phase": "Ready", "accessKey": "ZK4Q8N2M7RXW", "identityName": "files-mvjrswvl-app",
                    "secretName": "files-mvjrswvl-app"}
            })),
            from(json!({
                "apiVersion": "seaweed.seaweedfs.com/v1", "kind": "S3Identity",
                "metadata": {"name": "files-mvjrswvl-app", "namespace": "lab"},
                "spec": {"seaweedRef": {"name": "files-mvjrswvl"}}
            })),
            from(json!({
                "apiVersion": "seaweed.seaweedfs.com/v1", "kind": "S3Policy",
                "metadata": {"name": "files-mvjrswvl-app", "namespace": "lab"},
                "spec": {"seaweedRef": {"name": "files-mvjrswvl"},
                    "statements": [{"effect": "Allow", "actions": ["s3:*"], "resources": ["*"]}]}
            })),
            from(json!({
                "apiVersion": "seaweed.seaweedfs.com/v1", "kind": "S3PolicyBinding",
                "metadata": {"name": "files-mvjrswvl-app", "namespace": "lab"},
                "spec": {"seaweedRef": {"name": "files-mvjrswvl"},
                    "policyRef": {"name": "files-mvjrswvl-app"},
                    "subjects": [{"kind": "S3Identity", "name": "files-mvjrswvl-app"}]}
            })),
            from(json!({
                "apiVersion": "seaweed.seaweedfs.com/v1", "kind": "Bucket",
                "metadata": {"name": "uploads", "namespace": "lab"},
                "spec": {"clusterRef": {"name": "files-mvjrswvl"}, "reclaimPolicy": "Delete"},
                "status": {"phase": "Ready", "bucketName": "uploads",
                    "usage": {"sizeBytes": 734_003_200, "objectCount": 1284}}
            })),
            from(json!({
                "apiVersion": "upgrade.cattle.io/v1", "kind": "Plan",
                "metadata": {"name": "server", "namespace": "system-upgrade"},
                "spec": {"channel": "https://update.k3s.io/v1-release/channels/v1.34"},
                "status": {"latestVersion": "v1.34.12+k3s1"}
            })),
        ]
        .into_iter()
        .chain(live)
        .chain(docs_build())
        .chain(people())
        .chain(member_roles())
        .chain(velero())
        .chain(k8up())
        .chain(alerting())
        .chain(cluster_issuers())
        .collect(),
        // The metrics store where Zeabur's recommendation installs it.
        proxied: BTreeMap::from([(
            zeabur_api::settings::default_metrics_store().to_string(),
            answer,
        )]),
        forbidden_resources: std::env::var("ZEABUR_DEMO_FORBID")
            .map_or_default(|list| list.split(',').map(str::to_owned).collect()),
        // The same kinds in every namespace too, but pods: a project without its pods says
        // little, and `pods` here is for the cluster page's requests.
        forbidden_in_namespaces: std::env::var("ZEABUR_DEMO_FORBID").map_or_default(|list| {
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
            ("billing/api-5b7d9-c3v8n".to_owned(), billing_log(0)),
            ("billing/api-5b7d9-m1x4q".to_owned(), billing_log(1)),
            (
                "billing/invoice-run-29342880-7xk2p".to_owned(),
                "2026-10-14T02:00:01Z \u{1b}[32mINFO\u{1b}[0m invoice run started for 2026-10-13\n\
                 2026-10-14T02:00:04Z \u{1b}[31mERROR\u{1b}[0m connect db:5432: password authentication failed for user \"billing\"\n\
                 2026-10-14T02:00:04Z exit status 1"
                    .to_owned(),
            ),
            (
                "shop/orders-1".to_owned(),
                "{\"level\":\"info\",\"msg\":\"database system is ready to accept connections\",\"logger\":\"postgres\"}".to_owned(),
            ),
            (
                "shop/sessions-rs0-0".to_owned(),
                "{\"t\":{\"$date\":\"2026-10-14T02:00:00Z\"},\"s\":\"I\",\"c\":\"REPL\",\"msg\":\"Transition to primary complete\"}".to_owned(),
            ),
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
        secret_values: BTreeMap::from([
            (
                "shop/db-credentials".to_owned(),
                BTreeMap::from([
                    ("username".to_owned(), b"shop".to_vec()),
                    (
                        "password".to_owned(),
                        b"correct-horse-battery-staple".to_vec(),
                    ),
                ]),
            ),
            (
                "shop/shared-keys".to_owned(),
                BTreeMap::from([
                    (
                        "SENTRY_DSN".to_owned(),
                        b"https://4f1c@o1.ingest.example.com/7".to_vec(),
                    ),
                    ("MAILER_TOKEN".to_owned(), b"mt_live_51b2".to_vec()),
                ]),
            ),
        ]),
        facts,
        ..State::default()
    }
}

/// What one of billing's API replicas wrote: requests, and a slow one now and then.
fn billing_log(replica: u32) -> String {
    (1..=30)
        .map(|i| {
            let ms = (i * 7 + replica * 3) % 40 + 6;
            if i % 11 == 0 {
                format!("2026-10-14T09:{i:02}:12Z \u{1b}[33mWARN\u{1b}[0m POST /invoices 200 {}ms slow query", ms * 30)
            } else {
                format!("2026-10-14T09:{i:02}:12Z \u{1b}[32mINFO\u{1b}[0m GET /invoices/{} 200 {ms}ms", 4100 + i)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The cluster's version: a k3s whose minor is in maintenance and has a newer patch.
const VERSION: &str = "v1.34.10+k3s1";

/// `minutes` before now, as an event time.
/// When a workload was created: a few weeks ago, a different day for each name.
fn alerting_installed() -> bool {
    std::env::var("ZEABUR_DEMO_ALERTING").as_deref() != Ok("none")
}

fn alerting_served() -> Vec<ServedResource> {
    if !alerting_installed() {
        return Vec::new();
    }
    [am::CONFIGS, am::RULES]
        .into_iter()
        .map(|r| ServedResource::readable(r.group, r.version, r.resource, r.kind, true))
        .collect()
}

/// Channels of shop and of the cluster, and the rule set they route.
fn alerting() -> Vec<DynamicObject> {
    use zeabur_modules::alertmanager::{AlertmanagerChannelSettings as S, AlertmanagerEvent as E};
    if !alerting_installed() {
        return Vec::new();
    }
    let channel = |namespace: &str, name: &str, settings: &S, events: &[E], status: Value| {
        let mut object: DynamicObject = from(am::config(namespace, name, settings, events));
        object.metadata.resource_version = Some("4".to_owned());
        if let Some(data) = object.data.as_object_mut() {
            data.insert("status".to_owned(), status);
        }
        object
    };
    let operational = json!({"updateStatus": "operational"});
    vec![
        channel(
            "shop",
            "team-alerts",
            &S::Slack {
                webhook_url: "https://hooks.slack.com/services/demo".to_owned(),
            },
            &[
                E::DeployFailed,
                E::BuildFailed,
                E::CrashLoop,
                E::OutOfMemory,
            ],
            operational.clone(),
        ),
        channel(
            "shop",
            "on-call",
            &S::Webhook {
                url: "http://pager.internal/hook".to_owned(),
                bearer_token: String::new(),
            },
            &[E::DatabaseUnhealthy, E::BackupFailed],
            json!({"updateStatus": "failed",
                   "reason": "unable to fetch secret=\"zeabur-notify-on-call\", ns=\"shop\": \
                              secrets \"zeabur-notify-on-call\" not found"}),
        ),
        channel(
            am::CLUSTER_NAMESPACE,
            "admins",
            &S::Telegram {
                bot_token: "demo".to_owned(),
                chat_id: -1_001_234_567,
            },
            &[E::NodeNotReady, E::RuntimeSecurity, E::BackupFailed],
            operational,
        ),
        from(json!({
            "apiVersion": am::RULES.resource().api_version(), "kind": am::RULES.kind,
            "metadata": {"name": am::RULES_NAME, "namespace": am::CLUSTER_NAMESPACE},
            "spec": {"groups": []}
        })),
    ]
}

/// K8up, unless `ZEABUR_DEMO_K8UP` is `none`.
fn k8up_installed() -> bool {
    std::env::var("ZEABUR_DEMO_K8UP").unwrap_or_default() != "none"
}

fn k8up_served() -> Vec<ServedResource> {
    if !k8up_installed() {
        return Vec::new();
    }
    [
        ("schedules", "Schedule"),
        ("backups", "Backup"),
        ("snapshots", "Snapshot"),
        ("restores", "Restore"),
    ]
    .into_iter()
    .map(|(resource, kind)| ServedResource::readable("k8up.io", "v1", resource, kind, true))
    .collect()
}

/// A nightly schedule of shop's Redis volume, kept 7 days and 4 weeks; last night's backup
/// failed, the one before succeeded. Its snapshots can be restored, and one was restored into a
/// new volume.
fn k8up() -> Vec<DynamicObject> {
    if !k8up_installed() {
        return Vec::new();
    }
    let backend = json!({
        "repoPasswordSecretRef": {"name": "data-redis-0-backup", "key": "RESTIC_PASSWORD"},
        "s3": {"endpoint": "https://4f1c.r2.cloudflarestorage.com", "bucket": "acme-backups/shop/data-redis-0",
            "accessKeyIDSecretRef": {"name": "data-redis-0-backup", "key": "AWS_ACCESS_KEY_ID"},
            "secretAccessKeySecretRef": {"name": "data-redis-0-backup", "key": "AWS_SECRET_ACCESS_KEY"}}
    });
    let selector = json!([{"matchLabels": {"zeabur.com/backup-volume": "data-redis-0"}}]);
    let repository = "s3:https://4f1c.r2.cloudflarestorage.com/acme-backups/shop/data-redis-0";
    let snapshot = |id: &str, minutes: i64| {
        from(json!({
            "apiVersion": "k8up.io/v1", "kind": "Snapshot",
            "metadata": {"name": id, "namespace": "shop", "resourceVersion": "31"},
            "spec": {"id": id, "date": minutes_ago(minutes), "repository": repository,
                "paths": ["/data/data-redis-0"]}
        }))
    };
    let backup = |name: &str, minutes: i64, reason: &str, message: &str| {
        from(json!({
            "apiVersion": "k8up.io/v1", "kind": "Backup",
            "metadata": {"name": name, "namespace": "shop", "resourceVersion": "30",
                "creationTimestamp": minutes_ago(minutes)},
            "spec": {"backend": backend, "labelSelectors": selector},
            "status": {"conditions": [{"type": "Completed", "status": "True", "reason": reason,
                "message": message, "lastTransitionTime": minutes_ago(minutes - 2)}]}
        }))
    };
    vec![
        from(json!({
            "apiVersion": "k8up.io/v1", "kind": "Schedule",
            "metadata": {"name": "data-redis-0-backup", "namespace": "shop", "resourceVersion": "21"},
            "spec": {
                "backend": backend,
                "backup": {"schedule": "0 3 * * *", "labelSelectors": selector,
                    "podSecurityContext": {"runAsUser": 0}},
                "prune": {"schedule": "0 4 * * *",
                    "retention": {"keepLast": 3, "keepDaily": 7, "keepWeekly": 4}}
            }
        })),
        backup(
            "data-redis-0-backup-nightly-2",
            300,
            "Failed",
            "unable to open repository: 403 Forbidden",
        ),
        backup(
            "data-redis-0-backup-nightly-1",
            1740,
            "Succeeded",
            "the job succeeded",
        ),
        snapshot("3f9a6c1e7b2d4a58", 1738),
        snapshot("8c2e5d0a9f1b3c47", 3178),
        from(json!({
            "apiVersion": "k8up.io/v1", "kind": "Restore",
            "metadata": {"name": "data-redis-0-restore-1", "namespace": "shop",
                "resourceVersion": "41", "creationTimestamp": minutes_ago(90),
                "labels": {"zeabur.com/backup-volume": "data-redis-0"}},
            "spec": {"backend": backend, "snapshot": "3f9a6c1e7b2d4a58",
                "restoreMethod": {"folder": {"claimName": "data-redis-0-copy"}}},
            "status": {"conditions": [{"type": "Completed", "status": "True",
                "reason": "Succeeded", "lastTransitionTime": minutes_ago(88)}]}
        })),
    ]
}

/// Velero, unless `ZEABUR_DEMO_VELERO` is `none`; `location` has a location and nothing else.
fn velero_mode() -> String {
    std::env::var("ZEABUR_DEMO_VELERO").unwrap_or_default()
}

fn velero_served() -> Vec<ServedResource> {
    if velero_mode() == "none" {
        return Vec::new();
    }
    [
        ("backupstoragelocations", "BackupStorageLocation"),
        ("schedules", "Schedule"),
        ("backups", "Backup"),
        ("podvolumebackups", "PodVolumeBackup"),
        ("restores", "Restore"),
        ("downloadrequests", "DownloadRequest"),
    ]
    .into_iter()
    .map(|(resource, kind)| ServedResource::readable("velero.io", "v1", resource, kind, true))
    .collect()
}

/// A daily schedule kept a week, its backups (the newest failed to reach the bucket, one before
/// it copied all but two volumes), what their file-system backups copied, and a restore.
fn velero() -> Vec<DynamicObject> {
    let mode = velero_mode();
    if mode == "none" {
        return Vec::new();
    }
    let location = from(json!({
        "apiVersion": "velero.io/v1", "kind": "BackupStorageLocation",
        "metadata": {"name": "default", "namespace": "velero", "resourceVersion": "12"},
        "spec": {"provider": "aws", "default": true, "credential":
                {"name": "zeabur-backup-credentials", "key": "cloud"},
            "objectStorage": {"bucket": "acme-backups", "prefix": "zeabur"},
            "config": {"region": "auto", "s3ForcePathStyle": "true",
                "s3Url": "https://4f1c.r2.cloudflarestorage.com"}},
        "status": {"phase": "Available", "lastValidationTime": minutes_ago(1)}
    }));
    if mode == "location" {
        return vec![location];
    }
    let schedule = from(json!({
        "apiVersion": "velero.io/v1", "kind": "Schedule",
        "metadata": {"name": "zeabur", "namespace": "velero", "resourceVersion": "40",
            "labels": {"app.kubernetes.io/managed-by": "zeabur"}},
        "spec": {"schedule": "0 3 * * *", "template": {"includedNamespaces": ["*"],
            "ttl": "168h0m0s", "defaultVolumesToFsBackup": true, "storageLocation": "default"}},
        "status": {"phase": "Enabled", "lastBackup": minutes_ago(60 * 7)}
    }));
    let day = 24 * 60;
    let backup = |name: &str, started: i64, phase: &str, extra: Value| {
        let mut status = json!({"phase": phase, "startTimestamp": minutes_ago(started),
            "expiration": minutes_ago(started - 7 * day)});
        if let (Some(status), Value::Object(extra)) = (status.as_object_mut(), extra) {
            if phase != "FailedValidation" {
                status.insert(
                    "completionTimestamp".to_owned(),
                    json!(minutes_ago(started - 6)),
                );
            }
            status.extend(extra);
        }
        from(json!({
            "apiVersion": "velero.io/v1", "kind": "Backup",
            "metadata": {"name": name, "namespace": "velero",
                "labels": {"velero.io/schedule-name": "zeabur"}},
            "spec": {"includedNamespaces": ["*"], "ttl": "168h0m0s",
                "defaultVolumesToFsBackup": true, "storageLocation": "default"},
            "status": status
        }))
    };
    let volume = |backup: &str, pod: &str, bytes: u64| {
        from(json!({
            "apiVersion": "velero.io/v1", "kind": "PodVolumeBackup",
            "metadata": {"name": format!("{backup}-{pod}"), "namespace": "velero",
                "labels": {"velero.io/backup-name": backup}},
            "status": {"phase": "Completed", "progress": {"bytesDone": bytes, "totalBytes": bytes}}
        }))
    };
    let mut objects = vec![
        location,
        schedule,
        backup(
            "zeabur-20261023030000",
            60 * 7,
            "Failed",
            json!({"failureReason": "backup storage location default is unavailable: rpc error: code = Unknown desc = AccessDenied: Access Denied"}),
        ),
        backup(
            "zeabur-20261022030000",
            60 * 7 + day,
            "PartiallyFailed",
            json!({"errors": 2, "warnings": 1,
                "progress": {"itemsBackedUp": 1186, "totalItems": 1188}}),
        ),
    ];
    for n in 2..7 {
        let name = format!("zeabur-202610{:02}030000", 23 - n);
        objects.push(backup(
            &name,
            60 * 7 + n * day,
            "Completed",
            json!({"warnings": u8::from(n == 4),
                "progress": {"itemsBackedUp": 1180 - n, "totalItems": 1180 - n}}),
        ));
        objects.push(volume(
            &name,
            "shop-uploads",
            1_840_000_000 - 9_000_000 * n.unsigned_abs(),
        ));
        objects.push(volume(&name, "blog-content", 312_000_000));
    }
    objects.push(volume(
        "zeabur-20261022030000",
        "shop-uploads",
        1_852_000_000,
    ));
    // Yesterday's restore of the blog: one object was already there.
    objects.push(from(json!({
        "apiVersion": "velero.io/v1", "kind": "Restore",
        "metadata": {"name": "zeabur-20261021030000-20261023091500", "namespace": "velero"},
        "spec": {"backupName": "zeabur-20261021030000", "includedNamespaces": ["blog"]},
        "status": {"phase": "Completed", "warnings": 1,
            "startTimestamp": minutes_ago(day - 120), "completionTimestamp": minutes_ago(day - 122)}
    })));
    objects
}

/// The files of a backup's archive: path and object.
type Files = Vec<(String, Value)>;

/// The objects every demo backup holds: the cluster as it starts, and the project `wiki`, deleted
/// since.
fn backed_up(s: &State) -> (BTreeMap<String, Vec<String>>, Files) {
    let mut list: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut files = Vec::new();
    let mut add = |key: &str, namespace: &str, name: &str| {
        list.entry(key.to_owned())
            .or_default()
            .push(format!("{namespace}/{name}"));
    };
    let name = |meta: &zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta| {
        meta.name.clone().unwrap_or_default()
    };
    for (namespace, objects) in &s.objects {
        for d in &objects.deployments {
            add("apps/v1/Deployment", namespace, &name(&d.metadata));
        }
        for d in &objects.stateful_sets {
            add("apps/v1/StatefulSet", namespace, &name(&d.metadata));
        }
        for d in &objects.services {
            add("v1/Service", namespace, &name(&d.metadata));
        }
        for d in &objects.secrets {
            add("v1/Secret", namespace, &name(&d.secret.metadata));
        }
        for d in &objects.cron_jobs {
            add("batch/v1/CronJob", namespace, &name(&d.metadata));
        }
        for ingress in &objects.ingresses {
            let route = name(&ingress.metadata);
            add("networking.k8s.io/v1/Ingress", namespace, &route);
            let body = typed(ingress, "networking.k8s.io/v1", "Ingress");
            files.push((
                format!(
                    "resources/ingresses.networking.k8s.io/namespaces/{namespace}/{route}.json"
                ),
                body,
            ));
        }
    }
    for (key, name) in [
        ("apps/v1/Deployment", "web"),
        ("v1/Service", "web"),
        ("networking.k8s.io/v1/Ingress", "web"),
    ] {
        add(key, "wiki", name);
    }
    files.push((
        "resources/ingresses.networking.k8s.io/namespaces/wiki/web.json".to_owned(),
        json!({"apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
            "metadata": {"name": "web", "namespace": "wiki"},
            "spec": {"rules": [{"host": "wiki.example.com"}]}}),
    ));
    let mut namespaces = vec![(
        "wiki".to_owned(),
        json!({"apiVersion": "v1", "kind": "Namespace",
            "metadata": {"name": "wiki", "labels": {"zeabur.com/project": "true"}}}),
    )];
    for namespace in &s.namespaces {
        let body = typed(namespace, "v1", "Namespace");
        namespaces.push((name(&namespace.metadata), body));
    }
    let mut names: Vec<String> = Vec::new();
    for (namespace, body) in namespaces {
        files.push((
            format!("resources/namespaces/cluster/{namespace}.json"),
            body,
        ));
        names.push(namespace);
    }
    names.sort();
    list.insert("v1/Namespace".to_owned(), names);
    for entries in list.values_mut() {
        entries.sort();
    }
    (list, files)
}

/// `object` as JSON with its `apiVersion` and `kind`.
fn typed(object: &impl serde::Serialize, api_version: &str, kind: &str) -> Value {
    let mut body = serde_json::to_value(object).expect("an example object");
    if let Some(map) = body.as_object_mut() {
        map.insert("apiVersion".to_owned(), json!(api_version));
        map.insert("kind".to_owned(), json!(kind));
    }
    body
}

/// Plays Velero's object store and its `DownloadRequest` controller: every backup's list of
/// objects and archive come from a store on 127.0.0.1, which the location names as its
/// `publicUrl`.
async fn backup_store(cluster: MemoryCluster) -> std::io::Result<()> {
    use std::io::Write as _;
    let Some((list, files)) = cluster.inspect(backed_up) else {
        return Ok(());
    };
    let gzip = |data: &[u8]| -> std::io::Result<Vec<u8>> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data)?;
        encoder.finish()
    };
    let mut archive = tar::Builder::new(Vec::new());
    for (path, body) in &files {
        let body = serde_json::to_vec(body)?;
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive.append_data(&mut header, path, body.as_slice())?;
    }
    let contents = Arc::new(gzip(&archive.into_inner()?)?);
    let listed = Arc::new(gzip(&serde_json::to_vec(&list)?)?);
    let router = axum::Router::new().route(
        "/{*path}",
        axum::routing::get(
            move |axum::extract::Path(path): axum::extract::Path<String>| async move {
                if path.ends_with("-resource-list.json.gz") {
                    listed.as_ref().clone()
                } else {
                    contents.as_ref().clone()
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    tokio::spawn(async move { axum::serve(listener, router).await });
    cluster.update(|s| {
        for location in s.dynamic.iter_mut().filter(|o| {
            o.types
                .as_ref()
                .is_some_and(|t| t.kind == "BackupStorageLocation")
        }) {
            if let Some(config) = location
                .data
                .pointer_mut("/spec/config")
                .and_then(Value::as_object_mut)
            {
                config.insert("publicUrl".to_owned(), json!(base));
            }
        }
    });
    tokio::spawn(async move {
        loop {
            cluster.update(|s| {
                for request in s.dynamic.iter_mut().filter(|o| {
                    o.types.as_ref().is_some_and(|t| t.kind == "DownloadRequest")
                        && o.data.get("status").is_none()
                }) {
                    let text = |pointer: &str| {
                        request
                            .data
                            .pointer(pointer)
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned()
                    };
                    let backup = text("/spec/target/name");
                    let file = if text("/spec/target/kind") == "BackupResourceList" {
                        "-resource-list.json.gz"
                    } else {
                        ".tar.gz"
                    };
                    let status = json!({
                        "phase": "Processed",
                        "downloadURL": format!("{base}/acme-backups/zeabur/backups/{backup}/{backup}{file}?X-Amz-Signature=demo"),
                        "expiration": minutes_ago(-10)
                    });
                    if let Some(object) = request.data.as_object_mut() {
                        object.insert("status".to_owned(), status);
                    }
                }
            });
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    });
    Ok(())
}

fn created(name: &str) -> String {
    let days = name.bytes().map(i64::from).sum::<i64>() % 60 + 14;
    minutes_ago(days * 24 * 60)
}

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
// a metrics store behind the service proxy that answers Zeabur's range queries. The worker runs
// close to its memory limit and, over a day, leaks memory until it is killed for it every nine
// hours; a Deployment rolled out 30 hours ago, so a week shows the pods it replaced.

const MIB: f64 = 1_048_576.0;

/// What `pod` uses, as shares of its template's CPU request and memory limit. `analytics` runs
/// many small workers, so the pods together stay below what the nodes' `NodeMetrics` say.
fn shares(pod: &DemoPod) -> (f64, f64) {
    let (cpu, memory) = match pod.app.as_str() {
        "web" => (0.35, 0.42),
        "api" => (1.6, 0.68),
        "worker" => (0.9, 0.93),
        "db" => (0.4, 0.61),
        "redis" => (0.08, 0.22),
        _ => (0.3, 0.4),
    };
    if pod.namespace == "analytics" && !pod.stateful {
        (cpu / 4.0, memory / 4.0)
    } else {
        (cpu, memory)
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
            let (cpu_share, memory_share) = shares(pod);
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

/// The service a pod pattern of `zeabur_modules::prometheus_api::pod_pattern` names.
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

/// What the sample store answers: Zeabur's probe, and its range queries for the demo's pods.
/// `ZEABUR_DEMO_FORBID=services/proxy` refuses it, as kube-apiserver does without the right.
#[expect(
    clippy::cast_precision_loss,
    reason = "Unix seconds and indexes are far below 2^52"
)]
#[expect(
    clippy::cast_possible_truncation,
    reason = "sample restart times, rounded to a second"
)]
fn store(path: &str) -> Result<Value, zeabur_core::problem::KubernetesStatus> {
    if std::env::var("ZEABUR_DEMO_FORBID")
        .is_ok_and(|list| list.split(',').any(|r| r == "services/proxy"))
    {
        return Err(zeabur_core::problem::KubernetesStatus::new(
            403,
            "Forbidden",
            "services \"vmsingle-vm-victoria-metrics-k8s-stack:8428\" is forbidden: User \
             \"zeabur:alice\" cannot get resource \"services/proxy\" in API group \"\" in \
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
    if route == zeabur_modules::prometheus_api::QUERY {
        let result: Vec<&Value> = DISKS
            .get()
            .into_iter()
            .flatten()
            .filter(|s| {
                let name = s
                    .pointer("/metric/__name__")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                (promql.contains("node_filesystem") && name.starts_with("node_filesystem"))
                    || (promql.contains("kubelet_volume_stats")
                        && name.starts_with("kubelet_volume_stats")
                        && s.pointer("/metric/namespace").and_then(Value::as_str)
                            == Some(namespace))
            })
            .collect();
        return Ok(
            json!({"status": "success", "data": {"resultType": "vector", "result": result}}),
        );
    }
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
            let (cpu_share, memory_share) = shares(&run.pod);
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

const GIB: u64 = 1 << 30;

/// The sample store's disk series, as an instant query's results.
static DISKS: std::sync::OnceLock<Vec<Value>> = std::sync::OnceLock::new();

/// One series of an instant query's answer.
fn sample(name: &str, labels: &Value, value: u64) -> Value {
    let mut metric = labels.as_object().cloned().unwrap_or_default();
    metric.insert("__name__".to_owned(), json!(name));
    json!({"metric": metric, "value": [Timestamp::now().as_second(), value.to_string()]})
}

/// The disk series the sample store holds: each node's filesystem and the volume of each
/// StatefulSet pod (which this also mounts in the pod, from its claim `data-<pod>`). The small
/// control plane's disk and shop's database volume are nearly full.
fn disks(objects: &mut BTreeMap<String, NamespaceObjects>) -> Vec<Value> {
    let mut series = Vec::new();
    for (namespace, objects) in objects.iter_mut() {
        let sizes: BTreeMap<String, u64> = objects
            .stateful_sets
            .iter()
            .filter_map(|set| {
                let size = set
                    .spec
                    .as_ref()?
                    .volume_claim_templates
                    .as_ref()?
                    .first()?
                    .spec
                    .as_ref()?
                    .resources
                    .as_ref()?
                    .requests
                    .as_ref()?
                    .get("storage")?;
                Some((
                    set.metadata.name.clone()?,
                    zeabur_core::quantity::bytes(&size.0)?,
                ))
            })
            .collect();
        for pod in &mut objects.pods {
            let owner = pod
                .metadata
                .owner_references
                .iter()
                .flatten()
                .find(|o| o.kind == "StatefulSet")
                .map(|o| o.name.clone());
            let (Some(owner), Some(name), Some(spec)) =
                (owner, pod.metadata.name.clone(), pod.spec.as_mut())
            else {
                continue;
            };
            let Some(capacity) = sizes.get(&owner).copied() else {
                continue;
            };
            let claim = format!("data-{name}");
            spec.volumes = Some(vec![from(
                json!({"name": "data", "persistentVolumeClaim": {"claimName": claim}}),
            )]);
            let percent: u64 = if namespace == "shop" && owner == "db" {
                93
            } else {
                12 + claim.bytes().map(u64::from).sum::<u64>() % 40
            };
            let used = capacity / 100 * percent;
            let labels = json!({"namespace": namespace, "persistentvolumeclaim": claim,
                "node": spec.node_name.clone().unwrap_or_default()});
            series.extend([
                sample("kubelet_volume_stats_capacity_bytes", &labels, capacity),
                sample("kubelet_volume_stats_used_bytes", &labels, used),
                sample(
                    "kubelet_volume_stats_available_bytes",
                    &labels,
                    capacity - used,
                ),
            ]);
        }
    }
    for (node, size, used) in [("vps-1", 40, 35), ("vps-2", 160, 61), ("vps-3", 160, 72)] {
        let (size, used) = (size * GIB, used * GIB);
        let labels = json!({"node": node, "mountpoint": "/", "fstype": "ext4"});
        series.extend([
            sample("node_filesystem_size_bytes", &labels, size),
            sample("node_filesystem_free_bytes", &labels, size - used),
            sample("node_filesystem_avail_bytes", &labels, size - used),
        ]);
    }
    series
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
/// The `ClusterIssuers`: `letsencrypt` answers HTTP-01 challenges through Traefik, and
/// `letsencrypt-dns` answers DNS-01 challenges through Cloudflare, which wildcard domains need.
fn cluster_issuers() -> Vec<DynamicObject> {
    let issuer = |name: &str, solver: Value| {
        from(json!({
            "apiVersion": "cert-manager.io/v1", "kind": "ClusterIssuer",
            "metadata": {"name": name, "resourceVersion": "7"},
            "spec": {"acme": {"server": "https://acme-v02.api.letsencrypt.org/directory",
                "email": "ops@example.com",
                "privateKeySecretRef": {"name": format!("{name}-account-key")},
                "solvers": [solver]}},
            "status": {"conditions": [{"type": "Ready", "status": "True", "reason": "ACMEAccountRegistered",
                "message": "The ACME account was registered with the ACME server"}]}
        }))
    };
    vec![
        issuer(
            "letsencrypt",
            json!({"http01": {"ingress": {"ingressClassName": "traefik"}}}),
        ),
        issuer(
            "letsencrypt-dns",
            json!({"dns01": {"cloudflare": {"apiTokenSecretRef":
                {"name": "letsencrypt-dns-dns-credentials", "key": "api-token"}}}}),
        ),
    ]
}

/// cert-manager's controller, whose namespace holds the secrets of `ClusterIssuers`.
fn cert_manager() -> NamespaceObjects {
    NamespaceObjects {
        namespace: "cert-manager".to_owned(),
        deployments: vec![from(json!({
            "metadata": {"name": "cert-manager", "namespace": "cert-manager",
                "labels": {"app.kubernetes.io/name": "cert-manager"}},
            "spec": {"selector": {"matchLabels": {"app": "cert-manager"}},
                "template": {"metadata": {"labels": {"app": "cert-manager"}},
                    "spec": {"containers": [{"name": "cert-manager-controller",
                        "image": "quay.io/jetstack/cert-manager-controller:v1.21.2",
                        "args": ["--v=2", "--cluster-resource-namespace=$(POD_NAMESPACE)"]}]}}}
        }))],
        ..NamespaceObjects::default()
    }
}

fn kube_system() -> NamespaceObjects {
    let pod = |name: &str, cpu: &str, memory: &str| -> Pod {
        let replica_set = name.rsplit_once('-').map_or(name, |(set, _)| set);
        let hash = replica_set.rsplit_once('-').map_or("", |(_, hash)| hash);
        from(
            json!({"metadata": {"name": name, "namespace": "kube-system",
                "labels": {"app": name, "pod-template-hash": hash},
                "ownerReferences": [{"apiVersion": "apps/v1", "kind": "ReplicaSet",
                    "name": replica_set, "uid": "00000000-0000-0000-0000-000000000000",
                    "controller": true}]},
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
        if pod.status.as_ref().and_then(|s| s.phase.as_deref()) == Some("Pending") {
            continue;
        }
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

/// One connection for every request.
struct Demo {
    cluster: Arc<dyn Cluster>,
    tools: bool,
}

impl ClusterAccess for Demo {
    fn for_request(&self, _parts: &Parts) -> Result<Arc<dyn Cluster>, Box<Problem>> {
        Ok(Arc::clone(&self.cluster))
    }

    fn handoff(&self, _parts: &Parts) -> Option<Handoff> {
        self.tools.then(|| Handoff {
            token: zeabur_kube::UserToken::new(SecretString::from("demo-token"))
                .expect("a valid token"),
            expires_at: Timestamp::now()
                .checked_add(SignedDuration::from_mins(30))
                .expect("in range"),
        })
    }
}

/// Starts what the in-memory cluster leaves waiting: added debug containers and tools pods.
/// The demo as an RKE2 cluster from Zeabur Installer (`ZEABUR_DEMO_RKE2`).
/// A node shell Bob opened on `vps-2` earlier, which ended when its half hour ran out.
fn leftover_shell(s: &mut State) {
    let image = zeabur_api::settings::DEBUG_IMAGE;
    let request = zeabur_plan::node_shell::NodeShell {
        node: "vps-2",
        node_uid: None,
        person: "zeabur:bob",
        image,
        made_at: "2026-10-23T02:10:00Z",
    };
    let mut pod: Pod = from(zeabur_plan::node_shell::pod_body(&request));
    pod.metadata.creation_timestamp = Some(from(json!("2026-10-23T02:10:01Z")));
    pod.metadata.uid = Some("node-shell-bob".to_owned());
    pod.status = Some(from(
        json!({"phase": "Failed", "reason": "DeadlineExceeded",
        "message": "Pod was active on the node longer than the specified deadline",
        "startTime": "2026-10-23T02:10:04Z"}),
    ));
    s.namespaces
        .push(from(zeabur_plan::node_shell::namespace_body()));
    s.objects
        .entry(zeabur_plan::node_shell::NAMESPACE.to_owned())
        .or_insert_with(|| NamespaceObjects {
            namespace: zeabur_plan::node_shell::NAMESPACE.to_owned(),
            ..NamespaceObjects::default()
        })
        .pods
        .push(pod);
}

fn rke2(s: &mut State) {
    leftover_shell(s);
    let rke2 = |v: &str| v.replace("+k3s1", "+rke2r1").replace("-k3s1", "");
    s.facts.version = rke2(&s.facts.version);
    for (i, node) in s.nodes.iter_mut().enumerate() {
        let status = node.status.get_or_insert_with(Default::default);
        if let Some(info) = status.node_info.as_mut() {
            info.kubelet_version = rke2(&info.kubelet_version);
            info.container_runtime_version = rke2(&info.container_runtime_version);
        }
        status.addresses = Some(vec![from(
            json!({"type": "InternalIP", "address": format!("10.0.0.{}", i + 5)}),
        )]);
        node.spec.get_or_insert_with(Default::default).pod_cidrs =
            Some(vec![format!("10.42.{i}.0/24")]);
        let labels = node.metadata.labels.get_or_insert_with(Default::default);
        labels.insert(
            "installer.zeabur.com/provider".to_owned(),
            "hetzner".to_owned(),
        );
        labels.insert(
            "topology.kubernetes.io/region".to_owned(),
            "fsn1".to_owned(),
        );
        labels.insert(
            "topology.kubernetes.io/zone".to_owned(),
            "fsn1-dc14".to_owned(),
        );
    }
    let system = s
        .objects
        .entry("kube-system".to_owned())
        .or_insert_with(|| NamespaceObjects {
            namespace: "kube-system".to_owned(),
            ..NamespaceObjects::default()
        });
    // RKE2 runs the control plane as static pods; kube-apiserver shows each as a mirror pod.
    for component in ["etcd", "kube-apiserver"] {
        system.pods.push(from(json!({
            "metadata": {"name": format!("{component}-vps-1"), "namespace": "kube-system",
                "uid": format!("kube-system-{component}-vps-1"),
                "labels": {"component": component, "tier": "control-plane"},
                "annotations": {"kubernetes.io/config.mirror": "2b6f0c1d9e8a7f65",
                                "kubernetes.io/config.source": "file"},
                "ownerReferences": [{"apiVersion": "v1", "kind": "Node", "name": "vps-1",
                    "uid": "node-vps-1", "controller": true}]},
            "spec": {"nodeName": "vps-1", "containers": [{"name": component, "image": "example",
                "resources": {"requests": {"cpu": "200m", "memory": "512Mi"}}}]},
            "status": {"phase": "Running", "containerStatuses": [{"name": component,
                "ready": true, "restartCount": 0, "image": "example", "imageID": "",
                "state": {"running": {}}}]}
        })));
    }
}

/// One S3 destination on the cluster's list, `r2-backups`, whose keys shop's Redis volume backups
/// copied.
fn s3_destinations(s: &mut State) {
    let namespace = "zeabur-s3-destinations";
    let labels = json!({"zeabur.com/s3-destination": "r2-backups"});
    s.namespaces
        .push(from(json!({"metadata": {"name": namespace}})));
    s.objects.insert(
        namespace.to_owned(),
        NamespaceObjects {
            namespace: namespace.to_owned(),
            config_maps: vec![from(json!({
                "metadata": {"name": "r2-backups", "namespace": namespace, "labels": labels,
                             "resourceVersion": "1"},
                "data": {"endpoint": "https://4f1c.r2.cloudflarestorage.com",
                         "bucket": "acme-backups", "region": "auto", "prefix": "zeabur"}
            }))],
            secrets: vec![SecretSummary {
                secret: from(json!({
                    "metadata": {"name": "r2-backups", "namespace": namespace, "labels": labels,
                                 "resourceVersion": "1"},
                    "type": "Opaque"
                })),
                key_count: 2,
            }],
            ..NamespaceObjects::default()
        },
    );
    if let Some(shop) = s.objects.get_mut("shop") {
        shop.secrets.push(SecretSummary {
            secret: from(json!({
                "metadata": {"name": "data-redis-0-backup", "namespace": "shop", "labels": labels,
                             "annotations": {
                                 "zeabur.com/s3-destination-keys":
                                     "AWS_ACCESS_KEY_ID,AWS_SECRET_ACCESS_KEY"},
                             "resourceVersion": "22"},
                "type": "Opaque"
            })),
            key_count: 3,
        });
    }
    s.secret_values.insert(
        format!("{namespace}/r2-backups"),
        [
            ("AWS_ACCESS_KEY_ID", "demo-access-key"),
            ("AWS_SECRET_ACCESS_KEY", "demo-secret-key"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.as_bytes().to_vec()))
        .collect(),
    );
}

/// What the node list counts per node: Deployment pods owned by their ReplicaSets, a DaemonSet's
/// pod on each node, Zeabur's own pod and a pod without a controller on `vps-3`, a `worker` with
/// scratch space, and the `db` whose volume local-path keeps on `vps-3`.
fn maintenance(s: &mut State) {
    for objects in s.objects.values_mut() {
        let deployments: Vec<(String, BTreeMap<String, String>)> = objects
            .deployments
            .iter()
            .filter_map(|d| {
                Some((
                    d.metadata.name.clone()?,
                    d.spec.as_ref()?.selector.match_labels.clone()?,
                ))
            })
            .collect();
        for pod in &mut objects.pods {
            let name = pod.metadata.name.clone().unwrap_or_default();
            if pod
                .metadata
                .owner_references
                .as_ref()
                .is_some_and(|o| !o.is_empty())
            {
                continue;
            }
            let labels = pod.metadata.labels.get_or_insert_with(Default::default);
            let owner = deployments
                .iter()
                .find(|(_, selector)| selector.iter().all(|(k, v)| labels.get(k) == Some(v)))
                .map(|(deployment, _)| deployment.clone());
            let Some(deployment) = owner else {
                continue;
            };
            let hash = name
                .strip_prefix(&format!("{deployment}-"))
                .and_then(|rest| rest.split('-').next())
                .filter(|hash| !hash.is_empty())
                .unwrap_or("5d8f7c9b6")
                .to_owned();
            labels.insert("pod-template-hash".to_owned(), hash.clone());
            pod.metadata.owner_references = Some(vec![from(json!({
                "apiVersion": "apps/v1", "kind": "ReplicaSet", "name": format!("{deployment}-{hash}"),
                "uid": "00000000-0000-0000-0000-000000000000", "controller": true}))]);
        }
        for pod in &mut objects.pods {
            let app = pod
                .metadata
                .labels
                .as_ref()
                .and_then(|l| l.get("app"))
                .cloned()
                .unwrap_or_default();
            let volume: Option<Value> = match app.as_str() {
                "worker" => Some(json!({"name": "scratch", "emptyDir": {}})),
                "db" if objects.namespace == "shop" => Some(
                    json!({"name": "data", "persistentVolumeClaim": {"claimName": "data-db-0"}}),
                ),
                _ => None,
            };
            if let (Some(volume), Some(spec)) = (volume, pod.spec.as_mut()) {
                if app == "db" {
                    spec.node_name = Some("vps-3".to_owned());
                }
                spec.volumes.get_or_insert_with(Vec::new).push(from(volume));
            }
        }
    }
    if let Some(shop) = s.objects.get_mut("shop") {
        for claim in &mut shop.persistent_volume_claims {
            if claim.metadata.name.as_deref() == Some("data-db-0") {
                claim.spec.get_or_insert_with(Default::default).volume_name =
                    Some("pvc-8e2d41c7-5b0a-4f3e-9c61-2a7d0e9b4f18".to_owned());
            }
        }
        if let Some(web) = shop.pods.iter_mut().find(|p| {
            p.metadata
                .labels
                .as_ref()
                .and_then(|l| l.get("app"))
                .map(String::as_str)
                == Some("web")
        }) {
            web.spec.get_or_insert_with(Default::default).node_name = Some("vps-3".to_owned());
        }
    }
    s.served.extend([ServedResource::readable(
        "",
        "v1",
        "persistentvolumes",
        "PersistentVolume",
        false,
    )]);
    s.dynamic.push(from(json!({
        "apiVersion": "v1", "kind": "PersistentVolume",
        "metadata": {"name": "pvc-8e2d41c7-5b0a-4f3e-9c61-2a7d0e9b4f18"},
        "spec": {"capacity": {"storage": "20Gi"}, "accessModes": ["ReadWriteOnce"],
            "storageClassName": "local-path",
            "claimRef": {"namespace": "shop", "name": "data-db-0"},
            "hostPath": {"path": "/var/lib/rancher/k3s/storage/pvc-8e2d41c7_shop_data-db-0"},
            "nodeAffinity": {"required": {"nodeSelectorTerms": [{"matchExpressions": [
                {"key": "kubernetes.io/hostname", "operator": "In", "values": ["vps-3"]}]}]}}},
        "status": {"phase": "Bound"}
    })));
    let nodes: Vec<String> = s
        .nodes
        .iter()
        .filter_map(|n| n.metadata.name.clone())
        .collect();
    let system = s
        .objects
        .entry("kube-system".to_owned())
        .or_insert_with(|| NamespaceObjects {
            namespace: "kube-system".to_owned(),
            ..NamespaceObjects::default()
        });
    for (i, node) in nodes.iter().enumerate() {
        system.pods.push(from(json!({
            "metadata": {"name": format!("svclb-traefik-4e1f2a9c-{}", ["x7k2p", "m3q8d", "t9w4z"].get(i).copied().unwrap_or("r5n1v")),
                "namespace": "kube-system", "labels": {"app": "svclb-traefik-4e1f2a9c"},
                "ownerReferences": [{"apiVersion": "apps/v1", "kind": "DaemonSet",
                    "name": "svclb-traefik-4e1f2a9c", "uid": "00000000-0000-0000-0000-000000000000",
                    "controller": true}]},
            "spec": {"nodeName": node, "containers": [{"name": "lb-tcp-80", "image": "example"}]},
            "status": {"phase": "Running", "containerStatuses": [{"name": "lb-tcp-80",
                "ready": true, "restartCount": 0, "image": "example", "imageID": "",
                "state": {"running": {}}}]}
        })));
    }
    s.namespaces
        .push(from(json!({"metadata": {"name": "zeabur"}})));
    s.objects.insert(
        "zeabur".to_owned(),
        NamespaceObjects {
            namespace: "zeabur".to_owned(),
            pods: vec![from(json!({
                "metadata": {"name": "zeabur-6c9f7d8b5-r2xkq", "namespace": "zeabur",
                    "labels": {"app.kubernetes.io/name": "zeabur", "pod-template-hash": "6c9f7d8b5"},
                    "ownerReferences": [{"apiVersion": "apps/v1", "kind": "ReplicaSet",
                        "name": "zeabur-6c9f7d8b5", "uid": "00000000-0000-0000-0000-000000000000",
                        "controller": true}]},
                "spec": {"nodeName": "vps-3", "containers": [{"name": "zeabur", "image": "example",
                    "resources": {"requests": {"cpu": "100m", "memory": "256Mi"}}}]},
                "status": {"phase": "Running", "containerStatuses": [{"name": "zeabur",
                    "ready": true, "restartCount": 0, "image": "example", "imageID": "",
                    "state": {"running": {}}}]}
            }))],
            ..NamespaceObjects::default()
        },
    );
    s.dynamic.push(from(json!({
        "apiVersion": "metrics.k8s.io/v1beta1", "kind": "PodMetrics",
        "metadata": {"name": "zeabur-6c9f7d8b5-r2xkq", "namespace": "zeabur"},
        "timestamp": minutes_ago(0), "window": "15.012s",
        "containers": [{"name": "zeabur", "usage": {"cpu": "14m", "memory": "41Mi"}}]
    })));
    if let Some(lab) = s.objects.get_mut("lab") {
        lab.pods.push(from(json!({
            "metadata": {"name": "debug-toolbox", "namespace": "lab",
                "labels": {"run": "debug-toolbox"}},
            "spec": {"nodeName": "vps-3", "containers": [{"name": "debug-toolbox",
                "image": "example"}]},
            "status": {"phase": "Running", "containerStatuses": [{"name": "debug-toolbox",
                "ready": true, "restartCount": 0, "image": "example", "imageID": "",
                "state": {"running": {}}}]}
        })));
    }
}

/// k3s's local-path as the default class and Longhorn beside it. Every claim gets a
/// PersistentVolume; a local-path one is pinned to the node of the pod that mounts it.
fn storage(s: &mut State) {
    let class = |name: &str, provisioner: &str, expands: bool, default: bool, binding: &str| {
        from::<StorageClass>(json!({"metadata": {"name": name, "annotations": {
                "storageclass.kubernetes.io/is-default-class": default.to_string()}},
            "provisioner": provisioner, "allowVolumeExpansion": expands,
            "reclaimPolicy": "Delete", "volumeBindingMode": binding}))
    };
    let refused = s
        .forbidden_resources
        .contains("storageclasses.storage.k8s.io");
    s.storage_classes = (!refused).then(|| {
        vec![
            class(
                "local-path",
                "rancher.io/local-path",
                false,
                true,
                "WaitForFirstConsumer",
            ),
            class("longhorn", "driver.longhorn.io", true, false, "Immediate"),
        ]
    });
    let first = s
        .nodes
        .first()
        .and_then(|n| n.metadata.name.clone())
        .unwrap_or_default();
    let mut volumes = Vec::new();
    for (namespace, objects) in &mut s.objects {
        let node_of = |claim: &str| {
            objects
                .pods
                .iter()
                .find(|pod| {
                    let name = pod.metadata.name.as_deref().unwrap_or_default();
                    claim == name
                        || claim == format!("data-{name}")
                        || pod
                            .spec
                            .iter()
                            .flat_map(|s| s.volumes.iter().flatten())
                            .any(|v| {
                                v.persistent_volume_claim
                                    .as_ref()
                                    .is_some_and(|c| c.claim_name == claim)
                            })
                })
                .and_then(|pod| pod.spec.as_ref()?.node_name.clone())
                .unwrap_or_else(|| first.clone())
        };
        let nodes: Vec<String> = objects
            .persistent_volume_claims
            .iter()
            .map(|c| node_of(c.metadata.name.as_deref().unwrap_or_default()))
            .collect();
        for (claim, node) in objects.persistent_volume_claims.iter_mut().zip(nodes) {
            let name = claim.metadata.name.clone().unwrap_or_default();
            let class = if namespace == "shop" && name == "data-redis-0" {
                "longhorn"
            } else {
                "local-path"
            };
            let spec = claim.spec.get_or_insert_with(Default::default);
            spec.storage_class_name
                .get_or_insert_with(|| class.to_owned());
            if spec.volume_name.is_some() {
                continue;
            }
            let volume = format!("pvc-{:016x}", hash(&format!("{namespace}/{name}")));
            spec.volume_name = Some(volume.clone());
            let size = spec
                .resources
                .as_ref()
                .and_then(|r| r.requests.as_ref())
                .and_then(|r| r.get("storage"))
                .map_or_else(String::new, |q| q.0.clone());
            let source = if class == "local-path" {
                json!({"hostPath": {"path": format!("/var/lib/rancher/k3s/storage/{volume}_{namespace}_{name}")},
                    "nodeAffinity": {"required": {"nodeSelectorTerms": [{"matchExpressions": [
                        {"key": "kubernetes.io/hostname", "operator": "In", "values": [node]}]}]}}})
            } else {
                json!({"csi": {"driver": "driver.longhorn.io", "volumeHandle": volume}})
            };
            let mut spec = json!({"capacity": {"storage": size}, "accessModes": ["ReadWriteOnce"],
                "storageClassName": class, "persistentVolumeReclaimPolicy": "Delete",
                "claimRef": {"namespace": namespace, "name": name}});
            if let (Some(spec), Some(source)) = (spec.as_object_mut(), source.as_object()) {
                spec.extend(source.clone());
            }
            volumes.push(from(json!({"apiVersion": "v1", "kind": "PersistentVolume",
                "metadata": {"name": volume}, "spec": spec, "status": {"phase": "Bound"}})));
        }
    }
    s.dynamic.extend(volumes);
}

fn kubelet(cluster: MemoryCluster) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(700)).await;
            let now = Timestamp::now().to_string();
            cluster.update(|s| {
                for objects in s.objects.values_mut() {
                    for pod in &mut objects.pods {
                        start(pod, &now);
                    }
                }
            });
        }
    });
}

fn start(pod: &mut Pod, now: &str) {
    let labels = pod.metadata.labels.as_ref();
    let tools = labels.is_some_and(|l| l.contains_key("zeabur.com/tools"));
    let shell = labels.is_some_and(|l| l.contains_key(zeabur_plan::node_shell::LABEL));
    let added: Vec<String> = pod
        .spec
        .as_ref()
        .and_then(|s| s.ephemeral_containers.as_ref())
        .into_iter()
        .flatten()
        .map(|c| c.name.clone())
        .collect();
    let mut status = serde_json::to_value(pod.status.clone().unwrap_or_default())
        .expect("a pod status serialises");
    let status_map = status.as_object_mut().expect("an object");
    let running = json!({"running": {"startedAt": now}});
    let mut changed = false;
    for name in added {
        let list = status_map
            .entry("ephemeralContainerStatuses")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .expect("a list");
        if !list
            .iter()
            .any(|c| c.get("name").and_then(Value::as_str) == Some(name.as_str()))
        {
            list.push(
                json!({"name": name, "image": "busybox", "imageID": "", "ready": false,
                "restartCount": 0, "state": running}),
            );
            changed = true;
        }
    }
    let phase = status_map.get("phase").and_then(Value::as_str);
    if (tools && phase != Some("Running")) || (shell && phase.is_none()) {
        let container = if tools {
            "tools"
        } else {
            zeabur_plan::node_shell::CONTAINER
        };
        status_map.insert("phase".to_owned(), json!("Running"));
        status_map.insert("startTime".to_owned(), json!(now));
        status_map.insert(
            "containerStatuses".to_owned(),
            json!([{"name": container, "image": container, "imageID": "", "ready": true,
                "restartCount": 0, "state": running}]),
        );
        changed = true;
    }
    if changed {
        pod.status = Some(from(status));
    }
}

/// Made-up idle use for the Error pages template; `ZEABUR_DEMO_TEMPLATE_IDLE=large` makes it more
/// than the cluster has free.
fn template_idle(id: &str) -> Option<zeabur_catalog::IdleUse> {
    let large = std::env::var("ZEABUR_DEMO_TEMPLATE_IDLE").is_ok_and(|v| v == "large");
    (id == "error-pages").then(|| zeabur_catalog::IdleUse {
        memory_bytes: if large { 24 << 30 } else { 9 << 20 },
        cpu_millicores: if large { 6000 } else { 1 },
        environment: "single-node-amd64-v1.36.4".to_owned(),
        measured_at: "2026-10-24".to_owned(),
    })
}

/// The demo's GitHub App: its key, and the installations of `ZEABUR_DEMO_GITHUB`.
struct FakeGitHub {
    pem: String,
}

const DEMO_CLIENT_ID: &str = "Iv23lidemo";

fn demo_installations() -> Value {
    json!([
        {"id": 4242, "account": {"login": "acme", "type": "Organization"},
         "repository_selection": "selected",
         "html_url": "https://github.com/organizations/acme/settings/installations/4242"},
        {"id": 7070, "account": {"login": "bob", "type": "User"},
         "repository_selection": "all",
         "html_url": "https://github.com/settings/installations/7070"},
    ])
}

fn demo_repositories(installation: &str) -> Value {
    let names: &[(&str, &str, bool)] = if installation == "4242" {
        &[
            ("acme/shop-web", "main", true),
            ("acme/shop-api", "main", true),
            (
                "acme/storefront-with-a-rather-long-repository-name",
                "trunk",
                true,
            ),
            ("acme/docs", "main", false),
        ]
    } else {
        &[("bob/blog", "main", false)]
    };
    let repositories: Vec<Value> = names
        .iter()
        .map(|(name, branch, private)| {
            json!({"full_name": name, "default_branch": branch, "private": private})
        })
        .collect();
    json!({"total_count": repositories.len(), "repositories": repositories})
}

/// GitHub's web pages and REST API as far as Zeabur uses them, at `http://127.0.0.1:<port>`.
/// Nothing checks the App's JWT.
async fn fake_github() -> std::io::Result<(String, Arc<FakeGitHub>)> {
    use aws_lc_rs::encoding::AsDer as _;
    use aws_lc_rs::rsa::{KeyPair, KeySize};
    use axum::extract::{Path, RawQuery, State as Shared};
    use axum::http::StatusCode;
    use axum::response::Html;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use base64::Engine as _;

    let key = KeyPair::generate(KeySize::Rsa2048).map_err(std::io::Error::other)?;
    let der = base64::engine::general_purpose::STANDARD
        .encode(key.as_der().map_err(std::io::Error::other)?.as_ref());
    let fake = Arc::new(FakeGitHub {
        pem: format!("-----BEGIN PRIVATE KEY-----\n{der}\n-----END PRIVATE KEY-----\n"),
    });
    let router = Router::new()
        // GitHub's confirmation page: a link back with a code. A redirect would break the form's
        // `form-action`, which GitHub does not do either.
        .route(
            "/settings/apps/new",
            post(async |RawQuery(query): RawQuery, body: String| {
                let field = |text: &str, name: &str| {
                    form_urlencoded::parse(text.as_bytes())
                        .find(|(k, _)| k == name)
                        .map_or_default(|(_, v)| v.into_owned())
                };
                let manifest: Value =
                    serde_json::from_str(&field(&body, "manifest")).unwrap_or_default();
                let back = manifest
                    .get("redirect_url")
                    .and_then(Value::as_str)
                    .unwrap_or("/");
                let state = field(&query.unwrap_or_default(), "state");
                let query = form_urlencoded::Serializer::new(String::new())
                    .append_pair("code", "demo-code")
                    .append_pair("state", &state)
                    .finish();
                let name = manifest
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .replace('&', "&amp;")
                    .replace('<', "&lt;");
                let link = format!("{back}?{query}")
                    .replace('&', "&amp;")
                    .replace('"', "&quot;");
                Html(format!(
                    "<!doctype html><title>GitHub (demo)</title><p>Create GitHub App \
                     <b>{name}</b> for acme?</p><p><a href=\"{link}\">Create GitHub App</a></p>"
                ))
            }),
        )
        .route(
            "/app-manifests/{code}/conversions",
            post(async |Shared(fake): Shared<Arc<FakeGitHub>>| {
                (
                    StatusCode::CREATED,
                    Json(
                        json!({"id": 1001, "slug": "zeabur-demo", "client_id": DEMO_CLIENT_ID,
                                "pem": fake.pem, "webhook_secret": null,
                                "html_url": "https://github.com/apps/zeabur-demo",
                                "owner": {"login": "acme", "type": "Organization"}}),
                    ),
                )
            }),
        )
        .route(
            "/app/installations",
            get(async || Json(demo_installations())),
        )
        .route(
            "/app/installations/{id}",
            get(async |Path(id): Path<i64>| {
                demo_installations()
                    .as_array()
                    .and_then(|all| all.iter().find(|i| i["id"] == id).cloned())
                    .map_or_else(
                        || (StatusCode::NOT_FOUND, Json(json!({"message": "Not Found"}))),
                        |i| (StatusCode::OK, Json(i)),
                    )
            }),
        )
        .route(
            "/app/installations/{id}/access_tokens",
            post(async |Path(id): Path<String>| {
                (
                    StatusCode::CREATED,
                    Json(json!({"token": format!("ghs_demo_{id}")})),
                )
            }),
        )
        .route(
            "/installation/repositories",
            get(async |headers: axum::http::HeaderMap| {
                let token = headers
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default();
                Json(demo_repositories(
                    token.trim_start_matches("Bearer ghs_demo_"),
                ))
            }),
        )
        .with_state(Arc::clone(&fake));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}", listener.local_addr()?);
    tokio::spawn(async move { axum::serve(listener, router).await });
    Ok((url, fake))
}

/// Zeabur Builder's namespace, empty for the manifest flow.
fn builder_system(s: &mut State) {
    s.namespaces
        .push(from(json!({"metadata": {"name": "builder-system"}})));
}

/// Zeabur Builder's namespace with the App of [`fake_github`] in it, installed on `acme` and
/// `bob`; `acme` granted to `shop`.
fn github_app(s: &mut State, fake: &FakeGitHub) {
    let namespace = "builder-system";
    builder_system(s);
    s.objects.insert(
        namespace.to_owned(),
        NamespaceObjects {
            namespace: namespace.to_owned(),
            config_maps: vec![from(json!({
                "metadata": {"name": "github-app-installations", "namespace": namespace,
                             "resourceVersion": "3"},
                "data": {"4242": "shop"}
            }))],
            secrets: vec![SecretSummary {
                secret: from(json!({
                    "metadata": {"name": "github-app", "namespace": namespace,
                                 "resourceVersion": "2",
                                 "annotations": {
                                     "builder.zeabur.com/github-app":
                                         "https://github.com/apps/zeabur-demo"}},
                    "type": "Opaque"
                })),
                key_count: 3,
            }],
            ..NamespaceObjects::default()
        },
    );
    s.secret_values.insert(
        format!("{namespace}/github-app"),
        [
            ("app-id", "1001"),
            ("client-id", DEMO_CLIENT_ID),
            ("private-key", fake.pem.as_str()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.as_bytes().to_vec()))
        .collect(),
    );
    let repositories: Vec<Value> = demo_repositories("4242")
        .get("repositories")
        .and_then(Value::as_array)
        .map_or_default(|all| {
            all.iter()
                .map(|r| {
                    let field = |name: &str| r.get(name).cloned().unwrap_or_default();
                    json!({"fullName": field("full_name"), "defaultBranch": field("default_branch"),
                           "private": field("private")})
                })
                .collect()
        });
    let granted = json!({"id": 4242, "account": "acme", "repositories": repositories,
                         "refreshedAt": minutes_ago(90)});
    if let Some(shop) = s.objects.get_mut("shop") {
        shop.config_maps.push(from(json!({
            "metadata": {"name": "zeabur-github-app", "namespace": "shop",
                         "labels": {"zeabur.com/github-app": "true"}, "resourceVersion": "4"},
            "data": {"4242": granted.to_string()}
        })));
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cluster = MemoryCluster::new(state());
    if std::env::var("ZEABUR_DEMO_FORBID")
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
    cluster.update(|s| {
        maintenance(s);
        storage(s);
        s3_destinations(s);
        // A debug container's patch tests the pod's uid.
        for (namespace, objects) in &mut s.objects {
            for pod in &mut objects.pods {
                let name = pod.metadata.name.clone().unwrap_or_default();
                pod.metadata
                    .uid
                    .get_or_insert_with(|| format!("{namespace}-{name}"));
            }
        }
        for node in &mut s.nodes {
            let name = node.metadata.name.clone().unwrap_or_default();
            node.metadata
                .uid
                .get_or_insert_with(|| format!("node-{name}"));
        }
        s.without_shell
            .extend(["shop/web-7c9d8-abcde", "shop/web-7c9d8-fghij"].map(str::to_owned));
        DISKS.get_or_init(|| disks(&mut s.objects));
    });
    if std::env::var("ZEABUR_DEMO_FORBID")
        .is_ok_and(|list| list.split(',').any(|r| r == "namespaces"))
    {
        cluster.update(|s| s.cluster_wide_denied = true);
    }
    if std::env::var_os("ZEABUR_DEMO_RKE2").is_some() {
        cluster.update(rke2);
    }
    let github = match std::env::var("ZEABUR_DEMO_GITHUB") {
        Ok(mode) => {
            let (url, fake) = fake_github().await?;
            if mode == "installed" {
                cluster.update(|s| github_app(s, &fake));
            } else {
                cluster.update(builder_system);
            }
            Some(url)
        }
        Err(_) => None,
    };
    kubelet(cluster.clone());
    if !matches!(velero_mode().as_str(), "none" | "location") {
        backup_store(cluster.clone()).await?;
    }
    let tools = std::env::var_os("ZEABUR_DEMO_TOOLS").is_some();
    let cluster: Arc<dyn Cluster> = Arc::new(cluster);
    let terminals = tools.then(|| Arc::clone(&cluster));
    let access = Demo { cluster, tools };
    // Settings are kept in an `zeabur` namespace, as on the in-cluster server.
    let settings = zeabur_api::Settings {
        namespace: Some("zeabur".to_owned()),
        identity: zeabur_api::settings::IdentitySettings {
            issuer: Some("https://identity.example.com".to_owned()),
            project_roles: true,
            ..zeabur_api::settings::IdentitySettings::default()
        },
        domains: if std::env::var_os("ZEABUR_DEMO_NO_DOMAINS").is_some() {
            zeabur_api::settings::DomainSettings::default()
        } else {
            zeabur_api::settings::DomainSettings {
                suffixes: vec!["apps.example.com".to_owned()],
                cluster_issuer: Some("letsencrypt".to_owned()),
            }
        },
        builds: zeabur_api::settings::BuildSettings {
            registry: Some("zot.zot.svc:5000".to_owned()),
        },
        github: github.map_or_default(|url| zeabur_api::settings::GitHubSettings {
            web_url: url.clone(),
            api_url: url,
        }),
        ..zeabur_api::Settings::default()
    };
    let images = zeabur_api::images::Inspector::new(Arc::new(Registry::new()), Vec::new());
    let app = zeabur_api::router_for_demo(
        Arc::new(access),
        settings,
        Arc::new(dns()),
        Arc::new(images),
        template_idle,
        terminals,
    )
    .fallback_service(zeabur_ui::router());
    // Another port when 8080 is taken: ZEABUR_DEMO_ADDR=127.0.0.1:8090.
    let addr = std::env::var("ZEABUR_DEMO_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await
}
