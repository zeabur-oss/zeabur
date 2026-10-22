//! Objects written as YAML on a real cluster, as a person with a namespace-scoped role: the plan
//! dry-runs each object with the person's token, so what RBAC, a `ValidatingAdmissionPolicy` or a
//! quota refuses is refused before anything is written, at its document; a name that exists, a
//! cluster-scoped kind and another namespace are refused by Zeabur; a custom resource is created
//! at the plural discovery serves; an apply a quota stops half-way names what it created, and
//! deleting exactly that is a plan like any other. A Secret's value never shows in a plan, a
//! problem or the log.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::http::StatusCode;
use kube::Client;
use kube::api::{Api, DeleteParams, DynamicObject, PostParams};
use kube::core::{ApiResource, GroupVersionKind};
use serde_json::{Value, json};
use tracing_subscriber::util::SubscriberInitExt as _;
use zeabur_test_support::sandbox::{Sandbox, create};

use crate::common::{app, send};

const SECRET_VALUE: &str = "s3cr3t-t0ken-that-must-not-leak";
/// The value's base64, as `data` holds it.
const SECRET_BASE64: &str = "czNjcjN0LXQwa2VuLXRoYXQtbXVzdC1ub3QtbGVhaw";
const GROUP: &str = "create.zeabur.test";

const CONFIG: &str = "apiVersion: v1
kind: ConfigMap
metadata:
  name: hello-config
data:
  GREETING: hello
";
const DEPLOYMENT: &str = "apiVersion: apps/v1
kind: Deployment
metadata:
  name: hello
  labels: {team: web}
spec:
  replicas: 0
  selector:
    matchLabels: {app: hello}
  template:
    metadata:
      labels: {app: hello}
    spec:
      containers:
        - name: hello
          image: busybox:1.37
          envFrom:
            - configMapRef: {name: hello-config}
            - secretRef: {name: hello-env}
";
const SERVICE: &str = "apiVersion: v1
kind: Service
metadata:
  name: hello
spec:
  selector: {app: hello}
  ports:
    - port: 80
";

fn secret(name: &str) -> String {
    format!(
        "apiVersion: v1\nkind: Secret\nmetadata:\n  name: {name}\nstringData:\n  token: {SECRET_VALUE}\n"
    )
}

fn creates(documents: &[&str]) -> Value {
    json!({"changes": documents.iter().map(|y| json!({"op": "create", "yaml": y})).collect::<Vec<_>>()})
}

async fn plan(app: &Router, ns: &str, request: &Value) -> (StatusCode, Value) {
    send(
        app,
        "POST",
        &format!("/api/v1/projects/{ns}/plan"),
        Some(request),
    )
    .await
}

async fn apply(app: &Router, ns: &str, planned: &Value) -> (StatusCode, Value) {
    send(
        app,
        "POST",
        &format!("/api/v1/projects/{ns}/apply"),
        Some(&json!({"token": planned["token"]})),
    )
    .await
}

fn dynamic(admin: &Client, group: &str, version: &str, kind: &str) -> Api<DynamicObject> {
    Api::all_with(
        admin.clone(),
        &ApiResource::from_gvk(&GroupVersionKind::gvk(group, version, kind)),
    )
}

fn namespaced(admin: &Client, ns: &str, group: &str, kind: &str) -> Api<DynamicObject> {
    Api::namespaced_with(
        admin.clone(),
        ns,
        &ApiResource::from_gvk(&GroupVersionKind::gvk(group, "v1", kind)),
    )
}

/// What is logged while the test runs, at `INFO` as the server logs (crates/zeabur/src/main.rs).
/// Below it kube-rs writes a refusal with kube-apiserver's message, which may quote a value.
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<u8>>>);

