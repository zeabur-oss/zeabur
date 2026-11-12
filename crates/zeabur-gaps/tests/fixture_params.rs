//! A case whose parameters do not fit its operation fails when it loads.

mod ops;

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use zeabur_test_support::fixtures::Fixture;
    use zeabur_test_support::load;

    use super::ops;

    fn case(name: &str) -> Fixture {
        let mut cases = load!("gaps").unwrap();
        cases.retain(|fixture| fixture.name == format!("gaps/{name}"));
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
        let diagnose = case("diagnose-unreachable-no-service-services-unreadable");
        assert!(ops::run(&diagnose).is_ok());
        let now = "2026-10-15T12:00:00Z";
        for params in [
            json!({"now": now, "servicesReadabel": false}),
            json!({"now": now, "servicesReadable": false, "rules": []}),
            json!({"now": now, "servicesReadable": "no"}),
            json!({"now": "noon", "servicesReadable": false}),
            json!({"servicesReadable": false}),
        ] {
            refused(diagnose.clone(), &params);
        }
        let rules = case("rules-dangling-route-missing-gateway");
        assert!(ops::run(&rules).is_ok());
        for params in [
            json!({"rules": ["dangling-route"], "gateways": [{"namespace": "infra", "nam": "public"}]}),
            json!({"rules": "dangling-route"}),
            json!({"rules": ["dangling-route"], "retiredIngressClass": ["nginx"]}),
        ] {
            refused(rules.clone(), &params);
        }
    }
}
