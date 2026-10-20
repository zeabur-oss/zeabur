//! The API against a real kube-apiserver, as a person with a namespace-scoped role: projects,
//! ConfigMaps and Secrets read as them, Secrets listed without their values, the cluster's
//! version readable by anyone signed in, backup providers not installed, the first request after
//! their RoleBinding is deleted answered with `403`, and writes through plan and apply (Helm's
//! fields kept, one winner of 64 concurrent applies, a write outside the role refused before
//! anything is written, the other keys of a Secret the person cannot read kept, and the object
//! stored being the one the plan showed), and usage and history read as a person with `view`
//! (and, for history, `services/proxy`).
//!
//! Needs a disposable cluster where the current kubeconfig context is cluster admin
//! (CONTRIBUTING.md, "Tests"); the admin connection only prepares the namespace and the person.

#[cfg(test)]
mod common;

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt as _;
    use kube::Client;
    use kube::api::{Api, DeleteParams};
    use serde_json::{Value, json};
    use tower::ServiceExt as _;
    use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, ServiceAccount};
    use zeabur_kube::k8s_openapi::api::rbac::v1::{Role, RoleBinding};
    use zeabur_test_support::sandbox::{PERSON, Sandbox, create};

    use crate::common::{app, app_with, plan_and_apply, planned_and_applied, send};

    const SECRET_VALUE: &str = "s3cr3t-value-that-must-not-leak";

    async fn call(app: &Router, method: &str, uri: &str) -> (StatusCode, String) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8(body.to_vec()).unwrap())
    }

    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    #[expect(clippy::too_many_lines, reason = "one scenario, read top to bottom")]
    async fn the_api_reads_as_the_person_and_stops_when_their_right_goes() {
        let sandbox = Sandbox::new(
            "zeabur-api",
            json!([
                {"apiGroups": ["", "apps", "batch", "networking.k8s.io", "events.k8s.io"],
                 "resources": ["*"], "verbs": ["get", "list"]}
            ]),
        )
        .await;
        let (ns, admin) = (sandbox.namespace().to_owned(), sandbox.admin().clone());

        let admin_api = |kind: &str| -> Api<kube::api::DynamicObject> {
            let resource =
                kube::discovery::ApiResource::from_gvk(&kube::core::GroupVersionKind::gvk(
                    if kind == "Deployment" { "apps" } else { "" },
                    "v1",
                    kind,
                ));
            Api::namespaced_with(admin.clone(), &ns, &resource)
        };
        create(
            &admin_api("Secret"),
            json!({"apiVersion": "v1", "kind": "Secret",
                   "metadata": {"name": "db", "namespace": ns},
                   "type": "Opaque", "stringData": {"password": SECRET_VALUE}}),
        )
        .await;
        create(
            &admin_api("Deployment"),
            json!({"apiVersion": "apps/v1", "kind": "Deployment",
                   "metadata": {"name": "web", "namespace": ns},
                   "spec": {"replicas": 0, "selector": {"matchLabels": {"app": "web"}},
                            "template": {"metadata": {"labels": {"app": "web"}},
                                         "spec": {"containers": [{"name": "web", "image": "busybox:1.37",
                                             "env": [{"name": "PASSWORD", "valueFrom": {"secretKeyRef": {"name": "db", "key": "password"}}}]}]}}}}),
        )
        .await;

        let person = sandbox.user().clone();
        let app = app(&person);

        let (status, body) = call(&app, "GET", &format!("/api/v1/projects/{ns}")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let project: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(project["services"][0]["name"], "web");
        assert_eq!(
            project["services"][0]["id"],
            format!("projects/{ns}/services/deployment/web")
        );

        // The Secret is listed without its values: how many keys it holds.
        let secret = project["resources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == format!("projects/{ns}/resources/secret/db"))
            .unwrap();
        assert_eq!(secret["kind"], "Secret", "{secret}");
        assert_eq!(secret["keyCount"], 1);
        let (status, body) = call(&app, "GET", &format!("/api/v1/projects/{ns}/configmaps")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        for listing in [&body, &project.to_string()] {
            assert!(!listing.contains(SECRET_VALUE));
            assert!(!listing.contains("czNjcjN0")); // the value's base64 prefix
        }

        let (status, body) = call(&app, "GET", "/api/v1/projects").await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "listing namespaces is not in the role: {body}"
        );
        let (status, body) = call(&app, "GET", "/api/v1/cluster/capabilities").await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        let (status, body) = call(&app, "GET", "/api/v1/cluster").await;
        assert_eq!(
            status,
            StatusCode::OK,
            "the version is public information: {body}"
        );

        // Backups: discovery says no provider is installed on a plain k3s, before any of the
        // person's rights matter (Velero's objects would be read cluster-wide).
        let (status, body) = call(&app, "GET", &format!("/api/v1/projects/{ns}/backups")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let backups: Value = serde_json::from_str(&body).unwrap();
        let providers = backups["providers"].as_array().unwrap();
        let mut modules: Vec<&str> = providers
            .iter()
            .filter_map(|p| p["module"].as_str())
            .collect();
        modules.sort_unstable();
        assert_eq!(modules, ["cloudnative-pg", "k8up", "velero"]);
        assert!(
            providers.iter().all(|p| p["state"] == "not-installed"),
            "{body}"
        );

        Api::<RoleBinding>::namespaced(admin.clone(), &ns)
            .delete(PERSON, &DeleteParams::default())
            .await
            .unwrap();
        // kube-apiserver's RBAC authorizer reads bindings from a watch cache, so a deletion takes
        // effect within moments rather than on the very next request. Zeabur adds no cache of its
        // own: the first request after the cache catches up is refused.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let (status, body) = loop {
            let (status, body) = call(&app, "GET", &format!("/api/v1/projects/{ns}")).await;
            if status != StatusCode::OK {
                break (status, body);
            }
            assert!(
                std::time::Instant::now() < deadline,
                "still allowed 5 s after the binding was deleted"
            );
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        };
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        let problem: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(problem["kubernetes"]["reason"], "Forbidden");
    }

    async fn post(app: &Router, uri: &str, body: &Value) -> (StatusCode, Value) {
        send(app, "POST", uri, Some(body)).await
    }

    fn set_env(name: &str, value: &str) -> Value {
        json!({"changes": [{"op": "env", "workload": {"kind": "Deployment", "name": "web"},
            "changes": [{"op": "set", "name": name, "value": value}]}]})
    }

    /// Writes as the person: a field Helm owns is not taken over; a change goes through plan and
    /// apply and lands as the field manager `zeabur`; of 64 concurrent applies of one plan exactly
    /// one succeeds; a write the person's role does not allow is refused by the dry run and
    /// writes nothing.
    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    #[expect(clippy::too_many_lines, reason = "one scenario, read top to bottom")]
    async fn writes_go_through_plan_and_apply_as_the_person() {
        use kube::api::{Patch, PatchParams};

        // Reads everywhere in the namespace; writes Deployments only.
        let sandbox = Sandbox::new(
            "zeabur-writes",
            json!([
                {"apiGroups": ["", "apps", "batch", "networking.k8s.io", "events.k8s.io"],
                 "resources": ["*"], "verbs": ["get", "list"]},
                {"apiGroups": ["apps"], "resources": ["deployments"], "verbs": ["patch"]}
            ]),
        )
        .await;
        let (ns, admin) = (sandbox.namespace().to_owned(), sandbox.admin().clone());
        // The Deployment as Helm would apply it, without Helm's labels: only managedFields say
        // that `helm` owns HELM_VAR.
        let deployments = Api::<kube::api::DynamicObject>::namespaced_with(
            admin.clone(),
            &ns,
            &kube::discovery::ApiResource::from_gvk(&kube::core::GroupVersionKind::gvk(
                "apps",
                "v1",
                "Deployment",
            )),
        );
        deployments
            .patch(
                "web",
                &PatchParams::apply("helm"),
                &Patch::Apply(json!({"apiVersion": "apps/v1", "kind": "Deployment",
                    "metadata": {"name": "web", "namespace": ns},
                    "spec": {"replicas": 0, "selector": {"matchLabels": {"app": "web"}},
                             "template": {"metadata": {"labels": {"app": "web"}},
                                          "spec": {"containers": [{"name": "web", "image": "busybox:1.37",
                                              "env": [{"name": "HELM_VAR", "value": "from-helm"}]}]}}}})),
            )
            .await
            .unwrap();
        create(
            &Api::<kube::api::DynamicObject>::namespaced_with(
                admin.clone(),
                &ns,
                &kube::discovery::ApiResource::from_gvk(&kube::core::GroupVersionKind::gvk(
                    "",
                    "v1",
                    "ConfigMap",
                )),
            ),
            json!({"apiVersion": "v1", "kind": "ConfigMap",
                   "metadata": {"name": "settings", "namespace": ns}, "data": {"MODE": "a"}}),
        )
        .await;

        let person = sandbox.user().clone();
        let app = app(&person);
        let plan_uri = format!("/api/v1/projects/{ns}/plan");
        let apply_uri = format!("/api/v1/projects/{ns}/apply");

        // A field Helm owns is not taken over.
        let (status, problem) = post(&app, &plan_uri, &set_env("HELM_VAR", "mine")).await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");

        // A new variable: plan, apply, and it lands as `zeabur`, Helm's field untouched.
        let (status, plan) = post(&app, &plan_uri, &set_env("FROM_ZEABUR", "yes")).await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        let (status, applied) = post(&app, &apply_uri, &json!({"token": plan["token"]})).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let web = deployments.get("web").await.unwrap();
        let env = web
            .data
            .pointer("/spec/template/spec/containers/0/env")
            .unwrap();
        assert_eq!(
            env,
            &json!([{"name": "HELM_VAR", "value": "from-helm"}, {"name": "FROM_ZEABUR", "value": "yes"}])
        );
        let managers: Vec<&str> = web
            .metadata
            .managed_fields
            .iter()
            .flatten()
            .filter_map(|m| m.manager.as_deref())
            .collect();
        assert!(
            managers.contains(&"helm") && managers.contains(&"zeabur"),
            "{managers:?}"
        );
        let helm_fields = web
            .metadata
            .managed_fields
            .iter()
            .flatten()
            .find(|m| m.manager.as_deref() == Some("helm"))
            .and_then(|m| m.fields_v1.as_ref())
            .map(|f| f.0.to_string())
            .unwrap();
        assert!(
            helm_fields.contains("HELM_VAR"),
            "helm still owns its variable: {helm_fields}"
        );

        // Of 64 concurrent applies of one plan, exactly one succeeds.
        let (status, plan) = post(&app, &plan_uri, &set_env("RACE", "won")).await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        let race = json!({"token": plan["token"]});
        let racers: Vec<_> = (0..64)
            .map(|_| {
                let app = app.clone();
                let race = race.clone();
                let uri = apply_uri.clone();
                tokio::spawn(async move { post(&app, &uri, &race).await.0 })
            })
            .collect();
        let mut statuses = Vec::new();
        for apply in racers {
            statuses.push(apply.await.unwrap());
        }
        assert_eq!(
            statuses.iter().filter(|s| **s == StatusCode::OK).count(),
            1,
            "{statuses:?}"
        );
        // A loser is told the object moved on (409, whether kube-apiserver answered 409 or a
        // failed JSON Patch test, 422). 429 is allowed too: kube-apiserver's API Priority and
        // Fairness may turn some of 64 simultaneous requests from one person away, and Zeabur
        // passes that answer on with its Retry-After rather than guessing an outcome.
        assert!(
            statuses.iter().all(|s| matches!(
                *s,
                StatusCode::OK | StatusCode::CONFLICT | StatusCode::TOO_MANY_REQUESTS
            )),
            "{statuses:?}"
        );

        // The role has no `patch` on ConfigMaps: the plan is made, the dry run is refused, and
        // nothing is written.
        let change = json!({"changes": [{"op": "data", "kind": "ConfigMap", "name": "settings",
            "changes": [{"op": "set", "key": "MODE", "value": "b"}]}]});
        let (status, plan) = post(&app, &plan_uri, &change).await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        let (status, problem) = post(&app, &apply_uri, &json!({"token": plan["token"]})).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(
            problem["denied"],
            json!({"verb": "patch", "group": "", "resource": "configmaps",
                   "name": "settings", "namespace": ns}),
            "{problem}"
        );

        // A restart sets the annotation kubectl sets.
        let restart = json!({"changes": [{"op": "restart", "workload": {"kind": "Deployment", "name": "web"}}]});
        let (status, plan) = post(&app, &plan_uri, &restart).await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        let (status, applied) = post(&app, &apply_uri, &json!({"token": plan["token"]})).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let web = deployments.get("web").await.unwrap();
        assert!(
            web.data
                .pointer("/spec/template/metadata/annotations/kubectl.kubernetes.io~1restartedAt")
                .is_some()
        );
    }

    /// Releases and the second set of writes as the person: the releases a Deployment's
    /// ReplicaSets hold, a rollback, a claim that cannot grow on k3s's `local-path` class, a new
    /// volume, a probe with scaling and resources, a YAML edit of a NetworkPolicy, and a Secret
    /// edited as YAML that keeps its masked values.
    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    #[expect(clippy::too_many_lines, reason = "one scenario, read top to bottom")]
    async fn releases_rollbacks_and_settings_as_the_person() {
        use kube::api::DynamicObject;
        use kube::core::GroupVersionKind;
        use kube::discovery::ApiResource;

        // Reads and patches in the namespace; nothing cluster-wide (StorageClasses stay unread).
        let sandbox = Sandbox::new(
            "zeabur-settings",
            json!([
                {"apiGroups": ["", "apps", "batch", "networking.k8s.io", "events.k8s.io"],
                 "resources": ["*"], "verbs": ["get", "list", "patch", "create"]}
            ]),
        )
        .await;
        let (ns, admin) = (sandbox.namespace().to_owned(), sandbox.admin().clone());
        let api = |group: &str, version: &str, kind: &str| {
            Api::<DynamicObject>::namespaced_with(
                admin.clone(),
                &ns,
                &ApiResource::from_gvk(&GroupVersionKind::gvk(group, version, kind)),
            )
        };
        let deployments = api("apps", "v1", "Deployment");
        let web = |image: &str| {
            json!({"apiVersion": "apps/v1", "kind": "Deployment", "metadata": {"name": "web", "namespace": ns},
                "spec": {"replicas": 1, "selector": {"matchLabels": {"app": "web"}},
                    "template": {"metadata": {"labels": {"app": "web"}},
                        "spec": {"containers": [{"name": "web", "image": image,
                            "command": ["sleep", "3600"], "ports": [{"containerPort": 8080}]}]}}}})
        };
        create(&deployments, web("busybox:1.37")).await;
        deployments
            .patch("web", &kube::api::PatchParams::default(), &kube::api::Patch::Merge(json!({
                "spec": {"template": {"spec": {"containers": [{"name": "web", "image": "busybox:1.36"}]}}}})))
            .await
            .unwrap();
        create(
            &api("networking.k8s.io", "v1", "NetworkPolicy"),
            json!({"apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
                   "metadata": {"name": "web", "namespace": ns},
                   "spec": {"podSelector": {"matchLabels": {"app": "web"}}, "policyTypes": ["Ingress"]}}),
        )
        .await;
        create(
            &api("", "v1", "Secret"),
            json!({"apiVersion": "v1", "kind": "Secret", "metadata": {"name": "web", "namespace": ns},
                   "stringData": {"password": "hunter2"}}),
        )
        .await;
        create(
            &api("", "v1", "PersistentVolumeClaim"),
            json!({"apiVersion": "v1", "kind": "PersistentVolumeClaim", "metadata": {"name": "uploads", "namespace": ns},
                   "spec": {"accessModes": ["ReadWriteOnce"], "resources": {"requests": {"storage": "1Gi"}}}}),
        )
        .await;

        let person = sandbox.user().clone();
        let app = app(&person);
        let workload = json!({"kind": "Deployment", "name": "web"});

        // Two releases, once the controller has made the second ReplicaSet and moved the
        // Deployment's revision annotation to it; it writes the ReplicaSet first.
        let releases_uri = format!("/api/v1/projects/{ns}/services/deployment/web/releases");
        let mut releases = Value::Null;
        for _ in 0..60 {
            let (status, body) = call(&app, "GET", &releases_uri).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            releases = serde_json::from_str(&body).unwrap();
            if releases["items"].as_array().is_some_and(|i| i.len() == 2)
                && releases["items"][0]["current"] == true
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        assert_eq!(
            releases["items"][0]["images"],
            json!(["busybox:1.36"]),
            "{releases}"
        );
        assert_eq!(releases["items"][0]["current"], true, "{releases}");
        let old = releases["items"][1]["revision"].as_i64().unwrap();

        // Rollback: the template's image goes back.
        let (status, body) = plan_and_apply(
            &app,
            &ns,
            json!([{"op": "rollback", "workload": workload, "revision": old}]),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let image = deployments.get("web").await.unwrap();
        assert_eq!(
            image.data.pointer("/spec/template/spec/containers/0/image"),
            Some(&json!("busybox:1.37"))
        );

        // local-path cannot grow its claims: the dry run refuses, nothing is written.
        let (status, plan) = post(
            &app,
            &format!("/api/v1/projects/{ns}/plan"),
            &json!({"changes": [{"op": "expandVolume", "claim": "uploads", "size": "2Gi"}]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        assert!(
            plan["warnings"].to_string().contains("storageClassUnknown"),
            "{plan}"
        );
        let (status, problem) = post(
            &app,
            &format!("/api/v1/projects/{ns}/apply"),
            &json!({"token": plan["token"]}),
        )
        .await;
        assert!(status.is_client_error(), "{status} {problem}");
        let claim = api("", "v1", "PersistentVolumeClaim")
            .get("uploads")
            .await
            .unwrap();
        assert_eq!(
            claim.data.pointer("/spec/resources/requests/storage"),
            Some(&json!("1Gi"))
        );

        // A new volume: its claim, then the mount.
        let (status, body) = plan_and_apply(
            &app,
            &ns,
            json!([{"op": "addVolume", "workload": workload,
            "volume": {"name": "cache", "size": "1Gi", "mountPath": "/cache"}}]),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(
            api("", "v1", "PersistentVolumeClaim")
                .get("web-cache")
                .await
                .is_ok()
        );
        let mounted = deployments.get("web").await.unwrap();
        assert_eq!(
            mounted
                .data
                .pointer("/spec/template/spec/containers/0/volumeMounts/0/mountPath"),
            Some(&json!("/cache"))
        );

        // A probe, a replica count and resources, in one write.
        let (status, body) = plan_and_apply(&app, &ns, json!([
            {"op": "probe", "workload": workload, "probe": "readiness",
             "set": {"handler": "TCPSocket", "port": 8080}},
            {"op": "scale", "workload": workload, "replicas": 2},
            {"op": "resources", "workload": workload, "requests": {"cpu": "10m", "memory": "16Mi"},
             "limits": {"memory": "64Mi"}}
        ])).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let set = deployments.get("web").await.unwrap();
        assert_eq!(set.data.pointer("/spec/replicas"), Some(&json!(2)));
        assert_eq!(
            set.data
                .pointer("/spec/template/spec/containers/0/readinessProbe/tcpSocket/port"),
            Some(&json!(8080))
        );
        assert_eq!(
            set.data
                .pointer("/spec/template/spec/containers/0/resources/limits/memory"),
            Some(&json!("64Mi"))
        );

        // A YAML edit of a kind Zeabur does not model.
        let yaml = "apiVersion: networking.k8s.io/v1\nkind: NetworkPolicy\nmetadata:\n  name: web\nspec:\n  podSelector:\n    matchLabels: {app: web}\n  policyTypes: [Ingress, Egress]\n";
        let (status, body) = plan_and_apply(&app, &ns, json!([{"op": "edit",
            "resource": {"group": "networking.k8s.io", "version": "v1", "resource": "networkpolicies"},
            "name": "web", "yaml": yaml}])).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let policy = api("networking.k8s.io", "v1", "NetworkPolicy")
            .get("web")
            .await
            .unwrap();
        assert_eq!(
            policy.data.pointer("/spec/policyTypes"),
            Some(&json!(["Ingress", "Egress"]))
        );

        // A Secret edited as YAML: the masked value stays, the new key lands.
        let yaml = "apiVersion: v1\nkind: Secret\nmetadata: {name: web}\ntype: Opaque\ndata:\n  password: \u{ab}redacted\u{bb}\nstringData:\n  token: abc\n";
        let (status, body) = plan_and_apply(&app, &ns, json!([{"op": "edit",
            "resource": {"group": "", "version": "v1", "resource": "secrets"}, "name": "web", "yaml": yaml}])).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let secret = api("", "v1", "Secret").get("web").await.unwrap();
        assert_eq!(
            secret.data.pointer("/data/password"),
            Some(&json!("aHVudGVyMg=="))
        );
        assert_eq!(secret.data.pointer("/data/token"), Some(&json!("YWJj")));
    }

    /// A namespace whose person has the Role `rules`, the API as that person, and the admin
    /// connection, which only prepares objects and reads them back.
    async fn project_as(prefix: &str, rules: Value) -> (Sandbox, Router) {
        let sandbox = Sandbox::new(prefix, rules).await;
        let app = app(sandbox.user());
        (sandbox, app)
    }

    /// A name that climbs out of its path segment never reaches kube-apiserver: asked for the
    /// ConfigMap `../secrets/<name>`, kube's client would put the name into the path as it is,
    /// and the request would address the Secret, whose values the object route then shows only
    /// because it believes it read a ConfigMap. The name is refused as not found instead.
    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn a_name_cannot_climb_to_another_kind() {
        let (sandbox, app) = project_as(
            "zeabur-names",
            json!([{"apiGroups": [""], "resources": ["secrets", "configmaps"],
                    "verbs": ["get", "list"]}]),
        )
        .await;
        let (ns, admin) = (sandbox.namespace().to_owned(), sandbox.admin().clone());
        create(
            &objects(&admin, &ns, "", "Secret"),
            json!({"apiVersion": "v1", "kind": "Secret", "metadata": {"name": "db"},
                   "stringData": {"password": SECRET_VALUE}}),
        )
        .await;
        for name in [
            "..%2Fsecrets%2Fdb",
            "%2E%2E%2Fsecrets%2Fdb",
            "x%2F..%2F..%2Fsecrets%2Fdb",
        ] {
            let (status, body) = call(
                &app,
                "GET",
                &format!("/api/v1/projects/{ns}/objects/core/v1/configmaps/{name}"),
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{name}: {body}");
            assert!(!body.contains(SECRET_VALUE), "{name}: {body}");
            // Not even the key, nor its value base64-encoded as a Secret stores it.
            assert!(!body.contains("password"), "{name}: {body}");
        }
        // The Secret itself is readable through its own route, with its values hidden.
        let (status, body) = call(
            &app,
            "GET",
            &format!("/api/v1/projects/{ns}/objects/core/v1/secrets/db"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(!body.contains(SECRET_VALUE), "{body}");
    }

    /// An admin's handle on one kind of object in `ns`.
    fn objects(admin: &Client, ns: &str, group: &str, kind: &str) -> Api<kube::api::DynamicObject> {
        let resource = kube::discovery::ApiResource::from_gvk(&kube::core::GroupVersionKind::gvk(
            group, "v1", kind,
        ));
        Api::namespaced_with(admin.clone(), ns, &resource)
    }

    /// Editing one key of a Secret whose values the person cannot read keeps its other keys and
    /// their values; so does editing one key of a ConfigMap. Losing them was a bug once: without
    /// the values, the plan must add the key alone, never the whole `data` map. The Secret's
    /// labels edited as YAML need no values either, and a change of its type is refused by
    /// kube-apiserver, not by Zeabur.
    ///
    /// Kubernetes has no right to see a Secret's keys without its values: `list` and `watch`
    /// return the values as `get` does [k8s-rbac-secrets]. Zeabur reads values only with `get`
    /// (when a person reveals them or plans a Secret edit) and lists Secrets as a table without
    /// them, so a Role with `list` and `patch` but not `get` is the person who plans such an edit
    /// without the values.
    ///
    /// [k8s-rbac-secrets]: https://kubernetes.io/docs/concepts/security/rbac-good-practices/#listing-secrets
    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn editing_one_key_keeps_the_others_without_reading_the_values() {
        let (sandbox, app) = project_as(
            "zeabur-keys",
            json!([{"apiGroups": [""], "resources": ["secrets", "configmaps"],
                    "verbs": ["list", "patch"]}]),
        )
        .await;
        let (ns, admin) = (sandbox.namespace().to_owned(), sandbox.admin().clone());
        let secrets = objects(&admin, &ns, "", "Secret");
        create(
            &secrets,
            json!({"apiVersion": "v1", "kind": "Secret", "metadata": {"name": "db", "namespace": ns},
                   "stringData": {"password": "hunter2", "user": "admin"}}),
        )
        .await;
        let config_maps = objects(&admin, &ns, "", "ConfigMap");
        create(
            &config_maps,
            json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "settings", "namespace": ns},
                   "data": {"MODE": "a", "LEVEL": "3"}}),
        )
        .await;

        let edit = |kind: &str, name: &str, key: &str| {
            json!({"changes": [{"op": "data", "kind": kind, "name": name,
                "changes": [{"op": "set", "key": key, "value": "new"}]}]})
        };
        let (status, plan) = post(
            &app,
            &format!("/api/v1/projects/{ns}/plan"),
            &edit("Secret", "db", "password"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        assert!(
            plan["warnings"].to_string().contains("secretValuesUnknown"),
            "the plan could not read the values: {plan}"
        );
        let (status, body) = post(
            &app,
            &format!("/api/v1/projects/{ns}/apply"),
            &json!({"token": plan["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        // A new key, the same way.
        let (status, body) =
            plan_and_apply(&app, &ns, edit("Secret", "db", "token")["changes"].clone()).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let secret = secrets.get("db").await.unwrap();
        assert_eq!(
            secret.data["data"],
            json!({"password": "bmV3", "user": "YWRtaW4=", "token": "bmV3"})
        );

        let (status, body) = plan_and_apply(
            &app,
            &ns,
            edit("ConfigMap", "settings", "MODE")["changes"].clone(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let settings = config_maps.get("settings").await.unwrap();
        assert_eq!(settings.data["data"], json!({"MODE": "new", "LEVEL": "3"}));

        // The Secret's YAML, its values masked as the YAML view shows them: a label lands and
        // no value is written.
        let yaml = |kind: &str| {
            format!(
                "apiVersion: v1\nkind: Secret\nmetadata: {{name: db, labels: {{tier: data}}}}\n\
                 type: {kind}\ndata:\n  password: (hidden)\n  user: (hidden)\n  token: (hidden)\n"
            )
        };
        let edit = |kind: &str| {
            json!([{"op": "edit", "resource": {"group": "", "version": "v1", "resource": "secrets"},
                    "name": "db", "yaml": yaml(kind)}])
        };
        let (status, body) = plan_and_apply(&app, &ns, edit("Opaque")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let secret = secrets.get("db").await.unwrap();
        assert_eq!(
            secret
                .metadata
                .labels
                .as_ref()
                .and_then(|labels| labels.get("tier"))
                .map(String::as_str),
            Some("data")
        );
        assert_eq!(
            secret.data["data"],
            json!({"password": "bmV3", "user": "YWRtaW4=", "token": "bmV3"})
        );
        // A Secret's type cannot change [k8s-secret-types]: the plan has it, kube-apiserver's
        // dry run refuses it, and its answer names the field.
        //
        // [k8s-secret-types]: https://kubernetes.io/docs/concepts/configuration/secret/#secret-types
        let (status, body) = plan_and_apply(&app, &ns, edit("example.com/token")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(body["kubernetes"]["code"], json!(422), "{body}");
        assert!(
            body["kubernetes"]["message"]
                .as_str()
                .is_some_and(|m| m.contains("type")),
            "{body}"
        );
        assert_eq!(
            secrets.get("db").await.unwrap().data["type"],
            json!("Opaque")
        );
    }

    /// The object as the person sees it in a plan or on its page
    /// ([`zeabur_graph::manifest::clean`]), without the revision the Deployment controller
    /// notes after a template change.
    fn as_shown(object: &Value) -> Value {
        let mut shown = zeabur_graph::manifest::clean(object);
        if let Some(annotations) = shown
            .pointer_mut("/metadata/annotations")
            .and_then(Value::as_object_mut)
        {
            annotations.remove("deployment.kubernetes.io/revision");
            if annotations.is_empty()
                && let Some(metadata) = shown.get_mut("metadata").and_then(Value::as_object_mut)
            {
                metadata.remove("annotations");
            }
        }
        shown
    }

    /// The diff the person approves is the write that happens: for each change kind, the plan's
    /// `after` is the object kube-apiserver stores, apart from what the server sets itself. That
    /// is the server's metadata and status, and the defaults kube-apiserver fills in for fields a
    /// new probe leaves out [k8s-probe].
    ///
    /// [k8s-probe]: https://kubernetes.io/docs/reference/kubernetes-api/workload-resources/pod-v1/#Probe
    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn the_object_stored_is_the_plans_after() {
        let (sandbox, app) = project_as(
            "zeabur-after",
            json!([{"apiGroups": ["", "apps", "batch", "networking.k8s.io", "events.k8s.io"],
                    "resources": ["*"], "verbs": ["get", "list", "patch"]}]),
        )
        .await;
        let (ns, admin) = (sandbox.namespace().to_owned(), sandbox.admin().clone());
        let deployments = objects(&admin, &ns, "apps", "Deployment");
        create(
            &deployments,
            json!({"apiVersion": "apps/v1", "kind": "Deployment", "metadata": {"name": "web", "namespace": ns},
                "spec": {"replicas": 0, "selector": {"matchLabels": {"app": "web"}},
                    "template": {"metadata": {"labels": {"app": "web"}},
                        "spec": {"containers": [{"name": "web", "image": "busybox:1.37",
                            "ports": [{"containerPort": 8080}],
                            "env": [{"name": "MODE", "value": "fast"}]}]}}}}),
        )
        .await;
        let workload = json!({"kind": "Deployment", "name": "web"});
        let container = "/spec/template/spec/containers/0";
        let stored = async || serde_json::to_value(deployments.get("web").await.unwrap()).unwrap();
        let changes = [
            json!({"op": "env", "workload": workload,
                "changes": [{"op": "set", "name": "LEVEL", "value": "3"}]}),
            json!({"op": "probe", "workload": workload, "probe": "readiness",
                "set": {"handler": "TCPSocket", "port": 8080}}),
            json!({"op": "resources", "workload": workload,
                "requests": {"cpu": "10m", "memory": "16Mi"}, "limits": {"memory": "64Mi"}}),
        ];
        for change in changes {
            let plan = planned_and_applied(&app, &ns, json!([change])).await;
            let mut after = plan["objects"][0]["after"].clone();
            if change["op"] == "probe" {
                let probe = after
                    .pointer_mut(&format!("{container}/readinessProbe"))
                    .and_then(Value::as_object_mut)
                    .unwrap();
                for (field, default) in [
                    ("timeoutSeconds", 1),
                    ("periodSeconds", 10),
                    ("successThreshold", 1),
                    ("failureThreshold", 3),
                ] {
                    probe.entry(field).or_insert(json!(default));
                }
            }
            assert_eq!(as_shown(&stored().await), as_shown(&after), "{change}");
        }

        // A YAML edit that inserts a variable before the one the list had (a keyed list), made
        // from the object as its page shows it. After each template change the Deployment
        // controller notes the new revision in the Deployment's annotations through the status
        // subresource, which raises neither `generation` nor anything a plan pins; an edit made
        // before that would plan to set the annotation back, and its `test` of it would fail
        // once the controller writes. So the edit starts once the controller has caught up
        // (`status.observedGeneration`, set after the annotation in the same sync [k8s-sync]).
        //
        // [k8s-sync]: https://github.com/kubernetes/kubernetes/blob/master/pkg/controller/deployment/sync.go
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let settled = loop {
            let object = stored().await;
            if object.pointer("/status/observedGeneration")
                == object.pointer("/metadata/generation")
            {
                break object;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the Deployment controller did not catch up; last seen {object}"
            );
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        };
        let mut edited = zeabur_graph::manifest::clean(&settled);
        edited
            .pointer_mut(&format!("{container}/env"))
            .and_then(Value::as_array_mut)
            .unwrap()
            .insert(0, json!({"name": "FIRST", "value": "1"}));
        let edit = json!([{"op": "edit", "name": "web", "yaml": edited.to_string(),
            "resource": {"group": "apps", "version": "v1", "resource": "deployments"}}]);
        let plan = planned_and_applied(&app, &ns, edit).await;
        let after = stored().await;
        assert_eq!(as_shown(&after), as_shown(&plan["objects"][0]["after"]));
        assert_eq!(
            after.pointer(&format!("{container}/env/0/name")),
            Some(&json!("FIRST"))
        );
    }

    /// Deletions as the person: a ready-made NetworkPolicy created and then undone at the
    /// `resourceVersion` its creation returned is gone; a deletion planned before someone else
    /// changed the object is refused with `409` by its precondition; a deletion the person's role
    /// does not allow is refused by the dry run, naming the `delete`, and deletes nothing.
    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn deletions_keep_to_their_preconditions_and_the_persons_rights() {
        let (sandbox, app) = project_as(
            "zeabur-delete",
            json!([
                {"apiGroups": ["", "apps", "batch", "networking.k8s.io"],
                 "resources": ["*"], "verbs": ["get", "list"]},
                {"apiGroups": ["networking.k8s.io"], "resources": ["networkpolicies"],
                 "verbs": ["create", "delete"]}
            ]),
        )
        .await;
        let (ns, admin) = (sandbox.namespace().to_owned(), sandbox.admin().clone());
        let policies = objects(&admin, &ns, "networking.k8s.io", "NetworkPolicy");
        let resource =
            json!({"group": "networking.k8s.io", "version": "v1", "resource": "networkpolicies"});
        let preset = json!({"apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
            "metadata": {"name": "zeabur-isolate-web"},
            "spec": {"podSelector": {"matchLabels": {"app": "web"}}, "policyTypes": ["Ingress"]}});

        // Create, then undo at the version the creation returned.
        let (status, applied) =
            plan_and_apply(&app, &ns, json!([{"op": "create", "object": preset}])).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let version = applied["applied"][0]["resourceVersion"].clone();
        let (status, body) = plan_and_apply(
            &app,
            &ns,
            json!([{"op": "delete", "resource": resource, "name": "zeabur-isolate-web",
                    "resourceVersion": version}]),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(
            policies
                .get_opt("zeabur-isolate-web")
                .await
                .unwrap()
                .is_none()
        );

        // A deletion planned, then the object changed by someone else: refused, kept.
        let (status, applied) =
            plan_and_apply(&app, &ns, json!([{"op": "create", "object": preset}])).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let (status, plan) = post(
            &app,
            &format!("/api/v1/projects/{ns}/plan"),
            &json!({"changes": [{"op": "delete", "resource": resource,
                                 "name": "zeabur-isolate-web"}]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        policies
            .patch(
                "zeabur-isolate-web",
                &kube::api::PatchParams::default(),
                &kube::api::Patch::Merge(json!({"metadata": {"labels": {"team": "web"}}})),
            )
            .await
            .unwrap();
        let (status, problem) = post(
            &app,
            &format!("/api/v1/projects/{ns}/apply"),
            &json!({"token": plan["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
        assert!(
            policies
                .get_opt("zeabur-isolate-web")
                .await
                .unwrap()
                .is_some()
        );

        // The role has no `delete` on ConfigMaps: planned, refused by the dry run, kept.
        let config_maps = objects(&admin, &ns, "", "ConfigMap");
        create(
            &config_maps,
            json!({"apiVersion": "v1", "kind": "ConfigMap",
                   "metadata": {"name": "settings", "namespace": ns}, "data": {"MODE": "a"}}),
        )
        .await;
        let (status, plan) = post(
            &app,
            &format!("/api/v1/projects/{ns}/plan"),
            &json!({"changes": [{"op": "delete",
                "resource": {"group": "", "version": "v1", "resource": "configmaps"},
                "name": "settings"}]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        let (status, problem) = post(
            &app,
            &format!("/api/v1/projects/{ns}/apply"),
            &json!({"token": plan["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(
            problem["denied"],
            json!({"verb": "delete", "group": "", "resource": "configmaps",
                   "name": "settings", "namespace": ns}),
            "{problem}"
        );
        assert!(config_maps.get_opt("settings").await.unwrap().is_some());
    }

    /// The Namespace as the admin reads it.
    async fn namespace_labels(
        admin: &Client,
        ns: &str,
    ) -> std::collections::BTreeMap<String, String> {
        Api::<Namespace>::all(admin.clone())
            .get(ns)
            .await
            .unwrap()
            .metadata
            .labels
            .unwrap_or_default()
    }

    /// Adopting a namespace and letting it go, as a person whose Role names the Namespace itself.
    /// kube-apiserver authorises a request for `namespaces/{name}` in namespace `name`
    /// ([requestinfo]), so a Role in the namespace limited by `resourceNames` is enough. Without
    /// `patch` the person sees `canChange: denied` and the apply's dry run is refused with the
    /// request named; with it, the plan adds and then removes the one label, and the label
    /// kube-apiserver sets on every namespace (`kubernetes.io/metadata.name`) stays.
    ///
    /// [requestinfo]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/endpoints/request/requestinfo.go
    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn adopting_and_letting_go_patch_only_the_project_label() {
        let read = |ns: &str| {
            json!([{"apiGroups": [""], "resources": ["namespaces"],
            "resourceNames": [ns], "verbs": ["get"]}])
        };
        let (sandbox, app) = project_as("zeabur-adopt", json!([])).await;
        let (ns, admin) = (sandbox.namespace().to_owned(), sandbox.admin().clone());
        let roles = Api::<Role>::namespaced(admin.clone(), &ns);
        let set_rules = |rules: Value| {
            let roles = roles.clone();
            async move {
                roles
                    .patch(
                        PERSON,
                        &kube::api::PatchParams::default(),
                        &kube::api::Patch::Merge(json!({"rules": rules})),
                    )
                    .await
                    .unwrap();
            }
        };
        set_rules(read(&ns)).await;
        let namespace_uri = format!("/api/v1/projects/{ns}/namespace");
        // RBAC follows a Role's change within moments; wait for the answer wanted.
        let can_change = |wanted: &'static str| {
            let app = app.clone();
            let uri = namespace_uri.clone();
            async move {
                for _ in 0..50 {
                    let (status, body) = call(&app, "GET", &uri).await;
                    let body: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                    if status == StatusCode::OK && body["canChange"] == wanted {
                        return body;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
                panic!("canChange never became {wanted}");
            }
        };
        let namespace = can_change("denied").await;
        assert_eq!(namespace["adopted"], false);

        let (status, plan) = post(
            &app,
            &format!("/api/v1/projects/{ns}/plan"),
            &json!({"changes": [{"op": "adopt"}]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        let (status, problem) = post(
            &app,
            &format!("/api/v1/projects/{ns}/apply"),
            &json!({"token": plan["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(
            problem["denied"],
            json!({"verb": "patch", "group": "", "resource": "namespaces", "name": ns})
        );

        let mut patching = read(&ns);
        patching[0]["verbs"] = json!(["get", "patch"]);
        set_rules(patching).await;
        can_change("allowed").await;
        let before = namespace_labels(&admin, &ns).await;
        planned_and_applied(&app, &ns, json!([{"op": "adopt"}])).await;
        let mut expected = before.clone();
        expected.insert("zeabur.com/project".to_owned(), "true".to_owned());
        assert_eq!(namespace_labels(&admin, &ns).await, expected);

        planned_and_applied(
            &app,
            &ns,
            json!([{"op": "details", "displayName": "Adopted", "icon": "rocket"}]),
        )
        .await;
        planned_and_applied(&app, &ns, json!([{"op": "unadopt"}])).await;
        assert_eq!(namespace_labels(&admin, &ns).await, before);
        let annotations = Api::<Namespace>::all(admin.clone())
            .get(&ns)
            .await
            .unwrap()
            .metadata
            .annotations
            .unwrap_or_default();
        assert_eq!(annotations["zeabur.com/display-name"], "Adopted");
    }

    /// A `ResourceQuota` as the person meets it: a creation over it is refused with `403`, which
    /// the API tells from missing rights and answers as `quota-exceeded` with the quota's
    /// numbers; a Deployment whose second pod it refuses carries `ReplicaFailure`, which the
    /// project document diagnoses as `quotaBlocked`.
    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn a_quota_refuses_a_write_with_its_numbers_and_blocks_a_deployments_pods() {
        let (sandbox, app) = project_as(
            "zeabur-quota",
            json!([
                {"apiGroups": ["", "apps", "batch", "networking.k8s.io", "events.k8s.io"],
                 "resources": ["*"], "verbs": ["get", "list"]},
                {"apiGroups": ["networking.k8s.io"], "resources": ["networkpolicies"],
                 "verbs": ["create"]}
            ]),
        )
        .await;
        let (ns, admin) = (sandbox.namespace().to_owned(), sandbox.admin().clone());
        let quotas = objects(&admin, &ns, "", "ResourceQuota");
        create(
            &quotas,
            json!({"apiVersion": "v1", "kind": "ResourceQuota", "metadata": {"name": "limits"},
                   "spec": {"hard": {"pods": "1",
                                     "count/networkpolicies.networking.k8s.io": "0"}}}),
        )
        .await;
        // Quota admission refuses everything until the quota controller has filled the status.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while quotas
            .get("limits")
            .await
            .unwrap()
            .data
            .pointer("/status/used")
            .is_none()
        {
            assert!(std::time::Instant::now() < deadline, "no quota status");
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        let preset = json!({"apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
            "metadata": {"name": "zeabur-isolate-web"},
            "spec": {"podSelector": {"matchLabels": {"app": "web"}}, "policyTypes": ["Ingress"]}});
        let (status, problem) =
            plan_and_apply(&app, &ns, json!([{"op": "create", "object": preset}])).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(problem["kind"], "quota-exceeded", "{problem}");
        assert_eq!(
            problem["why"]["params"],
            json!({"quota": "limits", "resource": "count/networkpolicies.networking.k8s.io",
                   "hard": "0", "used": "0"})
        );

        create(
            &Api::<zeabur_kube::k8s_openapi::api::apps::v1::Deployment>::namespaced(
                admin.clone(),
                &ns,
            ),
            json!({"metadata": {"name": "blocked"},
                   "spec": {"replicas": 2, "selector": {"matchLabels": {"app": "blocked"}},
                            "template": {"metadata": {"labels": {"app": "blocked"}},
                                         "spec": {"containers": [{"name": "app",
                                             "image": "busybox:1.37",
                                             "command": ["sleep", "3600"]}]}}}}),
        )
        .await;
        let document = until(&app, &format!("/api/v1/projects/{ns}"), 120, |document| {
            document["services"].as_array().is_some_and(|services| {
                services.iter().any(|s| {
                    s["name"] == "blocked"
                        && s["diagnoses"]
                            .as_array()
                            .is_some_and(|d| d.iter().any(|d| d["kind"] == "quotaBlocked"))
                })
            })
        })
        .await;
        let blocked = document["services"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "blocked")
            .unwrap();
        let diagnosis = blocked["diagnoses"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["kind"] == "quotaBlocked")
            .unwrap();
        assert_eq!(diagnosis["cause"]["params"]["quota"], "limits");
    }

    /// A pod mounting a Secret that does not exist waits in `ContainerCreating` with no reason
    /// of its own; the kubelet's `FailedMount` event is diagnosed as `configMissing`, naming the
    /// volume and the Secret.
    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn a_volume_from_a_missing_secret_is_diagnosed_as_config_missing() {
        let (sandbox, app) = project_as(
            "zeabur-volume",
            json!([
                {"apiGroups": ["", "apps", "batch", "networking.k8s.io", "events.k8s.io"],
                 "resources": ["*"], "verbs": ["get", "list"]}
            ]),
        )
        .await;
        let (ns, admin) = (sandbox.namespace().to_owned(), sandbox.admin().clone());
        create(
            &Api::<zeabur_kube::k8s_openapi::api::apps::v1::Deployment>::namespaced(
                admin.clone(),
                &ns,
            ),
            json!({"metadata": {"name": "mounts"},
                   "spec": {"replicas": 1, "selector": {"matchLabels": {"app": "mounts"}},
                            "template": {"metadata": {"labels": {"app": "mounts"}},
                                         "spec": {"containers": [{"name": "app",
                                             "image": "busybox:1.37",
                                             "command": ["sleep", "3600"],
                                             "volumeMounts": [{"name": "v", "mountPath": "/v"}]}],
                                                  "volumes": [{"name": "v",
                                             "secret": {"secretName": "no-secret"}}]}}}}),
        )
        .await;
        let is_volume = |d: &Value| d["cause"]["key"] == "diagnosis.configMissing.cause.volume";
        let document = until(&app, &format!("/api/v1/projects/{ns}"), 120, |document| {
            document["services"].as_array().is_some_and(|services| {
                services.iter().any(|s| {
                    s["name"] == "mounts"
                        && s["diagnoses"]
                            .as_array()
                            .is_some_and(|d| d.iter().any(is_volume))
                })
            })
        })
        .await;
        let mounts = document["services"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "mounts")
            .unwrap();
        let diagnosis = mounts["diagnoses"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| is_volume(d))
            .unwrap();
        assert_eq!(diagnosis["kind"], "configMissing");
        assert_eq!(
            diagnosis["cause"]["params"],
            json!({"container": "app", "volume": "v", "object": "Secret", "name": "no-secret"})
        );
    }

    /// Asks `uri` until `done` holds of its answer, for at most `seconds`.
    async fn until(app: &Router, uri: &str, seconds: u64, done: impl Fn(&Value) -> bool) -> Value {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
        loop {
            let (status, body) = call(app, "GET", uri).await;
            let body: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            if status == StatusCode::OK && done(&body) {
                return body;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{uri} after {seconds} s: {status} {body}"
            );
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    /// Live usage from k3s's own metrics-server, as a person with only Kubernetes' `view` role
    /// in the namespace (which `system:aggregated-metrics-reader` extends with
    /// `pods.metrics.k8s.io`); and history from a VictoriaMetrics single-node store scraping the
    /// kubelets' cAdvisor endpoint, through the service proxy, once the person may `get`
    /// `services/proxy`. The store has no kube-state-metrics, so restarts are unknown, not
    /// zero.
    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    #[expect(clippy::too_many_lines, reason = "one scenario, read top to bottom")]
    async fn usage_and_history_are_read_as_the_person() {
        use zeabur_kube::k8s_openapi::api::rbac::v1::{ClusterRole, ClusterRoleBinding};

        let sandbox = Sandbox::new("zeabur-usage", json!([])).await;
        let (ns, admin) = (sandbox.namespace().to_owned(), sandbox.admin().clone());
        sandbox.delete_cluster_role(&ns);
        let accounts = Api::<ServiceAccount>::namespaced(admin.clone(), &ns);
        create(&accounts, json!({"metadata": {"name": "vm"}})).await;
        let bindings = Api::<RoleBinding>::namespaced(admin.clone(), &ns);
        create(
            &bindings,
            json!({"metadata": {"name": "view"},
                   "roleRef": {"apiGroup": "rbac.authorization.k8s.io", "kind": "ClusterRole", "name": "view"},
                   "subjects": [{"kind": "ServiceAccount", "name": PERSON, "namespace": ns}]}),
        )
        .await;
        // The store discovers the nodes and reads their kubelets' cAdvisor metrics.
        create(
            &Api::<ClusterRole>::all(admin.clone()),
            json!({"metadata": {"name": ns}, "rules": [
                {"apiGroups": [""], "resources": ["nodes"], "verbs": ["get", "list", "watch"]},
                {"apiGroups": [""], "resources": ["nodes/metrics"], "verbs": ["get"]}
            ]}),
        )
        .await;
        create(
            &Api::<ClusterRoleBinding>::all(admin.clone()),
            json!({"metadata": {"name": ns},
                   "roleRef": {"apiGroup": "rbac.authorization.k8s.io", "kind": "ClusterRole", "name": ns},
                   "subjects": [{"kind": "ServiceAccount", "name": "vm", "namespace": ns}]}),
        )
        .await;
        let scrape = "scrape_configs:\n\
            - job_name: cadvisor\n  scrape_interval: 10s\n  scheme: https\n  metrics_path: /metrics/cadvisor\n\
            \x20 tls_config: {insecure_skip_verify: true}\n\
            \x20 bearer_token_file: /var/run/secrets/kubernetes.io/serviceaccount/token\n\
            \x20 kubernetes_sd_configs: [{role: node}]\n";
        create(
            &objects(&admin, &ns, "", "ConfigMap"),
            json!({"apiVersion": "v1", "kind": "ConfigMap",
                   "metadata": {"name": "vm", "namespace": ns}, "data": {"scrape.yaml": scrape}}),
        )
        .await;
        create(
            &objects(&admin, &ns, "apps", "Deployment"),
            json!({"apiVersion": "apps/v1", "kind": "Deployment",
                   "metadata": {"name": "vm", "namespace": ns},
                   "spec": {"selector": {"matchLabels": {"app": "vm"}},
                            "template": {"metadata": {"labels": {"app": "vm"}},
                                         "spec": {"serviceAccountName": "vm",
                                             "containers": [{"name": "vm",
                                                 "image": "victoriametrics/victoria-metrics:v1.153.0",
                                                 "args": ["-promscrape.config=/etc/vm/scrape.yaml",
                                                          "-retentionPeriod=1d",
                                                          "-search.latencyOffset=5s"],
                                                 "ports": [{"containerPort": 8428}],
                                                 "volumeMounts": [{"name": "config", "mountPath": "/etc/vm"}]}],
                                             "volumes": [{"name": "config", "configMap": {"name": "vm"}}]}}}}),
        )
        .await;
        create(
            &objects(&admin, &ns, "", "Service"),
            json!({"apiVersion": "v1", "kind": "Service",
                   "metadata": {"name": "vm", "namespace": ns},
                   "spec": {"selector": {"app": "vm"}, "ports": [{"port": 8428}]}}),
        )
        .await;
        create(
            &objects(&admin, &ns, "apps", "Deployment"),
            json!({"apiVersion": "apps/v1", "kind": "Deployment",
                   "metadata": {"name": "web", "namespace": ns},
                   "spec": {"selector": {"matchLabels": {"app": "web"}},
                            "template": {"metadata": {"labels": {"app": "web"}},
                                         "spec": {"containers": [{"name": "web", "image": "busybox:1.37",
                                             "command": ["sh", "-c", "while true; do :; done"],
                                             "resources": {"requests": {"cpu": "10m", "memory": "16Mi"},
                                                           "limits": {"cpu": "50m", "memory": "32Mi"}}}]}}}}),
        )
        .await;

        let person = sandbox.user().clone();
        let settings = zeabur_api::Settings {
            metrics_store: zeabur_api::settings::parse_service_port(&format!("{ns}/vm:8428"))
                .unwrap(),
            namespace: None,
        };
        let app = app_with(&person, settings);

        // metrics-server leaves a pod out until it has two scrapes of it.
        let usage = until(&app, &format!("/api/v1/projects/{ns}/usage"), 120, |u| {
            u["pods"].as_array().is_some_and(|pods| {
                pods.iter()
                    .any(|p| p["pod"].as_str().unwrap_or_default().starts_with("web-"))
            })
        })
        .await;
        assert_eq!(usage["state"], "available");
        let web = usage["pods"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["pod"].as_str().unwrap().starts_with("web-"))
            .unwrap();
        assert!(web["memoryBytes"].as_u64().unwrap() > 0, "{web}");
        assert_eq!(web["containers"][0]["name"], "web");

        // `view` has no `services/proxy`: the store refuses the person.
        let history_uri = format!("/api/v1/projects/{ns}/services/deployment/web/history");
        let history = until(&app, &history_uri, 30, |h| h["state"] != "unavailable").await;
        assert_eq!(history["state"], "unreadable", "{history}");
        assert_eq!(
            history["unreadable"][0]["resource"],
            format!("services/proxy ({ns}/vm)")
        );

        create(
            &Api::<Role>::namespaced(admin.clone(), &ns),
            json!({"metadata": {"name": "history"}, "rules": [
                {"apiGroups": [""], "resources": ["services/proxy"], "verbs": ["get"]}
            ]}),
        )
        .await;
        create(
            &bindings,
            json!({"metadata": {"name": "history"},
                   "roleRef": {"apiGroup": "rbac.authorization.k8s.io", "kind": "Role", "name": "history"},
                   "subjects": [{"kind": "ServiceAccount", "name": PERSON, "namespace": ns}]}),
        )
        .await;
        let history = until(&app, &history_uri, 240, |h| {
            h["state"] == "available"
                && h["cpu"].as_array().is_some_and(|series| {
                    series.iter().any(|s| {
                        s["values"]
                            .as_array()
                            .is_some_and(|values| values.iter().any(Value::is_f64))
                    })
                })
                && h["memory"]
                    .as_array()
                    .is_some_and(|series| !series.is_empty())
        })
        .await;
        assert!(
            history["cpu"][0]["pod"]
                .as_str()
                .unwrap()
                .starts_with("web-"),
            "{history}"
        );
        assert_eq!(history["source"]["service"], format!("{ns}/vm:8428"));
        assert!(
            history.get("restarts").is_none(),
            "no kube-state-metrics: {history}"
        );
        let (status, setting) = call(&app, "GET", "/api/v1/settings/metrics-store").await;
        assert_eq!(status, StatusCode::OK, "{setting}");
        assert_eq!(
            serde_json::from_str::<Value>(&setting).unwrap()["state"],
            "available"
        );
    }
}
