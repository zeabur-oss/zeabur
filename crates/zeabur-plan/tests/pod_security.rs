//! Pod Security: a service's profile writes or removes the restricted settings on its pod
//! template.

#[cfg(test)]
mod tests {
    use serde_json::{Value, from_value, json};
    use zeabur_graph::Snapshot;
    use zeabur_graph::pod_security::level;
    use zeabur_model::PodSecurityLevel::{Baseline, Privileged, Restricted};
    use zeabur_model::{PodSecurityLevel, WorkloadKind, WorkloadRef};
    use zeabur_plan::json::apply_patch;
    use zeabur_plan::{Change, Input, Plan, PlanError, PlanRequest, Write, plan};

    fn deployment(name: &str, spec: &Value) -> Value {
        json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": name, "namespace": "shop", "resourceVersion": "3", "generation": 1},
            "spec": {"selector": {"matchLabels": {"app": name}}, "template": {
                "metadata": {"labels": {"app": name}}, "spec": spec}}
        })
    }

    fn nginx() -> Value {
        deployment(
            "web",
            &json!({"containers": [{"name": "web", "image": "nginx"}]}),
        )
    }

    fn web() -> WorkloadRef {
        WorkloadRef {
            kind: WorkloadKind::Deployment,
            name: "web".to_owned(),
        }
    }

    fn profile(profile: PodSecurityLevel) -> Change {
        Change::SecurityProfile {
            workload: web(),
            profile,
        }
    }

    fn planned(workloads: &[Value], changes: Vec<Change>) -> Result<Plan, PlanError> {
        let snapshot = Snapshot {
            namespace: "shop".to_owned(),
            deployments: workloads
                .iter()
                .map(|w| from_value(w.clone()).unwrap())
                .collect(),
            ..Snapshot::default()
        };
        plan(
            Input::new(&snapshot, "2026-10-23T00:00:00Z"),
            &PlanRequest { changes },
        )
    }

    fn written(plan: &Plan, stored: &Value, kind: &str) -> Value {
        let object = plan.objects.iter().find(|o| o.target.kind == kind).unwrap();
        let Write::Patch { operations } = &object.write else {
            panic!("{kind} is patched");
        };
        let after = apply_patch(stored, operations).unwrap();
        assert_eq!(after, object.raw_after);
        after
    }

    fn spec_level(deployment: &Value) -> PodSecurityLevel {
        level(&from_value(deployment["spec"]["template"]["spec"].clone()).unwrap())
    }

    #[test]
    fn restricted_makes_the_template_meet_the_level_and_baseline_lets_root_run_again() {
        let stored = nginx();
        let plan = planned(std::slice::from_ref(&stored), vec![profile(Restricted)]).unwrap();
        let strict = written(&plan, &stored, "Deployment");
        assert_eq!(spec_level(&strict), Restricted);
        assert_eq!(plan.impact, vec![web()]);
        let container = &strict["spec"]["template"]["spec"]["containers"][0];
        assert_eq!(
            container["securityContext"],
            json!({"allowPrivilegeEscalation": false, "capabilities": {"drop": ["ALL"]}})
        );
        let again = planned(std::slice::from_ref(&strict), vec![profile(Restricted)]).unwrap_err();
        assert_eq!(again.code, "unchanged");

        let plan = planned(std::slice::from_ref(&strict), vec![profile(Baseline)]).unwrap();
        let compatible = written(&plan, &strict, "Deployment");
        assert_eq!(spec_level(&compatible), Baseline);
        let pod = &compatible["spec"]["template"]["spec"];
        assert!(
            pod["securityContext"].get("runAsNonRoot").is_none(),
            "{pod}"
        );
        assert_eq!(
            pod["containers"][0]["securityContext"],
            json!({"allowPrivilegeEscalation": false})
        );
    }

    #[test]
    fn restricted_is_refused_where_the_template_itself_asks_for_root() {
        let root = deployment(
            "web",
            &json!({"containers": [{"name": "web", "image": "nginx", "securityContext": {"runAsUser": 0}}]}),
        );
        let refused = planned(&[root], vec![profile(Restricted)]).unwrap_err();
        assert_eq!(refused.code, "not_restricted");
        assert!(refused.detail.contains("runAsUser"), "{}", refused.detail);
        let privileged = planned(&[nginx()], vec![profile(Privileged)]).unwrap_err();
        assert_eq!(privileged.code, "unknown_profile");
    }
}
