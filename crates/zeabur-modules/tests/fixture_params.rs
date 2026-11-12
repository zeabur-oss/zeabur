//! A case whose parameters do not fit its operation fails when it loads.

mod ops;

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use zeabur_test_support::fixtures::Fixture;
    use zeabur_test_support::load;

    use super::ops;

    fn case(name: &str) -> Fixture {
        let mut cases = load!("modules").unwrap();
        cases.retain(|fixture| fixture.name == format!("modules/{name}"));
        cases.pop().unwrap()
    }

    fn refused(mut fixture: Fixture, params: &Value) {
        let operation = fixture.case.operation.clone();
        fixture.case.params = params.clone();
        let error = ops::run(&fixture).unwrap_err();
        assert!(
            error.contains(&format!("params of {operation}")),
            "{params}: {error}"
        );
    }

    #[test]
    fn a_misspelt_extra_or_mistyped_parameter_fails_the_load() {
        let backup = case("backup-gaps-by-provider");
        assert!(ops::run(&backup).is_ok());
        for params in [
            json!({"namespace": "shop", "installed": {"k8up": true}}),
            json!({"namespace": "shop", "installed": {"k8up": true, "velero": false, "stash": true}}),
            json!({"namespace": "shop", "installed": {"k8up": "yes", "velero": false}}),
            json!({"namespace": "shop", "instaled": {"k8up": true, "velero": false}}),
        ] {
            refused(backup.clone(), &params);
        }
        let network = case("network-gap-blocked-at-ingress");
        assert!(ops::run(&network).is_ok());
        for params in [
            json!({"namespace": "shop", "connections": [["worker"]]}),
            json!({"namespace": "shop", "connection": [["worker", "api"]]}),
        ] {
            refused(network.clone(), &params);
        }
    }
}