impl Log {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

impl std::io::Write for Log {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .map_err(|poisoned| std::io::Error::other(poisoned.to_string()))?
            .extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn no_secret(what: &str, text: &str) {
    let leaks: Vec<&str> = text
        .lines()
        .filter(|line| line.contains(SECRET_VALUE) || line.contains(SECRET_BASE64))
        .collect();
    assert!(leaks.is_empty(), "{what}: {leaks:#?}");
}

fn rules() -> Value {
    json!([
        {"apiGroups": [""], "resources": ["configmaps", "secrets", "services", "persistentvolumeclaims"],
         "verbs": ["get", "list", "create", "delete"]},
        {"apiGroups": ["apps"], "resources": ["deployments"],
         "verbs": ["get", "list", "create", "delete"]},
        {"apiGroups": [GROUP], "resources": ["widgets"],
         "verbs": ["get", "list", "create", "delete"]},
        {"apiGroups": ["", "apps", "batch", "events.k8s.io"], "resources": ["*"],
         "verbs": ["get", "list"]}
    ])
}

/// The CRD the custom-resource case creates in; cluster-wide, so made once and left for the
/// next run.
async fn widgets(admin: &Client) {
    let crds = dynamic(
        admin,
        "apiextensions.k8s.io",
        "v1",
        "CustomResourceDefinition",
    );
    let crd: DynamicObject = serde_json::from_value(json!({
        "apiVersion": "apiextensions.k8s.io/v1", "kind": "CustomResourceDefinition",
        "metadata": {"name": format!("widgets.{GROUP}")},
        "spec": {"group": GROUP, "scope": "Namespaced",
                 "names": {"plural": "widgets", "singular": "widget", "kind": "Widget"},
                 "versions": [{"name": "v1", "served": true, "storage": true,
                               "schema": {"openAPIV3Schema": {"type": "object",
                                   "x-kubernetes-preserve-unknown-fields": true}}}]}
    }))
    .unwrap();
    let _exists = crds.create(&PostParams::default(), &crd).await;
    let deadline = Instant::now() + Duration::from_secs(60);
    while discovery_lacks_widgets(admin).await {
        assert!(Instant::now() < deadline, "the CRD is not served");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn discovery_lacks_widgets(admin: &Client) -> bool {
    admin
        .list_api_group_resources(&format!("{GROUP}/v1"))
        .await
        .map_or(true, |list| {
            list.resources.iter().all(|r| r.name != "widgets")
        })
}

/// A policy on the namespace: a Deployment needs a `team` label, and a Secret called `leaky` is
/// refused with its value quoted, as a careless policy would.
async fn policy(admin: &Client, ns: &str) -> (Api<DynamicObject>, Api<DynamicObject>) {
    let policies = dynamic(
        admin,
        "admissionregistration.k8s.io",
        "v1",
        "ValidatingAdmissionPolicy",
    );
    let bindings = dynamic(
        admin,
        "admissionregistration.k8s.io",
        "v1",
        "ValidatingAdmissionPolicyBinding",
    );
    create(
        &policies,
        json!({"apiVersion": "admissionregistration.k8s.io/v1", "kind": "ValidatingAdmissionPolicy",
               "metadata": {"name": ns},
               "spec": {"failurePolicy": "Fail",
                        "matchConstraints": {"resourceRules": [
                            {"apiGroups": ["apps"], "apiVersions": ["v1"], "operations": ["CREATE"],
                             "resources": ["deployments"]},
                            {"apiGroups": [""], "apiVersions": ["v1"], "operations": ["CREATE"],
                             "resources": ["secrets"]}]},
                        "validations": [
                            {"expression": "request.kind.kind != 'Deployment' || (has(object.metadata.labels) && 'team' in object.metadata.labels)",
                             "message": "every Deployment needs a team label"},
                            {"expression": "request.kind.kind != 'Secret' || object.metadata.name != 'leaky'",
                             "messageExpression": "'token ' + string(object.data['token']) + ' is a known leaked token'"}]}}),
    )
    .await;
    create(
        &bindings,
        json!({"apiVersion": "admissionregistration.k8s.io/v1", "kind": "ValidatingAdmissionPolicyBinding",
               "metadata": {"name": ns},
               "spec": {"policyName": ns, "validationActions": ["Deny"],
                        "matchResources": {"namespaceSelector": {"matchLabels":
                            {"kubernetes.io/metadata.name": ns}}}}}),
    )
    .await;
    (policies, bindings)
}

#[tokio::test]
#[ignore = "needs a disposable cluster"]
#[expect(clippy::too_many_lines, reason = "one scenario, read top to bottom")]
async fn objects_written_as_yaml_are_created_as_the_person() {
    let log = Log::default();
    let writer = log.clone();
    let _logging = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .set_default();

    let sandbox = Sandbox::new("zeabur-create", rules()).await;
    let (ns, admin) = (sandbox.namespace().to_owned(), sandbox.admin().clone());
    widgets(&admin).await;
    let app = app(sandbox.user());

    // What the person may create, from discovery and their rules.
    let (status, list) = send(
        &app,
        "GET",
        &format!("/api/v1/projects/{ns}/creatable"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let access = |kind: &str| {
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|k| k["kind"] == kind)
            .map(|k| (k["access"].clone(), k["custom"].clone()))
    };
    assert_eq!(access("Deployment"), Some((json!("allowed"), json!(false))));
    assert_eq!(access("CronJob"), Some((json!("denied"), json!(false))));
    assert_eq!(access("Widget"), Some((json!("allowed"), json!(true))));
    assert!(
        list["clusterScoped"]
            .as_array()
            .unwrap()
            .contains(&json!({"group": "rbac.authorization.k8s.io", "kind": "ClusterRole"})),
        "{list}"
    );

    // Four objects in the order written; created dependencies first, the Secret's value hidden.
    let hello_env = secret("hello-env");
    let (status, planned) = plan(
        &app,
        &ns,
        &creates(&[DEPLOYMENT, SERVICE, &hello_env, CONFIG]),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{planned}");
    no_secret("plan", &planned.to_string());
    let order: Vec<&str> = planned["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["target"]["kind"].as_str().unwrap())
        .collect();
    assert_eq!(order, ["Secret", "ConfigMap", "Service", "Deployment"]);
    let (status, applied) = apply(&app, &ns, &planned).await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    no_secret("apply", &applied.to_string());
    let stored = namespaced(&admin, &ns, "", "Secret")
        .get("hello-env")
        .await
        .unwrap();
    assert_eq!(stored.data["data"]["token"], format!("{SECRET_BASE64}=="));
    let (status, document) = send(&app, "GET", &format!("/api/v1/projects/{ns}"), None).await;
    assert_eq!(status, StatusCode::OK, "{document}");
    assert!(
        document["services"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "hello"),
        "{document}"
    );

    // A name that exists is refused, never updated.
    let (status, problem) = plan(&app, &ns, &creates(&[SERVICE])).await;
    assert_eq!(status, StatusCode::CONFLICT, "{problem}");
    assert_eq!(
        problem["at"],
        json!({"change": 0, "field": "metadata.name"})
    );

    // A kind outside the person's rights is refused by kube-apiserver at the dry run.
    let cron = "apiVersion: batch/v1\nkind: CronJob\nmetadata:\n  name: nightly\nspec:\n  \
                schedule: '0 3 * * *'\n  jobTemplate:\n    spec:\n      template:\n        spec:\n          \
                restartPolicy: OnFailure\n          containers:\n            - name: n\n              \
                image: busybox:1.37\n";
    let other = CONFIG.replace("hello-config", "other-config");
    let (status, problem) = plan(&app, &ns, &creates(&[&other, cron])).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    assert_eq!(problem["kind"], "forbidden", "{problem}");
    assert_eq!(problem["at"]["change"], 1, "{problem}");

    // A cluster-scoped kind and another namespace are refused before any dry run.
    let role =
        "apiVersion: rbac.authorization.k8s.io/v1\nkind: ClusterRole\nmetadata:\n  name: x\n";
    let (status, problem) = plan(&app, &ns, &creates(&[&other, role])).await;
    assert!(status.is_client_error(), "{problem}");
    assert_eq!(
        problem["at"],
        json!({"change": 1, "field": "kind"}),
        "{problem}"
    );
    let elsewhere = other.replace(
        "  name: other-config\n",
        "  name: other-config\n  namespace: default\n",
    );
    let (status, problem) = plan(&app, &ns, &creates(&[&elsewhere])).await;
    assert!(status.is_client_error(), "{problem}");
    assert_eq!(
        problem["at"],
        json!({"change": 0, "field": "metadata.namespace"}),
        "{problem}"
    );

    // A custom resource, at the plural discovery serves.
    let widget = format!(
        "apiVersion: {GROUP}/v1\nkind: Widget\nmetadata:\n  name: knob\nspec:\n  size: 3\n"
    );
    let (status, planned) = plan(&app, &ns, &creates(&[&widget])).await;
    assert_eq!(status, StatusCode::OK, "{planned}");
    assert_eq!(planned["objects"][0]["target"]["plural"], "widgets");
    let (status, applied) = apply(&app, &ns, &planned).await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    let widgets = Api::<DynamicObject>::namespaced_with(
        admin.clone(),
        &ns,
        &ApiResource::from_gvk(&GroupVersionKind::gvk(GROUP, "v1", "Widget")),
    );
    assert_eq!(widgets.get("knob").await.unwrap().data["spec"]["size"], 3);

    // A ValidatingAdmissionPolicy refuses at the dry run; quoting a Secret's value, it is hidden.
    let (policies, bindings) = policy(&admin, &ns).await;
    let unlabelled = DEPLOYMENT.replace(
        "  name: hello\n  labels: {team: web}\n",
        "  name: unlabelled\n",
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    let problem = loop {
        let (status, problem) = plan(&app, &ns, &creates(&[&other, &unlabelled])).await;
        if status != StatusCode::OK {
            break problem;
        }
        assert!(Instant::now() < deadline, "the policy never applied");
        tokio::time::sleep(Duration::from_secs(1)).await;
    };
    assert_eq!(problem["kind"], "policy-denied", "{problem}");
    assert_eq!(problem["at"]["change"], 1, "{problem}");
    assert!(problem.to_string().contains("team label"), "{problem}");
    let leaky = secret("leaky");
    let (status, problem) = plan(&app, &ns, &creates(&[&leaky])).await;
    assert!(status.is_client_error(), "{problem}");
    assert_eq!(problem["kind"], "policy-denied", "{problem}");
    assert_eq!(problem["at"]["change"], 0, "{problem}");
    no_secret("problem", &problem.to_string());
    let _ignored = bindings.delete(&ns, &DeleteParams::default()).await;
    let _ignored = policies.delete(&ns, &DeleteParams::default()).await;

    no_secret("log", &log.text());
}

/// A quota refuses a claim at the dry run; ConfigMaps it allows one at a time pass their dry
/// runs and stop the apply at the second, which names the first for the cleanup.
#[tokio::test]
#[ignore = "needs a disposable cluster"]
async fn a_quota_refuses_at_the_plan_or_part_way_with_a_cleanup() {
    let sandbox = Sandbox::new("zeabur-create-quota", rules()).await;
    let (ns, admin) = (sandbox.namespace().to_owned(), sandbox.admin().clone());
    let quotas = namespaced(&admin, &ns, "", "ResourceQuota");
    // `kube-root-ca.crt` is the namespace's first ConfigMap.
    create(
        &quotas,
        json!({"apiVersion": "v1", "kind": "ResourceQuota", "metadata": {"name": "limits"},
               "spec": {"hard": {"count/configmaps": "2", "persistentvolumeclaims": "0"}}}),
    )
    .await;
    let deadline = Instant::now() + Duration::from_secs(60);
    while quotas
        .get("limits")
        .await
        .unwrap()
        .data
        .pointer("/status/used/count~1configmaps")
        != Some(&json!("1"))
    {
        assert!(Instant::now() < deadline, "no quota status");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let app = app(sandbox.user());

    let claim = "apiVersion: v1\nkind: PersistentVolumeClaim\nmetadata:\n  name: data\nspec:\n  \
                 accessModes: [ReadWriteOnce]\n  resources:\n    requests:\n      storage: 1Gi\n";
    let (status, problem) = plan(&app, &ns, &creates(&[SERVICE, claim])).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    assert_eq!(problem["kind"], "quota-exceeded", "{problem}");
    assert_eq!(problem["at"]["change"], 1, "{problem}");

    let second = CONFIG.replace("hello-config", "second-config");
    let (status, planned) = plan(&app, &ns, &creates(&[CONFIG, &second, SERVICE])).await;
    assert_eq!(status, StatusCode::OK, "{planned}");
    let (status, problem) = apply(&app, &ns, &planned).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    assert_eq!(problem["kind"], "quota-exceeded", "{problem}");
    assert_eq!(problem["at"]["change"], 1, "{problem}");
    let applied = problem["applied"].as_array().unwrap();
    assert_eq!(applied.len(), 1, "{problem}");
    assert_eq!(applied[0]["name"], "hello-config");
    let configs = namespaced(&admin, &ns, "", "ConfigMap");
    assert!(configs.get_opt("second-config").await.unwrap().is_none());
    assert!(
        namespaced(&admin, &ns, "", "Service")
            .get_opt("hello")
            .await
            .unwrap()
            .is_none(),
        "not attempted"
    );

    let cleanup = json!({"changes": [{"op": "delete", "name": "hello-config",
        "resourceVersion": applied[0]["resourceVersion"],
        "resource": {"group": "", "version": "v1", "resource": "configmaps"}}]});
    let (status, planned) = plan(&app, &ns, &cleanup).await;
    assert_eq!(status, StatusCode::OK, "{planned}");
    let (status, deleted) = apply(&app, &ns, &planned).await;
    assert_eq!(status, StatusCode::OK, "{deleted}");
    let deadline = Instant::now() + Duration::from_secs(30);
    while configs.get_opt("hello-config").await.unwrap().is_some() {
        assert!(Instant::now() < deadline, "not deleted");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
