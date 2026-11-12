//! Projection behaviours shown with the fixture cases' operations and inputs, and the checks a
//! case's parameters pass when it loads.

mod ops;

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use k8s_openapi::ByteString;
    use k8s_openapi::api::apps::v1::Deployment;
    use k8s_openapi::api::core::v1::{EnvFromSource, Secret, SecretEnvSource};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
    use serde_json::{Value, from_value, json, to_value};
    use zeabur_graph::{Snapshot, attribution, project};
    use zeabur_model::WorkloadRef;
    use zeabur_test_support::fixtures::{Fixture, compare};
    use zeabur_test_support::load;

    use super::ops;

    fn case(area: &str, name: &str) -> Fixture {
        let mut cases = load!(area).unwrap();
        cases.retain(|fixture| fixture.name == format!("{area}/{name}"));
        cases.pop().unwrap()
    }

    fn with_params(mut fixture: Fixture, params: Value) -> Result<Value, String> {
        fixture.case.params = params;
        ops::run(&fixture)
    }

    #[test]
    fn a_misspelt_extra_or_mistyped_parameter_fails_the_load() {
        let env = case("projection", "env-secret-reference-is-masked");
        assert!(ops::run(&env).is_ok());
        for params in [
            json!({"containr": "app"}),
            json!({"container": "app", "namespace": "demo"}),
            json!({"container": 1}),
            json!({}),
        ] {
            let error = with_params(env.clone(), params.clone()).unwrap_err();
            assert!(
                error.contains("params of projection.env"),
                "{params}: {error}"
            );
        }
        let group = case("graph", "resolve-group-explicit-label-wins");
        let error = with_params(group, json!({"labels": {"zeabur.com/group": 1}})).unwrap_err();
        assert!(error.contains("params of graph.resolveGroup"), "{error}");
        let phase = case("graph", "deployment-phase-fully-rolled-out-is-running");
        let error = with_params(phase, json!({"namespace": "demo"})).unwrap_err();
        assert!(error.contains("params of graph.deploymentPhase"), "{error}");
    }

    /// The claim in `Snapshot`'s documentation: a Secret's values never reach a projected view,
    /// even when a reader leaves them in the snapshot.
    #[test]
    fn secret_values_never_reach_the_projection() {
        let value = "s3cr3t-value";
        let mut snapshot = Snapshot {
            namespace: "demo".to_owned(),
            ..Snapshot::default()
        };
        snapshot.secrets.push(Secret {
            metadata: ObjectMeta {
                name: Some("db".to_owned()),
                namespace: Some("demo".to_owned()),
                ..ObjectMeta::default()
            },
            data: Some(BTreeMap::from([(
                "password".to_owned(),
                ByteString(value.as_bytes().to_vec()),
            )])),
            string_data: Some(BTreeMap::from([("token".to_owned(), value.to_owned())])),
            type_: Some("Opaque".to_owned()),
            ..Secret::default()
        });
        let mut deployment: Deployment = from_value(json!({
            "apiVersion": "apps/v1",
            "kind": "Deployment",
            "metadata": {"name": "api", "namespace": "demo"},
            "spec": {
                "selector": {"matchLabels": {"app": "api"}},
                "template": {
                    "metadata": {"labels": {"app": "api"}},
                    "spec": {
                        "containers": [{
                            "name": "app",
                            "env": [{
                                "name": "PASSWORD",
                                "valueFrom": {"secretKeyRef": {"name": "db", "key": "password"}}
                            }],
                            "volumeMounts": [{"name": "creds", "mountPath": "/creds"}]
                        }],
                        "volumes": [{"name": "creds", "secret": {"secretName": "db"}}]
                    }
                }
            }
        }))
        .unwrap();
        let container = &mut deployment
            .spec
            .as_mut()
            .unwrap()
            .template
            .spec
            .as_mut()
            .unwrap()
            .containers[0];
        container.env_from = Some(vec![EnvFromSource {
            secret_ref: Some(SecretEnvSource {
                name: "db".to_owned(),
                optional: None,
            }),
            ..EnvFromSource::default()
        }]);
        snapshot.deployments.push(deployment);

        let projected = serde_json::to_string(&project(&snapshot)).unwrap();
        assert!(!projected.contains(value), "{projected}");
        assert!(
            !projected.contains("czNjcjN0"),
            "base64 of the value: {projected}"
        );
        assert!(
            !projected.contains("token"),
            "a key name of the Secret: {projected}"
        );
        // The Secret is still attributed: references are read, values are not.
        assert_eq!(
            attribution(&snapshot).secrets["db"].owners,
            vec![WorkloadRef {
                kind: zeabur_model::WorkloadKind::Deployment,
                name: "api".to_owned()
            }]
        );
    }

    /// The project of the attribution fixture: one service per workload, the shared ConfigMap
    /// under both, and the Service as the n8n Deployment's endpoint, listed once in the project.
    #[test]
    fn project_groups_resources_under_workloads() {
        let fixture = load!("graph")
            .unwrap()
            .into_iter()
            .find(|fixture| fixture.name == "graph/attribution-shared-configmap-and-orphan-pvc")
            .unwrap();
        let project = to_value(project(&ops::snapshot(&fixture.input, "demo").unwrap())).unwrap();
        compare(
            &json!({
                "namespace": "demo",
                "services": [
                    {
                        "kind": "Deployment",
                        "name": "n8n",
                        "group": "default",
                        "containers": [{
                            "name": "app",
                            "importedEnv": [{
                                "source": {"kind": "ConfigMap", "name": "app-config"},
                                "keyCount": 0,
                                "readOnly": false,
                                "origin": {
                                    "kind": "Deployment",
                                    "name": "n8n",
                                    "fieldPath": "spec.template.spec.containers[name=app].envFrom[0]"
                                }
                            }]
                        }],
                        "endpointNames": ["n8n"]
                    },
                    {"kind": "Deployment", "name": "worker", "endpointNames": []}
                ],
                "endpoints": [{"name": "n8n", "origin": {"kind": "Service", "name": "n8n"}}],
                "resources": [
                    {"kind": "ConfigMap", "name": "app-config", "attribution": {"ownership": "shared"}},
                    {"kind": "PersistentVolumeClaim", "name": "leftover-data", "attribution": {"ownership": "orphan"}},
                    {"kind": "Service", "name": "n8n", "attribution": {"ownership": "owned"}}
                ]
            }),
            &project,
        )
        .unwrap();
    }
}
