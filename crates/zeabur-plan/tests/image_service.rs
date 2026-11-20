//! A service from a container image is one change that makes its Deployment and the Secrets,
//! claims, Service and Ingress the form or a Compose file asks for.

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use zeabur_graph::Snapshot;
    use zeabur_graph::pod_security::level;
    use zeabur_model::PodSecurityLevel;
    use zeabur_plan::{
        Change, EnvValueFrom, HostPort, ImageServiceChange, Input, Plan, PlanError, PlanRequest,
        RegistryLogin, ServedKind, ServiceSize, ServiceVariable, ServiceVolume, WaitFor, Write,
        plan,
    };

    fn kind(group: &str, kind: &str, plural: &str) -> ServedKind {
        ServedKind {
            group: group.to_owned(),
            kind: kind.to_owned(),
            plural: plural.to_owned(),
            namespaced: true,
        }
    }

    fn service() -> ImageServiceChange {
        ImageServiceChange {
            name: "web".to_owned(),
            image: "nginx:1.27".to_owned(),
            port: None,
            more_ports: Vec::new(),
            host: None,
            variables: Vec::new(),
            secret_variables: true,
            volumes: Vec::new(),
            size: ServiceSize::Small,
            replicas: 1,
            registry: None,
            security: PodSecurityLevel::Baseline,
            command: Vec::new(),
            args: Vec::new(),
            wait_for: None,
        }
    }

    fn planned(change: ImageServiceChange, issuer: Option<&str>) -> Result<Plan, PlanError> {
        let snapshot = Snapshot {
            namespace: "shop".to_owned(),
            ..Snapshot::default()
        };
        let kinds = [
            kind("", "Secret", "secrets"),
            kind("", "Service", "services"),
            kind("", "PersistentVolumeClaim", "persistentvolumeclaims"),
            kind("apps", "Deployment", "deployments"),
            kind("networking.k8s.io", "Ingress", "ingresses"),
        ];
        plan(
            Input {
                kinds: &kinds,
                cluster_issuer: issuer,
                ..Input::new(&snapshot, "2026-11-20T00:00:00Z")
            },
            &PlanRequest {
                changes: vec![Change::CreateImageService(change)],
            },
        )
    }

    fn created(plan: &Plan, kind: &str) -> Option<Value> {
        plan.objects
            .iter()
            .find(|o| o.target.kind == kind)
            .map(|o| match &o.write {
                Write::Create { body } => body.clone(),
                Write::Patch { .. }
                | Write::MergePatch { .. }
                | Write::Delete { .. }
                | Write::EphemeralContainers { .. } => panic!("{kind} is created"),
            })
    }

    fn with(edit: impl FnOnce(&mut ImageServiceChange)) -> ImageServiceChange {
        let mut change = service();
        edit(&mut change);
        change
    }

    /// Only an image is a worker: a Deployment and nothing that takes connections.
    #[test]
    fn only_an_image_is_a_worker() {
        let plan = planned(service(), None).unwrap();
        let deployment = created(&plan, "Deployment").unwrap();
        let container = &deployment["spec"]["template"]["spec"]["containers"][0];
        assert_eq!(container["image"], "nginx:1.27");
        assert!(container["readinessProbe"].is_null());
        assert!(created(&plan, "Service").is_none());
        assert!(created(&plan, "Ingress").is_none());
    }

    /// A port adds a Service, with the more ports after it, and a readiness probe; a host an
    /// Ingress, with HTTPS where the cluster has an issuer.
    #[test]
    fn a_port_and_a_host_open_the_service() {
        let change = with(|c| {
            c.port = Some(80);
            c.more_ports = vec![9090];
            c.host = Some(" web.example.com ".to_owned());
        });
        let plan = planned(change, Some("letsencrypt")).unwrap();
        let service = created(&plan, "Service").unwrap();
        assert_eq!(
            service["spec"]["ports"],
            json!([{"name": "http", "port": 80, "targetPort": 80},
                   {"name": "port-9090", "port": 9090, "targetPort": 9090}])
        );
        let deployment = created(&plan, "Deployment").unwrap();
        assert_eq!(
            deployment["spec"]["template"]["spec"]["containers"][0]["readinessProbe"],
            json!({"tcpSocket": {"port": 80}})
        );
        let ingress = created(&plan, "Ingress").unwrap();
        assert_eq!(ingress["spec"]["rules"][0]["host"], "web.example.com");
        assert_eq!(ingress["spec"]["tls"][0]["secretName"], "web-tls");
        assert_eq!(
            ingress["metadata"]["annotations"]["cert-manager.io/cluster-issuer"],
            "letsencrypt"
        );
    }

    /// Plain values go in `<name>-env` unless asked not to, a literal `$(` too; a Secret key and
    /// a value that expands other variables stay in the Deployment.
    #[test]
    fn variables_go_in_a_secret_unless_they_cannot() {
        let variables = vec![
            ServiceVariable {
                name: "MODE".to_owned(),
                value: "production".to_owned(),
                value_from: None,
                expands: false,
            },
            ServiceVariable {
                name: "TOKEN".to_owned(),
                value: "a$(PORT)b".to_owned(),
                value_from: None,
                expands: false,
            },
            ServiceVariable {
                name: "URL".to_owned(),
                value: "http://$(HOST)".to_owned(),
                value_from: None,
                expands: true,
            },
            ServiceVariable {
                name: "PASSWORD".to_owned(),
                value: String::new(),
                value_from: Some(EnvValueFrom {
                    secret_name: Some("db-app".to_owned()),
                    config_map_name: None,
                    key: Some("password".to_owned()),
                    field_path: None,
                }),
                expands: false,
            },
        ];
        let kept = planned(with(|c| c.variables = variables.clone()), None).unwrap();
        let secret = created(&kept, "Secret").unwrap();
        assert_eq!(secret["metadata"]["name"], "web-env");
        assert_eq!(
            secret["stringData"],
            json!({"MODE": "production", "TOKEN": "a$(PORT)b"})
        );
        let container =
            &created(&kept, "Deployment").unwrap()["spec"]["template"]["spec"]["containers"][0];
        assert_eq!(container["envFrom"][0]["secretRef"]["name"], "web-env");
        let names: Vec<&str> = container["env"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|e| e["name"].as_str())
            .collect();
        assert_eq!(names, ["URL", "PASSWORD"]);

        let plain = planned(
            with(|c| {
                c.variables = variables;
                c.secret_variables = false;
            }),
            None,
        )
        .unwrap();
        assert!(created(&plain, "Secret").is_none());
    }

    /// A Compose file may name a variable `discovery.type`; kube-apiserver judges the name in the
    /// plan's dry run, not a stricter rule here.
    #[test]
    fn a_variable_name_kubernetes_takes_plans() {
        let variable = ServiceVariable {
            name: "discovery.type".to_owned(),
            value: "single-node".to_owned(),
            value_from: None,
            expands: false,
        };
        let plan = planned(with(|c| c.variables = vec![variable]), None).unwrap();
        let secret = created(&plan, "Secret").unwrap();
        assert_eq!(
            secret["stringData"],
            json!({"discovery.type": "single-node"})
        );
    }

    /// A volume makes one replica that is recreated, not rolled, with a claim per volume.
    #[test]
    fn a_volume_runs_one_replica_recreated() {
        let change = with(|c| {
            c.replicas = 3;
            c.volumes = vec![ServiceVolume {
                name: "data".to_owned(),
                mount_path: "/data".to_owned(),
                size: "1Gi".to_owned(),
                read_only: false,
            }];
        });
        let plan = planned(change, None).unwrap();
        let claim = created(&plan, "PersistentVolumeClaim").unwrap();
        assert_eq!(claim["metadata"]["name"], "web-data");
        assert_eq!(claim["spec"]["resources"]["requests"]["storage"], "1Gi");
        let deployment = created(&plan, "Deployment").unwrap();
        assert_eq!(deployment["spec"]["replicas"], 1);
        assert_eq!(deployment["spec"]["strategy"]["type"], "Recreate");
        assert_eq!(
            deployment["spec"]["template"]["spec"]["volumes"][0]["persistentVolumeClaim"]["claimName"],
            "web-data"
        );
    }

    /// A private registry is a `dockerconfigjson` Secret the pods pull with.
    #[test]
    fn a_private_registry_is_a_secret_the_pods_pull_with() {
        let change = with(|c| {
            c.registry = Some(RegistryLogin {
                server: "ghcr.io".to_owned(),
                username: "acme".to_owned(),
                password: "token".to_owned(),
            });
        });
        let plan = planned(change, None).unwrap();
        let secret = created(&plan, "Secret").unwrap();
        assert_eq!(secret["type"], "kubernetes.io/dockerconfigjson");
        let config: Value =
            serde_json::from_str(secret["stringData"][".dockerconfigjson"].as_str().unwrap())
                .unwrap();
        assert_eq!(config["auths"]["ghcr.io"]["auth"], "YWNtZTp0b2tlbg==");
        let deployment = created(&plan, "Deployment").unwrap();
        assert_eq!(
            deployment["spec"]["template"]["spec"]["imagePullSecrets"],
            json!([{"name": "web-registry"}])
        );
    }

    /// Strict and Compatible pods meet their level; neither mounts the service account's token.
    #[test]
    fn each_profile_meets_its_level() {
        for security in [PodSecurityLevel::Restricted, PodSecurityLevel::Baseline] {
            let plan = planned(with(|c| c.security = security), None).unwrap();
            let pod = &created(&plan, "Deployment").unwrap()["spec"]["template"]["spec"];
            assert_eq!(
                level(&serde_json::from_value(pod.clone()).unwrap()),
                security
            );
            assert_eq!(pod["automountServiceAccountToken"], false);
        }
    }

    /// Init containers wait for each host in turn, as nobody, so a restricted pod starts them.
    #[test]
    fn init_containers_wait_for_each_host() {
        let change = with(|c| {
            c.security = PodSecurityLevel::Restricted;
            c.wait_for = Some(WaitFor {
                hosts: vec![HostPort {
                    host: "db".to_owned(),
                    port: 5432,
                }],
                image: "busybox:1.37".to_owned(),
            });
        });
        let plan = planned(change, None).unwrap();
        let pod = &created(&plan, "Deployment").unwrap()["spec"]["template"]["spec"];
        assert_eq!(pod["initContainers"][0]["name"], "wait-1");
        assert_eq!(
            pod["initContainers"][0]["command"][2],
            "until nc -z -w 2 db 5432; do sleep 2; done"
        );
        assert_eq!(
            level(&serde_json::from_value(pod.clone()).unwrap()),
            PodSecurityLevel::Restricted
        );
    }

    /// The form's rules are the server's.
    #[test]
    fn a_request_the_form_would_refuse_plans_nothing() {
        let code = |change: ImageServiceChange| planned(change, None).unwrap_err().code;
        assert_eq!(code(with(|c| c.name = "Web".to_owned())), "invalid_name");
        assert_eq!(code(with(|c| c.image = "-x".to_owned())), "invalid_image");
        assert_eq!(code(with(|c| c.replicas = 0)), "invalid_replicas");
        assert_eq!(code(with(|c| c.replicas = 21)), "invalid_replicas");
        assert_eq!(
            code(with(|c| c.host = Some("web.example.com".to_owned()))),
            "host_without_port"
        );
        assert_eq!(
            code(with(|c| c.volumes = vec![ServiceVolume {
                name: "data".to_owned(),
                mount_path: "data".to_owned(),
                size: "1Gi".to_owned(),
                read_only: false,
            }])),
            "relative_mount_path"
        );
        assert_eq!(
            code(with(|c| c.registry = Some(RegistryLogin {
                server: "ghcr.io".to_owned(),
                username: String::new(),
                password: "x".to_owned(),
            }))),
            "invalid_registry"
        );
    }
}
