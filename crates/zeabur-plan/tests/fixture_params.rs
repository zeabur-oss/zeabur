//! A case whose parameters do not fit its operation fails when it loads.

mod ops;

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use zeabur_test_support::fixtures::Fixture;
    use zeabur_test_support::load;

    use super::ops;

    fn case(name: &str) -> Fixture {
        let mut cases = load!("mutation").unwrap();
        cases.retain(|fixture| fixture.name == format!("mutation/{name}"));
        cases.pop().unwrap()
    }

    fn with_params(mut fixture: Fixture, params: Value) -> Result<Value, String> {
        fixture.case.params = params;
        ops::run(&fixture)
    }

    #[test]
    fn a_misspelt_extra_or_mistyped_parameter_fails_the_load() {
        let plan = case("plan-env-set-new-variable");
        assert!(ops::run(&plan).is_ok());
        let request = plan.case.params["request"].clone();
        let mut nested = request.clone();
        nested["changes"][0]["changes"][0]["valu"] = json!("production");
        for params in [
            json!({"requst": request}),
            json!({"request": request, "reloadr": true}),
            json!({"request": request, "reloader": "yes"}),
            json!({"request": nested}),
        ] {
            let error = with_params(plan.clone(), params.clone()).unwrap_err();
            assert!(
                error.contains("params of mutation.plan"),
                "{params}: {error}"
            );
        }
    }
}
