//! A service from an uploaded folder or a Git repository is one change that makes the `Build` and
//! the objects of the service that runs what it builds, the same for the web interface and
//! `zeaburctl up`.
//!
//! Every `Build` Zeabur writes fits the CRD of the Zeabur Builder release in
//! `xtask/pins/zeabur-builder`. `scripts/builder-crds.sh` copies that CRD next to this file; a pin
//! without its copy fails here, so a Builder release that changes the `Build` fails the pull
//! request that moves the pin.

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::{Value, json};
    use zeabur_graph::Snapshot;
    use zeabur_graph::pod_security::level;
    use zeabur_model::{PodSecurityLevel, WorkloadKind, WorkloadRef};
    use zeabur_plan::builds::builds;
    use zeabur_plan::{
        BuildMethod, BuildOptionsChange, Change, DeployKeySecret, Fetched, GitAccess,
        GitServiceChange, GitSourceChange, Input, NameValue, Plan, PlanError, PlanRequest,
        ServedKind, ServiceSize, UploadServiceChange, Write, plan,
    };

    /// The CRDs of the pinned Zeabur Builder, with the release they come from on the first line.
    const CRDS: &str = include_str!("zeabur-builder-crds.yaml");
    const PIN: &str = include_str!("../../../xtask/pins/zeabur-builder");

    /// The schema of a `Build`'s `spec` in the pinned CRDs.
    fn spec_schema() -> Value {
        let crds: Vec<Value> = serde_saphyr::from_multiple(CRDS).unwrap();
        let build = crds
            .into_iter()
            .find(|crd| crd["spec"]["names"]["kind"] == "Build")
            .unwrap();
        let version = build["spec"]["versions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == "v1alpha1")
            .unwrap();
        version["schema"]["openAPIV3Schema"]["properties"]["spec"].clone()
    }

    /// Where `value` breaks `schema`, a structural `openAPIV3Schema` node: its type, required and
    /// unknown fields (which the API server would drop), enums, lengths, patterns, minimums and
    /// items. CEL rules (`x-kubernetes-validations`) are not checked.
    fn breaks(value: &Value, schema: &Value, at: &str, out: &mut Vec<String>) {
        if value.is_null() && schema["nullable"] == true {
            return;
        }
        let kind = schema["type"].as_str().unwrap_or_default();
        let fits = match kind {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "integer" => value.is_i64() || value.is_u64(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            _ => true,
        };
        if !fits {
            out.push(format!("{at}: not a {kind}"));
            return;
        }
        if let Some(allowed) = schema["enum"].as_array()
            && !allowed.contains(value)
        {
            out.push(format!("{at}: {value} is not one of {allowed:?}"));
        }
        if let Some(text) = value.as_str() {
            let length = text.chars().count();
            if schema["maxLength"]
                .as_u64()
                .is_some_and(|max| length as u64 > max)
                || schema["minLength"]
                    .as_u64()
                    .is_some_and(|min| (length as u64) < min)
            {
                out.push(format!("{at}: length {length} is out of range"));
            }
            if let Some(pattern) = schema["pattern"].as_str()
                && !regex::Regex::new(pattern).unwrap().is_match(text)
            {
                out.push(format!("{at}: {text:?} does not match {pattern}"));
            }
        }
        if let (Some(number), Some(min)) = (value.as_f64(), schema["minimum"].as_f64())
            && number < min
        {
            out.push(format!("{at}: {number} is below {min}"));
        }
        if let Some(items) = value.as_array() {
            if schema["maxItems"]
                .as_u64()
                .is_some_and(|max| items.len() as u64 > max)
            {
                out.push(format!("{at}: more than {} items", schema["maxItems"]));
            }
            for (index, item) in items.iter().enumerate() {
                breaks(item, &schema["items"], &format!("{at}[{index}]"), out);
            }
        }
        if let Some(fields) = value.as_object() {
            for required in schema["required"].as_array().into_iter().flatten() {
                let name = required.as_str().unwrap();
                if !fields.contains_key(name) {
                    out.push(format!("{at}.{name}: Required value"));
                }
            }
            for (name, field) in fields {
                let at = format!("{at}.{name}");
                if let Some(property) = schema["properties"].get(name) {
                    breaks(field, property, &at, out);
                } else if schema["additionalProperties"].is_object() {
                    breaks(field, &schema["additionalProperties"], &at, out);
                } else if schema["x-kubernetes-preserve-unknown-fields"] != true {
                    out.push(format!("{at}: unknown field"));
                }
            }
        }
    }

    /// `build` fits the pinned Builder's CRD.
    fn fits_the_crd(build: &Value) {
        let mut out = Vec::new();
        breaks(&build["spec"], &spec_schema(), "spec", &mut out);
        assert!(out.is_empty(), "{out:#?}\n{build:#}");
    }

    /// Build options on `build` as a Build Zeabur Builder serves, applied to it.
    fn with_options(build: &Value, change: BuildOptionsChange) -> Value {
        let snapshot = Snapshot {
            namespace: "shop".to_owned(),
            ..Snapshot::default()
        };
        let mut served = build.clone();
        served["metadata"]["generation"] = json!(1);
        let objects = [Fetched {
            resource: builds(),
            name: served["metadata"]["name"].as_str().unwrap().to_owned(),
            object: served.clone(),
        }];
        let kinds = [kind("builder.zeabur.com", "Build", "builds")];
        let planned = plan(
            Input {
                kinds: &kinds,
                objects: &objects,
                ..Input::new(&snapshot, "2026-10-23T00:00:00Z")
            },
            &PlanRequest {
                changes: vec![Change::BuildOptions(change)],
            },
        )
        .unwrap();
        let Write::Patch { operations } = &planned.objects[0].write else {
            panic!("build options patch the Build");
        };
        let patch: json_patch::Patch = serde_json::from_value(json!(operations)).unwrap();
        json_patch::patch(&mut served, &patch).unwrap();
        served
    }

    /// The copied CRDs are the pinned release's.
    #[test]
    fn the_crds_are_the_pinned_builders() {
        let first = CRDS.lines().next().unwrap();
        assert!(
            first.starts_with(&format!("# zeabur-oss/builder {} ", PIN.trim())),
            "{first:?} is not the CRD of the pinned Zeabur Builder {}: run scripts/builder-crds.sh",
            PIN.trim()
        );
    }

    /// The check finds the shape Zeabur Builder v0.2.4 stopped taking: a source without `type`,
    /// credentials beside `git`, and the context inside `git`.
    #[test]
    fn the_check_refuses_a_source_the_builder_does_not_take() {
        let old = json!({"builderRef": "railpack",
            "source": {"git": {"url": "https://github.com/acme/site.git", "contextDir": "web"},
                       "credentialsRef": {"name": "site-git"}}});
        let mut out = Vec::new();
        breaks(&old, &spec_schema(), "spec", &mut out);
        out.sort();
        assert_eq!(
            out,
            [
                "spec.source.credentialsRef: unknown field",
                "spec.source.git.contextDir: unknown field",
                "spec.source.type: Required value",
            ]
        );
    }

    /// Every Build in the fixtures, which stand for what Zeabur Builder serves, fits its CRD.
    #[test]
    fn every_build_in_the_fixtures_fits_the_crd() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/mutation");
        let mut seen = 0;
        for case in std::fs::read_dir(root).unwrap() {
            let input = case.unwrap().path().join("input.yaml");
            let Ok(text) = std::fs::read_to_string(&input) else {
                continue;
            };
            let objects: Vec<Value> = serde_saphyr::from_multiple(&text).unwrap();
            for build in objects.iter().filter(|o| o["kind"] == "Build") {
                fits_the_crd(build);
                seen += 1;
            }
        }
        assert!(seen > 0);
    }

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
            method: BuildMethod::Railpack,
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
        planned_change(Change::CreateUploadService(change), registry, issuer)
    }

    fn planned_change(
        change: Change,
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
                changes: vec![change],
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
        assert_eq!(
            build["spec"]["source"],
            json!({"type": "archive", "archive": {}})
        );
        fits_the_crd(&build);
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
        fits_the_crd(&build);
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
        docker.method = BuildMethod::Dockerfile;
        docker.directory = Some("./apps/web/".to_owned());
        docker.dockerfile = Some("docker/Dockerfile.prod".to_owned());
        assert_eq!(level_of(docker.clone()), PodSecurityLevel::Baseline);
        docker.security = Some(PodSecurityLevel::Restricted);
        docker.size = Some(ServiceSize::Large);
        assert_eq!(level_of(docker.clone()), PodSecurityLevel::Restricted);
        let build = created(&planned(docker, Some("zot:5000"), None).unwrap(), "Build").unwrap();
        assert_eq!(
            build["spec"]["source"],
            json!({"type": "archive", "archive": {},
                   "contextDir": "apps/web", "dockerfile": "docker/Dockerfile.prod"})
        );
        assert_eq!(build["spec"]["builderRef"], "dockerfile");
        fits_the_crd(&build);
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
    }

    /// `security: privileged` gives no privileged settings: the pod is at least baseline.
    #[test]
    fn a_privileged_choice_plans_no_privileged_pod() {
        let mut change = service();
        change.method = BuildMethod::Dockerfile;
        change.security = Some(PodSecurityLevel::Privileged);
        let plan = planned(change, Some("zot:5000"), None).unwrap();
        let deployment = created(&plan, "Deployment").unwrap();
        let pod = &deployment["spec"]["template"]["spec"];
        assert_eq!(
            level(&serde_json::from_value(pod.clone()).unwrap()),
            PodSecurityLevel::Baseline
        );
        assert_eq!(
            pod["containers"][0]["securityContext"]["allowPrivilegeEscalation"],
            false
        );
        assert!(pod["containers"][0]["securityContext"]["privileged"].is_null());
    }

    /// A host with a wildcard or upper case, a Dockerfile path that leaves the folder and a
    /// build variable name the shell cannot read are refused.
    #[test]
    fn unsafe_hosts_paths_and_build_variables_are_refused() {
        let code = |edit: &dyn Fn(&mut UploadServiceChange)| {
            let mut change = service();
            change.method = BuildMethod::Dockerfile;
            change.port = Some(80);
            edit(&mut change);
            planned(change, Some("zot:5000"), None).unwrap_err().code
        };
        for host in ["*.example.com", "Site.example.com"] {
            assert_eq!(
                code(&|c| c.host = Some(host.to_owned())),
                "invalid_host",
                "{host}"
            );
        }
        for path in ["../Dockerfile", "docker/../../Dockerfile"] {
            assert_eq!(
                code(&|c| c.dockerfile = Some(path.to_owned())),
                "invalid_path",
                "{path}"
            );
        }
        assert_eq!(
            code(&|c| c.build_variables = vec![NameValue {
                name: "NPM-TOKEN".to_owned(),
                value: String::new()
            }]),
            "invalid_env_name"
        );
    }

    fn git() -> GitServiceChange {
        GitServiceChange {
            name: "site".to_owned(),
            url: "https://github.com/acme/site".to_owned(),
            branch: None,
            method: BuildMethod::Railpack,
            directory: None,
            dockerfile: None,
            port: Some(8080),
            host: None,
            variables: Vec::new(),
            build_variables: Vec::new(),
            size: None,
            security: None,
            access: GitAccess::Public,
            deploy_key: None,
        }
    }

    fn git_plan(change: GitServiceChange) -> Result<Plan, PlanError> {
        planned_change(Change::CreateGitService(change), Some("zot:5000"), None)
    }

    /// A public repository is read over HTTPS from its default branch, with no Secret; the
    /// Deployment runs what the `Build` pushes and Keel moves it.
    #[test]
    fn a_public_repository_builds_from_its_default_branch() {
        let plan = git_plan(git()).unwrap();
        let build = created(&plan, "Build").unwrap();
        assert_eq!(
            build["spec"]["source"],
            json!({"type": "git", "git": {"url": "https://github.com/acme/site.git", "ref": "HEAD"}})
        );
        fits_the_crd(&build);
        assert_eq!(build["spec"]["notify"][0]["type"], "keel");
        let deployment = created(&plan, "Deployment").unwrap();
        assert_eq!(
            deployment["spec"]["template"]["spec"]["containers"][0]["image"],
            "zot:5000/shop/site:pending"
        );
        let mut branch = git();
        branch.branch = Some(" release/1.4 ".to_owned());
        let build = created(&git_plan(branch).unwrap(), "Build").unwrap();
        assert_eq!(build["spec"]["source"]["git"]["ref"], "release/1.4");
        fits_the_crd(&build);
    }

    /// A token goes in a basic-auth Secret `<name>-git` that Zeabur Builder reads by its label,
    /// and the `Build` names it.
    #[test]
    fn a_token_goes_in_a_secret_the_build_names() {
        let mut change = git();
        change.access = GitAccess::Token {
            username: "x-access-token".to_owned(),
            token: " ghp_1 ".to_owned(),
        };
        let plan = git_plan(change).unwrap();
        let secret = plan
            .objects
            .iter()
            .find(|o| o.target.name == "site-git")
            .unwrap();
        let Write::Create { body } = &secret.write else {
            panic!("the Secret is created");
        };
        assert_eq!(body["type"], "kubernetes.io/basic-auth");
        assert_eq!(
            body["metadata"]["labels"]["builder.zeabur.com/git-credentials"],
            "true"
        );
        assert_eq!(body["stringData"]["password"], "ghp_1");
        let build = created(&plan, "Build").unwrap();
        assert_eq!(
            build["spec"]["source"]["git"]["credentialsRef"]["name"],
            "site-git"
        );
        fits_the_crd(&build);
    }

    /// A deploy key reads the repository over SSH, from the Secret the API made.
    #[test]
    fn a_deploy_key_reads_over_ssh() {
        let mut change = git();
        change.access = GitAccess::DeployKey { known_hosts: None };
        assert_eq!(
            git_plan(change.clone()).unwrap_err().code,
            "unresolved_deploy_key"
        );
        change.deploy_key = Some(DeployKeySecret {
            private_key: "KEY".to_owned(),
            known_hosts: "github.com ssh-ed25519 AAAA\n".to_owned(),
        });
        let plan = git_plan(change).unwrap();
        let build = created(&plan, "Build").unwrap();
        assert_eq!(
            build["spec"]["source"]["git"]["url"],
            "git@github.com:acme/site.git"
        );
        assert_eq!(
            build["spec"]["source"]["git"]["credentialsRef"]["name"],
            "site-git"
        );
        fits_the_crd(&build);
        let secret = created(&plan, "Secret").unwrap();
        assert_eq!(secret["type"], "kubernetes.io/ssh-auth");
    }

    /// A repository the GitHub App reads names its installation and no Secret.
    #[test]
    fn the_github_app_names_its_installation() {
        let mut change = git();
        change.access = GitAccess::GithubApp {
            installation_id: 42,
        };
        let plan = git_plan(change).unwrap();
        let build = created(&plan, "Build").unwrap();
        assert_eq!(
            build["spec"]["source"]["git"]["githubApp"],
            json!({"installationId": 42})
        );
        fits_the_crd(&build);
        assert!(created(&plan, "Secret").is_none_or(|s| s["metadata"]["name"] != "site-git"));
    }

    /// Build options set the directory and Dockerfile on `spec.source` for both sources, and
    /// remove them again, and the Build still fits the CRD.
    #[test]
    fn build_options_keep_the_build_in_the_crd() {
        let mut docker = git();
        docker.method = BuildMethod::Dockerfile;
        let mut upload = service();
        upload.method = BuildMethod::Dockerfile;
        for build in [
            created(&git_plan(docker).unwrap(), "Build").unwrap(),
            created(&planned(upload, Some("zot:5000"), None).unwrap(), "Build").unwrap(),
        ] {
            let set = with_options(
                &build,
                BuildOptionsChange {
                    build: "site".to_owned(),
                    context_dir: Some("apps/web".to_owned()),
                    dockerfile: Some("Dockerfile.prod".to_owned()),
                    env: Vec::new(),
                },
            );
            assert_eq!(set["spec"]["source"]["contextDir"], "apps/web");
            assert_eq!(set["spec"]["source"]["dockerfile"], "Dockerfile.prod");
            fits_the_crd(&set);
            let cleared = with_options(
                &set,
                BuildOptionsChange {
                    build: "site".to_owned(),
                    context_dir: Some(String::new()),
                    dockerfile: Some(String::new()),
                    env: Vec::new(),
                },
            );
            assert_eq!(cleared["spec"]["source"], build["spec"]["source"]);
            fits_the_crd(&cleared);
        }
    }

    fn running(image: &str, keel: bool) -> Value {
        let annotations = if keel {
            json!({"keel.sh/policy": "force"})
        } else {
            json!({})
        };
        json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": "site", "namespace": "shop", "resourceVersion": "3",
                "generation": 1, "annotations": annotations},
            "spec": {"selector": {"matchLabels": {"app": "site"}}, "template": {
                "metadata": {"labels": {"app": "site"}},
                "spec": {"containers": [{"name": "site", "image": image}]}}}
        })
    }

    fn source() -> GitSourceChange {
        GitSourceChange {
            workload: WorkloadRef {
                kind: WorkloadKind::Deployment,
                name: "site".to_owned(),
            },
            container: None,
            url: "https://github.com/acme/site".to_owned(),
            branch: None,
            method: BuildMethod::Railpack,
            directory: Some("web".to_owned()),
            dockerfile: None,
            access: GitAccess::Public,
            deploy_key: None,
        }
    }

    /// A `gitSource` on `deployment`, with `build` as the `Build` of its name if it has one.
    fn source_plan(
        change: GitSourceChange,
        deployment: Value,
        build: Option<Value>,
    ) -> Result<Plan, PlanError> {
        #[expect(clippy::unwrap_in_result, reason = "a test fixture")]
        let snapshot = Snapshot {
            namespace: "shop".to_owned(),
            deployments: vec![serde_json::from_value(deployment).unwrap()],
            ..Snapshot::default()
        };
        let objects: Vec<Fetched> = build
            .into_iter()
            .map(|object| Fetched {
                resource: builds(),
                name: "site".to_owned(),
                object,
            })
            .collect();
        let kinds = [
            kind("", "Secret", "secrets"),
            kind("apps", "Deployment", "deployments"),
            kind("builder.zeabur.com", "Build", "builds"),
        ];
        plan(
            Input {
                kinds: &kinds,
                objects: &objects,
                build_registry: Some("zot:5000"),
                ..Input::new(&snapshot, "2026-11-26T00:00:00Z")
            },
            &PlanRequest {
                changes: vec![Change::GitSource(change)],
            },
        )
    }

    fn after<'a>(plan: &'a Plan, kind: &str) -> Option<&'a Value> {
        plan.objects
            .iter()
            .find(|o| o.target.kind == kind)
            .map(|o| &o.raw_after)
    }

    /// An image service gets a `Build` of the repository, and its container moves to what the
    /// `Build` pushes, which Keel updates, as for a new Git service.
    #[test]
    fn an_image_service_gets_a_build_and_runs_what_it_pushes() {
        let plan = source_plan(source(), running("nginx:1.27", false), None).unwrap();
        let build = created(&plan, "Build").unwrap();
        assert_eq!(
            build["spec"]["source"],
            json!({"type": "git", "git": {"url": "https://github.com/acme/site.git", "ref": "HEAD"},
                   "contextDir": "web"})
        );
        assert_eq!(build["spec"]["output"]["image"], "zot:5000/shop/site");
        assert_eq!(build["spec"]["notify"][0]["type"], "keel");
        fits_the_crd(&build);
        let deployment = after(&plan, "Deployment").unwrap();
        assert_eq!(
            deployment["spec"]["template"]["spec"]["containers"][0]["image"],
            "zot:5000/shop/site:pending"
        );
        assert_eq!(
            deployment["metadata"]["annotations"]["keel.sh/policy"],
            "force"
        );
    }

    /// An uploaded folder's `Build` builds the repository instead, with the credentials it is
    /// given; the service already runs what that `Build` pushes, so it is left as it is.
    #[test]
    fn an_upload_service_builds_the_repository_instead() {
        let mut upload = service();
        upload.directory = Some("old".to_owned());
        let mut build =
            created(&planned(upload, Some("zot:5000"), None).unwrap(), "Build").unwrap();
        build["metadata"]["generation"] = json!(1);
        let mut change = source();
        change.method = BuildMethod::Dockerfile;
        change.dockerfile = Some("Dockerfile.prod".to_owned());
        change.access = GitAccess::Token {
            username: "x-access-token".to_owned(),
            token: "ghp_1".to_owned(),
        };
        let image = "zot:5000/shop/site:main-1@sha256:".to_owned() + &"a".repeat(64);
        let plan = source_plan(change, running(&image, true), Some(build)).unwrap();
        let written = plan.objects.iter().find(|o| o.target.kind == "Build");
        assert!(written.is_some_and(|o| matches!(o.write, Write::Patch { .. })));
        let build = after(&plan, "Build").unwrap();
        assert_eq!(
            build["spec"]["source"],
            json!({"type": "git",
                   "git": {"url": "https://github.com/acme/site.git", "ref": "HEAD",
                           "credentialsRef": {"name": "site-git"}},
                   "contextDir": "web", "dockerfile": "Dockerfile.prod"})
        );
        assert_eq!(build["spec"]["builderRef"], "dockerfile");
        fits_the_crd(build);
        let secret = created(&plan, "Secret").unwrap();
        assert_eq!(secret["metadata"]["name"], "site-git");
        assert!(after(&plan, "Deployment").is_none());
    }

    /// A `Build` that already reads a repository is changed in its own settings.
    #[test]
    fn a_build_from_git_is_not_given_another_source() {
        let build = created(&git_plan(git()).unwrap(), "Build").unwrap();
        let refused = source_plan(
            source(),
            running("zot:5000/shop/site:pending", true),
            Some(build),
        )
        .unwrap_err();
        assert_eq!(refused.code, "not_an_upload");
    }

    /// The form's rules for a repository and a branch are the server's.
    #[test]
    fn a_repository_or_branch_git_would_refuse_plans_nothing() {
        let code = |edit: &dyn Fn(&mut GitServiceChange)| {
            let mut change = git();
            edit(&mut change);
            git_plan(change).unwrap_err().code
        };
        assert_eq!(
            code(&|c| c.url = "github.com/acme".to_owned()),
            "invalid_repository"
        );
        for branch in ["a..b", "main.lock", "/main"] {
            assert_eq!(
                code(&|c| c.branch = Some(branch.to_owned())),
                "invalid_branch",
                "{branch}"
            );
        }
        assert_eq!(
            code(&|c| c.access = GitAccess::Token {
                username: String::new(),
                token: "t".to_owned()
            }),
            "invalid_token"
        );
    }
}
