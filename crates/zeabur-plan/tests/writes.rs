//! What a plan writes to an object that exists, as properties over every change kind that does
//! so [quickcheck]:
//!
//! - **A write never takes over a field Helm, Argo CD or Flux owns.** Whoever owns the field a
//!   change touches (in `managedFields` [k8s-ssa-fields]), the plan is refused when the owner is a
//!   tool that reapplies what it owns, and names any other owner.
//! - **The write is the change the person approves.** The JSON Patch [rfc6902], applied to the
//!   object as it is stored, gives exactly the object the plan shows as the result. No other RFC
//!   6902 implementation is in the dependency tree, so the patch is applied with Zeabur's; what the
//!   property adds is that the whole write (its precondition, and the operations of several changes
//!   to one object in order) holds on the stored object, not on the planner's working copy.
//!   kube-apiserver's own answer is checked by the cluster tests. The Secret's values are readable
//!   here: a plan without them cannot show the keys it does not touch.
//!
//! The cases are generated: the workload's kind, its container, variables, port and mount, the
//! routes' hostnames and the keys of a ConfigMap and a Secret. A restart is left out of the
//! first property: it may be planned on an object a tool manages. Adding a volume or a domain
//! that gets a route of its own writes new list entries and new objects, which nobody owns yet.
//!
//! [quickcheck]: https://www.cs.tufts.edu/comp/150FP/archive/john-hughes/quick.pdf
//! [k8s-ssa-fields]: https://kubernetes.io/docs/reference/using-api/server-side-apply/#field-management
//! [rfc6902]: https://www.rfc-editor.org/rfc/rfc6902

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use base64::Engine as _;
    use proptest::prelude::*;
    use serde_json::{Value, from_value, json};
    use zeabur_graph::Snapshot;
    use zeabur_model::{PodSecurityLevel, Tool, WorkloadKind, WorkloadRef};
    use zeabur_plan::checks::{ProbeHandler, ProbeRequest, ResourceRef};
    use zeabur_plan::json::apply_patch;
    use zeabur_plan::rules::EnvEdit;
    use zeabur_plan::{
        Change, DataEdit, DataKind, ErrorKind, Fetched, Input, Plan, PlanError, PlanRequest,
        ProbeKind, Release, ReleasePods, RouteKind, RouteRef, Write, plan,
    };

    /// The managers that reapply what they own: Helm [helm], Flux's controllers [flux] and Argo
    /// CD's, with and without server-side apply [argocd].
    ///
    /// [helm]: https://github.com/helm/helm/blob/main/pkg/kube/client.go
    /// [flux]: https://fluxcd.io/flux/components/kustomize/kustomizations/#server-side-apply
    /// [argocd]: https://argo-cd.readthedocs.io/en/stable/user-guide/sync-options/#server-side-apply
    const REAPPLYING: [(&str, Tool); 5] = [
        ("helm", Tool::Helm),
        ("argocd-controller", Tool::ArgoCd),
        ("argocd-application-controller", Tool::ArgoCd),
        ("kustomize-controller", Tool::Flux),
        ("helm-controller", Tool::Flux),
    ];

    /// Managers whose fields a person may change: `zeabur`, kubectl's verbs and controllers.
    const OTHERS: [&str; 6] = [
        "zeabur",
        "kubectl",
        "kubectl-client-side-apply",
        "kubectl-edit",
        "kube-controller-manager",
        "k9s",
    ];

    const NOW: &str = "2026-10-15T00:00:00Z";

    /// One generated project: a workload `app` with one container, its Service, a route with
    /// several hostnames, a ConfigMap `settings`, a Secret `credentials` and a claim `data`.
    #[derive(Debug, Clone)]
    struct World {
        kind: WorkloadKind,
        container: String,
        /// At least one variable.
        env: BTreeMap<String, String>,
        port: i32,
        mount: String,
        /// At least two, so one can be removed.
        hosts: BTreeSet<String>,
        /// The route is an Ingress, else an `HTTPRoute`.
        ingress: bool,
        /// The key the ConfigMap and the Secret hold.
        key: String,
        team: String,
    }

    fn world() -> impl Strategy<Value = World> {
        (
            prop::sample::select(vec![
                WorkloadKind::Deployment,
                WorkloadKind::StatefulSet,
                WorkloadKind::DaemonSet,
                WorkloadKind::CronJob,
            ]),
            "[a-z][a-z0-9-]{0,7}",
            prop::collection::btree_map("[A-Z][A-Z_]{0,5}", "[0-9][a-z0-9]{0,5}", 1..4),
            1_i32..65536,
            "/[a-z]{1,6}(/[a-z]{1,6})?",
            prop::collection::btree_set("[a-z]{1,8}", 2..4),
            any::<bool>(),
            "[a-z][a-z0-9._-]{0,8}",
            "[a-z]{1,6}",
        )
            .prop_map(
                |(kind, container, env, port, mount, hosts, ingress, key, team)| World {
                    kind,
                    container,
                    env,
                    port,
                    mount,
                    hosts: hosts
                        .into_iter()
                        .map(|h| format!("{h}.example.test"))
                        .collect(),
                    ingress,
                    key,
                    team,
                },
            )
    }

    impl World {
        fn reference(&self) -> WorkloadRef {
            WorkloadRef {
                kind: self.kind,
                name: "app".to_owned(),
            }
        }

        fn first_variable(&self) -> String {
            self.env.keys().next().cloned().unwrap_or_default()
        }

        fn first_host(&self) -> String {
            self.hosts.iter().next().cloned().unwrap_or_default()
        }

        fn template(&self, image: &str) -> Value {
            let env: Vec<Value> = self
                .env
                .iter()
                .map(|(name, value)| json!({"name": name, "value": value}))
                .collect();
            json!({
                "metadata": {"labels": {"app": "app"}},
                "spec": {
                    "containers": [{
                        "name": self.container, "image": image, "env": env,
                        "ports": [{"name": "web", "containerPort": self.port, "protocol": "TCP"}],
                        "volumeMounts": [{"name": "data", "mountPath": self.mount}],
                        "livenessProbe": {"tcpSocket": {"port": self.port}},
                        "resources": {"requests": {"cpu": "100m"}}
                    }],
                    "volumes": [{"name": "data", "emptyDir": {}}]
                }
            })
        }

        /// The workload as kube-apiserver stores it.
        fn workload(&self) -> Value {
            let template = self.template("example/app:2");
            let selector = json!({"matchLabels": {"app": "app"}});
            let spec = match self.kind {
                WorkloadKind::CronJob => {
                    json!({"schedule": "0 * * * *", "jobTemplate": {"spec": {"template": template}}})
                }
                WorkloadKind::DaemonSet => json!({"selector": selector, "template": template}),
                WorkloadKind::StatefulSet => json!({"replicas": 1, "serviceName": "app",
                    "selector": selector, "template": template}),
                WorkloadKind::Job => json!({"template": template}),
                WorkloadKind::Deployment => {
                    json!({"replicas": 1, "selector": selector, "template": template})
                }
            };
            json!({
                "apiVersion": self.resource().api_version(), "kind": self.kind.as_str(),
                "metadata": {"name": "app", "namespace": "demo", "resourceVersion": "7",
                             "generation": 2, "labels": {"app": "app", "team": self.team}},
                "spec": spec
            })
        }

        fn resource(&self) -> ResourceRef {
            let (group, resource) = match self.kind {
                WorkloadKind::CronJob => ("batch", "cronjobs"),
                WorkloadKind::DaemonSet => ("apps", "daemonsets"),
                WorkloadKind::StatefulSet => ("apps", "statefulsets"),
                WorkloadKind::Job => ("batch", "jobs"),
                WorkloadKind::Deployment => ("apps", "deployments"),
            };
            ResourceRef {
                group: group.to_owned(),
                version: "v1".to_owned(),
                resource: resource.to_owned(),
            }
        }

        /// Every object of the project as stored, by the name the cases use.
        fn objects(&self) -> BTreeMap<&'static str, Value> {
            let mut objects = BTreeMap::from([
                ("workload", self.workload()),
                (
                    "service",
                    json!({"apiVersion": "v1", "kind": "Service",
                        "metadata": {"name": "app", "namespace": "demo", "resourceVersion": "3"},
                        "spec": {"selector": {"app": "app"},
                                 "ports": [{"name": "http", "port": 80, "targetPort": self.port}]}}),
                ),
                (
                    "configmap",
                    json!({"apiVersion": "v1", "kind": "ConfigMap",
                        "metadata": {"name": "settings", "namespace": "demo", "resourceVersion": "4"},
                        "data": {(self.key.clone()): "1"}}),
                ),
                (
                    "secret",
                    json!({"apiVersion": "v1", "kind": "Secret", "type": "Opaque",
                        "metadata": {"name": "credentials", "namespace": "demo", "resourceVersion": "5"},
                        "data": {(self.key.clone()): base64::engine::general_purpose::STANDARD.encode("s3cret")}}),
                ),
                (
                    "claim",
                    json!({"apiVersion": "v1", "kind": "PersistentVolumeClaim",
                        "metadata": {"name": "data", "namespace": "demo", "resourceVersion": "6"},
                        "spec": {"accessModes": ["ReadWriteOnce"],
                                 "resources": {"requests": {"storage": "1Gi"}}}}),
                ),
            ]);
            let route = if self.ingress {
                let rules: Vec<Value> = self
                    .hosts
                    .iter()
                    .map(|h| {
                        json!({"host": h, "http": {"paths": [{"path": "/", "pathType": "Prefix",
                            "backend": {"service": {"name": "app", "port": {"number": 80}}}}]}})
                    })
                    .collect();
                json!({"apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
                    "metadata": {"name": "app-routes", "namespace": "demo", "resourceVersion": "8",
                                 "generation": 1},
                    "spec": {"rules": rules}})
            } else {
                json!({"apiVersion": "gateway.networking.k8s.io/v1", "kind": "HTTPRoute",
                    "metadata": {"name": "app-routes", "namespace": "demo", "resourceVersion": "8",
                                 "generation": 1},
                    "spec": {"hostnames": self.hosts,
                             "rules": [{"backendRefs": [{"name": "app", "port": 80}]}]}})
            };
            objects.insert("route", route);
            objects
        }

        fn route(&self) -> RouteRef {
            RouteRef {
                kind: if self.ingress {
                    RouteKind::Ingress
                } else {
                    RouteKind::HttpRoute
                },
                name: "app-routes".to_owned(),
            }
        }

        /// The `managedFields` path of the pod template's spec.
        fn pod_spec(&self) -> Vec<String> {
            let names: &[&str] = if self.kind == WorkloadKind::CronJob {
                &["spec", "jobTemplate", "spec", "template", "spec"]
            } else {
                &["spec", "template", "spec"]
            };
            names.iter().map(|n| field(n)).collect()
        }

        /// The `managedFields` path of the container.
        fn container_path(&self) -> Vec<String> {
            let mut path = self.pod_spec();
            path.extend([field("containers"), key(&json!({"name": self.container}))]);
            path
        }
    }

    /// `f:<name>`.
    fn field(name: &str) -> String {
        format!("f:{name}")
    }

    /// `k:<json>`: a list entry by its key fields, as kube-apiserver writes it.
    fn key(fields: &Value) -> String {
        format!("k:{fields}")
    }

    /// The change kinds that write an object that exists.
    const CASES: usize = 19;

    /// Case `which` on `world`: the world (with the workload kind the change needs), the change,
    /// the object it writes and the path of the field it changes, in `managedFields` terms.
    #[expect(clippy::too_many_lines, reason = "one arm per change kind")]
    fn case(world: &World, which: usize) -> (World, Change, &'static str, Vec<String>) {
        let mut w = world.clone();
        // Only Deployments and StatefulSets scale, a CronJob keeps no releases, and a Service
        // selects the pods of a workload that runs them.
        match which {
            5 if !matches!(w.kind, WorkloadKind::Deployment | WorkloadKind::StatefulSet) => {
                w.kind = WorkloadKind::Deployment;
            }
            6 | 10 if w.kind == WorkloadKind::CronJob => w.kind = WorkloadKind::Deployment,
            _ => {}
        }
        let variable = w.first_variable();
        let mut env = w.container_path();
        env.extend([field("env"), key(&json!({"name": variable}))]);
        let container = w.container_path();
        let at = |extra: &[String]| [container.clone(), extra.to_vec()].concat();
        let host = w.first_host();
        let route_field = field(if w.ingress { "rules" } else { "hostnames" });
        let env_edit = |op: &str, value: Option<&str>| Change::Env {
            workload: w.reference(),
            container: Some(w.container.clone()),
            changes: vec![EnvEdit {
                op: op.to_owned(),
                name: variable.clone(),
                value: value.map(str::to_owned),
                value_from: None,
            }],
        };
        let yaml = |edit: &dyn Fn(&mut Value)| {
            let mut edited = zeabur_plan::edit::editable(&w.workload());
            edit(&mut edited);
            Change::Edit {
                resource: w.resource(),
                name: "app".to_owned(),
                yaml: edited.to_string(),
            }
        };
        let containers = if w.kind == WorkloadKind::CronJob {
            "/spec/jobTemplate/spec/template/spec/containers/0"
        } else {
            "/spec/template/spec/containers/0"
        };
        match which {
            0 => (w.clone(), env_edit("set", Some("changed")), "workload", env),
            1 => (w.clone(), env_edit("remove", None), "workload", env),
            2 => (
                w.clone(),
                Change::Probe {
                    workload: w.reference(),
                    container: Some(w.container.clone()),
                    probe: ProbeKind::Liveness,
                    set: Some(ProbeRequest {
                        handler: ProbeHandler::HttpGet,
                        port: Some(w.port),
                        path: Some("/healthz".to_owned()),
                        scheme: None,
                        command: None,
                        initial_delay_seconds: None,
                        period_seconds: None,
                        timeout_seconds: None,
                        failure_threshold: None,
                    }),
                },
                "workload",
                at(&[field("livenessProbe")]),
            ),
            3 => (
                w.clone(),
                Change::Resources {
                    workload: w.reference(),
                    container: Some(w.container.clone()),
                    requests: BTreeMap::from([("cpu".to_owned(), "250m".to_owned())]),
                    limits: BTreeMap::new(),
                },
                "workload",
                at(&[field("resources")]),
            ),
            4 => (
                w.clone(),
                Change::EditMount {
                    workload: w.reference(),
                    container: Some(w.container.clone()),
                    mount_path: w.mount.clone(),
                    new_mount_path: None,
                    read_only: Some(true),
                },
                "workload",
                at(&[field("volumeMounts"), key(&json!({"mountPath": w.mount}))]),
            ),
            5 => (
                w.clone(),
                Change::Scale {
                    workload: w.reference(),
                    replicas: 3,
                },
                "workload",
                vec![field("spec"), field("replicas")],
            ),
            6 => (
                w.clone(),
                Change::Rollback {
                    workload: w.reference(),
                    revision: 1,
                },
                "workload",
                vec![field("spec"), field("template")],
            ),
            7 => (
                w.clone(),
                Change::Data {
                    kind: DataKind::ConfigMap,
                    name: "settings".to_owned(),
                    changes: vec![DataEdit {
                        op: "set".to_owned(),
                        key: w.key.clone(),
                        value: Some("2".to_owned()),
                    }],
                },
                "configmap",
                vec![field("data"), field(&w.key)],
            ),
            8 => (
                w.clone(),
                Change::Data {
                    kind: DataKind::Secret,
                    name: "credentials".to_owned(),
                    changes: vec![DataEdit {
                        op: "remove".to_owned(),
                        key: w.key.clone(),
                        value: None,
                    }],
                },
                "secret",
                vec![field("data"), field(&w.key)],
            ),
            9 => (
                w.clone(),
                Change::ExpandVolume {
                    claim: "data".to_owned(),
                    size: "2Gi".to_owned(),
                },
                "claim",
                vec![
                    field("spec"),
                    field("resources"),
                    field("requests"),
                    field("storage"),
                ],
            ),
            10 => (
                w.clone(),
                Change::AddDomain {
                    workload: w.reference(),
                    hostname: "new-domain.example.test".to_owned(),
                },
                "route",
                vec![field("spec"), route_field],
            ),
            11 => (
                w.clone(),
                Change::RenameDomain {
                    route: w.route(),
                    from: host,
                    to: "renamed-domain.example.test".to_owned(),
                },
                "route",
                vec![field("spec"), route_field],
            ),
            12 => (
                w.clone(),
                Change::RemoveDomain {
                    route: w.route(),
                    hostname: host,
                },
                "route",
                vec![field("spec"), route_field],
            ),
            13 => (
                w.clone(),
                yaml(&|o| {
                    if let Some(image) = o.pointer_mut(&format!("{containers}/image")) {
                        *image = json!("example/app:3");
                    }
                }),
                "workload",
                at(&[field("image")]),
            ),
            // A container port is keyed by its number and protocol [k8s-core-types].
            //
            // [k8s-core-types]: https://github.com/kubernetes/api/blob/master/core/v1/types.go
            14 => (
                w.clone(),
                yaml(&|o| {
                    if let Some(port) = o.pointer_mut(&format!("{containers}/ports/0")) {
                        port["name"] = json!("http");
                    }
                }),
                "workload",
                at(&[
                    field("ports"),
                    key(&json!({"containerPort": w.port, "protocol": "TCP"})),
                    field("name"),
                ]),
            ),
            15 => (
                w.clone(),
                yaml(&|o| o["metadata"]["labels"]["team"] = json!("platform")),
                "workload",
                vec![field("metadata"), field("labels"), field("team")],
            ),
            // A Secret's YAML with its values shown masked, as the YAML view shows them.
            16 => {
                let mut edited = zeabur_plan::edit::editable(&w.objects()["secret"]);
                edited["type"] = json!("example.com/token");
                edited["data"][&w.key] = json!("(hidden)");
                (
                    w.clone(),
                    Change::Edit {
                        resource: ResourceRef {
                            group: String::new(),
                            version: "v1".to_owned(),
                            resource: "secrets".to_owned(),
                        },
                        name: "credentials".to_owned(),
                        yaml: edited.to_string(),
                    },
                    "secret",
                    vec![field("type")],
                )
            }
            17 => (
                w.clone(),
                Change::SecurityProfile {
                    workload: w.reference(),
                    profile: PodSecurityLevel::Restricted,
                },
                "workload",
                at(&[field("securityContext")]),
            ),
            _ => {
                let mut edited = zeabur_plan::edit::editable(&w.objects()["configmap"]);
                edited["data"][&w.key] = json!("2");
                (
                    w.clone(),
                    Change::Edit {
                        resource: ResourceRef {
                            group: String::new(),
                            version: "v1".to_owned(),
                            resource: "configmaps".to_owned(),
                        },
                        name: "settings".to_owned(),
                        yaml: edited.to_string(),
                    },
                    "configmap",
                    vec![field("data"), field(&w.key)],
                )
            }
        }
    }

    /// How much of the field the manager owns.
    #[derive(Debug, Clone, Copy)]
    enum Extent {
        /// The field itself.
        Whole,
        /// Only a part of it.
        Part,
        /// A field beside it, not the field.
        Sibling,
    }

    fn extent() -> impl Strategy<Value = Extent> {
        prop::sample::select(vec![Extent::Whole, Extent::Part, Extent::Sibling])
    }

    /// Records `manager` as the owner of `path` in `object` (to `extent`).
    fn own(object: &mut Value, manager: &str, path: &[String], extent: Extent) {
        let mut path = path.to_vec();
        let leaf = match extent {
            Extent::Whole => json!({}),
            Extent::Part => json!({".": {}, "f:part": {}}),
            Extent::Sibling => {
                path.pop();
                path.push(field("other field"));
                json!({})
            }
        };
        let fields = path
            .iter()
            .rev()
            .fold(leaf, |inner, token| json!({ (token.clone()): inner }));
        object["metadata"]["managedFields"] = json!([{
            "manager": manager, "operation": "Update", "apiVersion": object["apiVersion"],
            "time": "2026-10-01T00:00:00Z", "fieldsType": "FieldsV1", "fieldsV1": fields
        }]);
    }

    /// What the API reads of `objects`: the snapshot, the `HTTPRoute` objects and the values of
    /// the Secret.
    fn read(
        world: &World,
        objects: &BTreeMap<&'static str, Value>,
    ) -> (Snapshot, Vec<Value>, BTreeMap<String, Vec<u8>>) {
        let object = |name: &str| objects.get(name).cloned().unwrap_or_default();
        let mut snapshot = Snapshot {
            namespace: "demo".to_owned(),
            ..Snapshot::default()
        };
        let workload = object("workload");
        match world.kind {
            WorkloadKind::CronJob => snapshot.cron_jobs.push(from_value(workload).unwrap()),
            WorkloadKind::DaemonSet => snapshot.daemon_sets.push(from_value(workload).unwrap()),
            WorkloadKind::StatefulSet => {
                snapshot.stateful_sets.push(from_value(workload).unwrap());
            }
            WorkloadKind::Job => snapshot.jobs.push(from_value(workload).unwrap()),
            WorkloadKind::Deployment => snapshot.deployments.push(from_value(workload).unwrap()),
        }
        snapshot
            .services
            .push(from_value(object("service")).unwrap());
        snapshot
            .config_maps
            .push(from_value(object("configmap")).unwrap());
        let mut secret = object("secret");
        let stored = secret
            .as_object_mut()
            .and_then(|s| s.remove("data"))
            .unwrap_or_default();
        snapshot.secrets.push(from_value(secret).unwrap());
        snapshot
            .persistent_volume_claims
            .push(from_value(object("claim")).unwrap());
        let mut routes = Vec::new();
        if world.ingress {
            snapshot
                .ingresses
                .push(from_value(object("route")).unwrap());
        } else {
            routes.push(object("route"));
        }
        let values: BTreeMap<String, Vec<u8>> = stored
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, v)| {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(v.as_str().unwrap_or_default())
                    .unwrap();
                (k.clone(), bytes)
            })
            .collect();
        (snapshot, routes, values)
    }

    /// Plans `changes` against `objects`, as the API does with what it reads.
    fn planned(
        world: &World,
        objects: &BTreeMap<&'static str, Value>,
        changes: Vec<Change>,
    ) -> Result<Plan, PlanError> {
        let (snapshot, routes, values) = read(world, objects);
        let secret_values = BTreeMap::from([("credentials".to_owned(), values)]);
        let releases = BTreeMap::from([(
            world.reference(),
            vec![Release {
                revision: 1,
                name: "app-1".to_owned(),
                images: vec!["example/app:1".to_owned()],
                created: None,
                change_cause: None,
                current: false,
                pods: ReleasePods { ready: 0, total: 0 },
                template: world.template("example/app:1"),
                template_yaml: String::new(),
            }],
        )]);
        let fetched = [Fetched {
            resource: world.resource(),
            name: "app".to_owned(),
            object: objects.get("workload").cloned().unwrap_or_default(),
        }];
        plan(
            Input {
                http_routes: &routes,
                secret_values: &secret_values,
                releases: &releases,
                storage_classes: None,
                objects: &fetched,
                ..Input::new(&snapshot, NOW)
            },
            &PlanRequest { changes },
        )
    }

    fn manager() -> impl Strategy<Value = &'static str> {
        let reapplying = REAPPLYING.iter().map(|(manager, _)| *manager);
        prop::sample::select(reapplying.chain(OTHERS).collect::<Vec<_>>())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// Given a field owned by `manager`, when a change touches it, then the plan is refused
        /// if `manager` reapplies what it owns, naming its tool, and otherwise names `manager`
        /// as an owner; a field beside it is neither refused nor named.
        #[test]
        fn a_field_a_reapplying_tool_owns_is_never_taken_over(
            world in world(),
            which in 0..CASES,
            manager in manager(),
            extent in extent(),
        ) {
            let (world, change, target, path) = case(&world, which);
            let mut objects = world.objects();
            if let Some(object) = objects.get_mut(target) {
                own(object, manager, &path, extent);
            }
            let result = planned(&world, &objects, vec![change]);
            let owns = !matches!(extent, Extent::Sibling);
            let tool = REAPPLYING.iter().find(|(m, _)| *m == manager).map(|(_, tool)| *tool);
            if let (true, Some(tool)) = (owns, tool) {
                let Err(error) = result else {
                    return Err(TestCaseError::fail(format!("{manager} owns {path:?}, yet it was planned")));
                };
                prop_assert_eq!(error.code.as_str(), "field_managed_by_tool", "{}", error.detail);
                prop_assert_eq!(error.kind, ErrorKind::Conflict);
                prop_assert!(error.detail.contains(manager), "{}", error.detail);
                prop_assert_eq!(error.managed_by, Some(tool));
            } else {
                let plan = result.map_err(|e| TestCaseError::fail(format!("{}: {}", e.code, e.detail)))?;
                let named = plan
                    .objects
                    .iter()
                    .flat_map(|o| &o.owners)
                    .any(|o| o.managers.iter().any(|m| m == manager));
                prop_assert_eq!(named, owns, "owners: {:?}", plan.objects.iter().map(|o| &o.owners).collect::<Vec<_>>());
            }
        }

        /// Given changes to several objects (several to some), when the plan's patches are
        /// applied to the objects as stored, then each gives exactly the object the plan shows
        /// as the result: the `test` operations hold on the stored object, and nothing the plan
        /// does not show is written.
        #[test]
        fn each_patch_turns_the_stored_object_into_the_planned_one(
            world in world(),
            picks in prop::collection::btree_set(0..CASES, 1..5),
            restart in any::<bool>(),
        ) {
            let mut changes = Vec::new();
            let mut world = world;
            let mut picks = picks;
            // A renamed hostname is no longer there to remove.
            if picks.contains(&11) {
                picks.remove(&12);
            }
            // Changes that need another workload kind are planned on that kind alone.
            for which in picks {
                let (adjusted, change, _, _) = case(&world, which);
                if adjusted.kind != world.kind {
                    if !changes.is_empty() {
                        continue;
                    }
                    world = adjusted;
                }
                changes.push(change);
            }
            if restart && world.kind != WorkloadKind::CronJob {
                changes.push(Change::Restart { workload: world.reference() });
            }
            let objects = world.objects();
            let plan = planned(&world, &objects, changes).map_err(|e| TestCaseError::fail(format!("{}: {}", e.code, e.detail)))?;
            for object in &plan.objects {
                let Write::Patch { operations } = &object.write else {
                    return Err(TestCaseError::fail(format!("{} was created", object.target.name)));
                };
                let stored = objects
                    .values()
                    .find(|o| o["kind"] == object.target.kind.as_str() && o["metadata"]["name"] == object.target.name.as_str())
                    .unwrap();
                let written = apply_patch(stored, operations).map_err(|e| TestCaseError::fail(e.to_string()))?;
                prop_assert_eq!(&written, &object.raw_after, "{}", object.target.kind);
            }
        }
    }
}
