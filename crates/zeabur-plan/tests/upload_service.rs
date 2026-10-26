//! A service from an uploaded folder is one change that makes the `Build` and the objects of the
//! service that runs what it builds, the same for the web interface and `zeaburctl up`.

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use zeabur_graph::Snapshot;
    use zeabur_graph::pod_security::level;
    use zeabur_model::PodSecurityLevel;
    use zeabur_plan::{
        Change, Input, NameValue, Plan, PlanError, PlanRequest, ServedKind, ServiceSize,
        UploadMethod, UploadServiceChange, Write, plan,
    };

    fn kind(group: &str, kind: &str, plural: &str) -> ServedKind {
        ServedKind {
            group: group.to_owned(),
            kind: kind.to_owned(),
            plural: plural.to_owned(),
            namespaced: true,
        }
    }

    fn service() -> UploadServiceChange {
        UploadServiceChange {
            name: "site".to_owned(),
            request: "up-1".to_owned(),
            method: UploadMethod::Railpack,
            directory: None,
            dockerfile: None,
            port: None,
            host: None,
            variables: Vec::new(),
            build_variables: Vec::new(),
            size: None,
            security: None,
        }
    }

    fn planned(
        change: UploadServiceChange,
        registry: Option<&str>,
        issuer: Option<&str>,
    ) -> Result<Plan, PlanError> {
        let snapshot = Snapshot {
            namespace: "shop".to_owned(),
            ..Snapshot::default()
        };
        let kinds = [
            kind("", "Secret", "secrets"),
            kind("", "Service", "services"),
            kind("apps", "Deployment", "deployments"),
            kind("networking.k8s.io", "Ingress", "ingresses"),
            kind("builder.zeabur.com", "Build", "builds"),
        ];
        plan(
            Input {
                kinds: &kinds,
                build_registry: registry,
                cluster_issuer: issuer,
                ..Input::new(&snapshot, "2026-10-23T00:00:00Z")
            },
            &PlanRequest {
                changes: vec![Change::CreateUploadService(change)],
            },
        )
    }

    fn created(plan: &Plan, kind: &str) -> Option<Value> {
        plan.objects
            .iter()
            .find(|o| o.target.kind == kind)
            .map(|o| match &o.write {
                Write::Create { body } => body.clone(),
                Write::Patch { .. } | Write::Delete { .. } | Write::EphemeralContainers { .. } => {
                    panic!("{kind} is created")
                }
            })
    }

    fn kinds_of(plan: &Plan) -> Vec<&str> {
        plan.objects
            .iter()
            .map(|o| o.target.kind.as_str())
            .collect()
    }

    /// The Build asks for its token as it is made and builds an uploaded folder into the
    /// registry; the Deployment runs that repository, and the Build comes last.
    #[test]
    fn the_build_asks_for_its_token_and_the_deployment_runs_its_image() {
        let plan = planned(service(), Some("zot.zot.svc:5000/"), None).unwrap();
        let build = created(&plan, "Build").unwrap();
        assert_eq!(
            build["metadata"]["annotations"]["builder.zeabur.com/upload-token-request"],
            "up-1"
        );
        assert_eq!(build["metadata"]["namespace"], "shop");
        assert_eq!(build["spec"]["source"], json!({"archive": {}}));
        assert_eq!(
            build["spec"]["output"]["image"],
            "zot.zot.svc:5000/shop/site"
        );
        assert_eq!(build["spec"]["builderRef"], "railpack");
        let deployment = created(&plan, "Deployment").unwrap();
        assert_eq!(
            deployment["spec"]["template"]["spec"]["containers"][0]["image"],
            "zot.zot.svc:5000/shop/site:pending"
        );
        assert_eq!(
            deployment["metadata"]["annotations"]["keel.sh/policy"],
            "force"
        );
        assert_eq!(kinds_of(&plan).last(), Some(&"Build"));
    }

    /// Without a port the service is a worker: no Service, no variables Secret.
    #[test]
    fn a_service_without_a_port_takes_no_connections() {
        let plan = planned(service(), Some("zot:5000"), None).unwrap();
        assert!(created(&plan, "Service").is_none());
        assert!(created(&plan, "Secret").is_none());
        assert!(created(&plan, "Ingress").is_none());
    }

    /// A port gets a Service and `PORT`; a host gets an Ingress that asks for HTTPS where the
    /// cluster has an issuer.
    #[test]
    fn a_port_and_a_host_open_the_service_to_connections() {
        let mut change = service();
        change.port = Some(8080);
        change.host = Some("site-shop.example.com".to_owned());
        change.variables = vec![NameValue {
            name: "MODE".to_owned(),
            value: "production".to_owned(),
        }];
        let plan = planned(change.clone(), Some("zot:5000"), Some("letsencrypt")).unwrap();
        let service = created(&plan, "Service").unwrap();
        assert_eq!(service["spec"]["ports"][0]["port"], 8080);
        let secret = created(&plan, "Secret").unwrap();
        assert_eq!(secret["metadata"]["name"], "site-env");
        assert_eq!(secret["stringData"]["PORT"], "8080");
        assert_eq!(secret["stringData"]["MODE"], "production");
        let ingress = created(&plan, "Ingress").unwrap();
        assert_eq!(ingress["spec"]["rules"][0]["host"], "site-shop.example.com");
        assert_eq!(ingress["spec"]["tls"][0]["secretName"], "site-tls");
        assert_eq!(
            ingress["metadata"]["annotations"]["cert-manager.io/cluster-issuer"],
            "letsencrypt"
        );
        let no_issuer = planned(change, Some("zot:5000"), None).unwrap();
        assert!(created(&no_issuer, "Ingress").unwrap()["spec"]["tls"].is_null());
    }

    /// Build variables go to the Secret `<name>-build-env`, and the Build reads them from there.
    #[test]
    fn build_variables_are_read_from_a_secret() {
        let mut change = service();
        change.build_variables = vec![NameValue {
            name: "NPM_TOKEN".to_owned(),
            value: "s3cret".to_owned(),
        }];
        let plan = planned(change, Some("zot:5000"), None).unwrap();
        let secret = plan
            .objects
            .iter()
            .find(|o| o.target.name == "site-build-env")
            .unwrap();
        assert_eq!(secret.target.kind, "Secret");
        let build = created(&plan, "Build").unwrap();
        assert_eq!(
            build["spec"]["env"][0]["valueFrom"]["secretKeyRef"],
            json!({"name": "site-build-env", "key": "NPM_TOKEN"})
        );
    }

    /// Railpack images run as a regular user, so the pod is restricted; a Dockerfile's often run
    /// as root, so baseline; a choice wins. The folder and Dockerfile go in the source.
    #[test]
    fn the_security_profile_follows_the_method_unless_chosen() {
        let level_of = |change: UploadServiceChange| {
            let plan = planned(change, Some("zot:5000"), None).unwrap();
            let deployment = created(&plan, "Deployment").unwrap();
            level(&serde_json::from_value(deployment["spec"]["template"]["spec"].clone()).unwrap())
        };
        assert_eq!(level_of(service()), PodSecurityLevel::Restricted);
        let mut docker = service();
        docker.method = UploadMethod::Dockerfile;
        docker.directory = Some("./apps/web/".to_owned());
        docker.dockerfile = Some("docker/Dockerfile.prod".to_owned());
        assert_eq!(level_of(docker.clone()), PodSecurityLevel::Baseline);
        docker.security = Some(PodSecurityLevel::Restricted);
        docker.size = Some(ServiceSize::Large);
        assert_eq!(level_of(docker.clone()), PodSecurityLevel::Restricted);
        let build = created(&planned(docker, Some("zot:5000"), None).unwrap(), "Build").unwrap();
        assert_eq!(
            build["spec"]["source"]["archive"],
            json!({"contextDir": "apps/web", "dockerfile": "docker/Dockerfile.prod"})
        );
        assert_eq!(build["spec"]["builderRef"], "dockerfile");
    }

    /// Nothing is planned for a cluster without a registry, or for input the Build would refuse.
    #[test]
    fn a_request_the_build_would_refuse_plans_nothing() {
        let code = |change: UploadServiceChange, registry: Option<&str>| {
            planned(change, registry, None).unwrap_err().code
        };
        assert_eq!(code(service(), None), "no_registry");
        assert_eq!(code(service(), Some("")), "no_registry");
        let with = |edit: &dyn Fn(&mut UploadServiceChange)| {
            let mut change = service();
            edit(&mut change);
            code(change, Some("zot:5000"))
        };
        assert_eq!(with(&|c| c.name = "Site".to_owned()), "invalid_name");
        assert_eq!(with(&|c| c.request = String::new()), "invalid_request");
        assert_eq!(
            with(&|c| c.directory = Some("../x".to_owned())),
            "invalid_path"
        );
        assert_eq!(
            with(&|c| c.host = Some("a.example.com".to_owned())),
            "host_without_port"
        );
        assert_eq!(
            with(&|c| {
                c.port = Some(80);
                c.host = Some("not a host".to_owned());
            }),
            "invalid_host"
        );
        assert_eq!(
            with(&|c| c.variables = vec![NameValue {
                name: "1A".to_owned(),
                value: String::new()
            }]),
            "invalid_env_name"
        );
    }
}
