//! Pod Security: a service's profile writes or removes the restricted settings on its pod
//! template, and the project's level moves only where it is safe: lowered from `restricted`
//! before the pods that need it, raised only when every workload meets it.

#[cfg(test)]
mod tests {
    use serde_json::{Value, from_value, json};
    use zeabur_graph::Snapshot;
    use zeabur_graph::pod_security::level;
    use zeabur_model::PodSecurityLevel::{Baseline, Privileged, Restricted};
    use zeabur_model::{PodSecurityLevel, WorkloadKind, WorkloadRef};
    use zeabur_plan::json::apply_patch;
    use zeabur_plan::{Change, Input, Plan, PlanError, PlanRequest, Warning, Write, plan};

    const ENFORCE: &str = "pod-security.kubernetes.io/enforce";

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

    fn namespace(enforce: Option<&str>) -> Value {
        let mut labels = json!({"zeabur.com/project": "true"});
        if let Some(enforce) = enforce {
            labels[ENFORCE] = json!(enforce);
            labels["pod-security.kubernetes.io/warn"] = json!("restricted");
        }
        json!({"apiVersion": "v1", "kind": "Namespace",
               "metadata": {"name": "shop", "resourceVersion": "7", "labels": labels}})
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

    fn planned(
        workloads: &[Value],
        namespace: Option<&Value>,
        changes: Vec<Change>,
    ) -> Result<Plan, PlanError> {
        let snapshot = Snapshot {
            namespace: "shop".to_owned(),
            deployments: workloads
                .iter()
                .map(|w| from_value(w.clone()).unwrap())
                .collect(),
            ..Snapshot::default()
        };
        plan(
            Input {
                namespace,
                ..Input::new(&snapshot, "2026-10-23T00:00:00Z")
            },
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
        let plan = planned(
            std::slice::from_ref(&stored),
            None,
            vec![profile(Restricted)],
        )
        .unwrap();
        let strict = written(&plan, &stored, "Deployment");
        assert_eq!(spec_level(&strict), Restricted);
        assert_eq!(plan.impact, vec![web()]);
        let container = &strict["spec"]["template"]["spec"]["containers"][0];
        assert_eq!(
            container["securityContext"],
            json!({"allowPrivilegeEscalation": false, "capabilities": {"drop": ["ALL"]}})
        );
        let again = planned(
            std::slice::from_ref(&strict),
            None,
            vec![profile(Restricted)],
        )
        .unwrap_err();
        assert_eq!(again.code, "unchanged");

        let plan = planned(std::slice::from_ref(&strict), None, vec![profile(Baseline)]).unwrap();
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
        let refused = planned(&[root], None, vec![profile(Restricted)]).unwrap_err();
        assert_eq!(refused.code, "not_restricted");
        assert!(refused.detail.contains("runAsUser"), "{}", refused.detail);
        let privileged = planned(&[nginx()], None, vec![profile(Privileged)]).unwrap_err();
        assert_eq!(privileged.code, "unknown_profile");
    }

    #[test]
    fn baseline_lowers_a_restricted_project_first_and_leaves_any_other_label_alone() {
        let stored = namespace(Some("restricted"));
        let plan = planned(
            &[nginx()],
            Some(&stored),
            vec![profile(Baseline), Change::PodSecurity { level: Baseline }],
        )
        .unwrap_err();
        // nginx already has no restricted settings to remove.
        assert_eq!(plan.code, "unchanged");

        let plan = planned(
            &[nginx()],
            Some(&stored),
            vec![Change::PodSecurity { level: Baseline }],
        )
        .unwrap();
        assert_eq!(plan.objects[0].target.kind, "Namespace");
        let after = written(&plan, &stored, "Namespace");
        assert_eq!(after["metadata"]["labels"][ENFORCE], "baseline");
        assert_eq!(
            after["metadata"]["labels"]["pod-security.kubernetes.io/warn"],
            "restricted"
        );

        let strict = written(
            &planned(&[nginx()], None, vec![profile(Restricted)]).unwrap(),
            &nginx(),
            "Deployment",
        );
        let both = planned(
            &[strict],
            Some(&stored),
            vec![profile(Baseline), Change::PodSecurity { level: Baseline }],
        )
        .unwrap();
        let kinds: Vec<&str> = both
            .objects
            .iter()
            .map(|o| o.target.kind.as_str())
            .collect();
        assert_eq!(kinds, ["Namespace", "Deployment"]);

        for unlabelled in [namespace(None), namespace(Some("baseline"))] {
            let nothing = planned(
                &[nginx()],
                Some(&unlabelled),
                vec![Change::PodSecurity { level: Baseline }],
            )
            .unwrap_err();
            assert_eq!(nothing.code, "unchanged");
        }
    }

    #[test]
    fn restricted_is_raised_only_when_every_workload_meets_it() {
        let stored = namespace(Some("baseline"));
        let refused = planned(
            &[nginx()],
            Some(&stored),
            vec![Change::PodSecurity { level: Restricted }],
        )
        .unwrap_err();
        assert_eq!(refused.code, "pod_security_unmet");
        assert_eq!(refused.change, Some(0));
        assert!(refused.detail.contains("web"), "{}", refused.detail);

        let plan = planned(
            &[nginx()],
            Some(&stored),
            vec![
                profile(Restricted),
                Change::PodSecurity { level: Restricted },
            ],
        )
        .unwrap();
        let after = written(&plan, &stored, "Namespace");
        assert_eq!(after["metadata"]["labels"][ENFORCE], "restricted");
    }

    #[test]
    fn the_last_service_made_restricted_says_the_project_could_be_too() {
        let stored = namespace(Some("baseline"));
        let plan = planned(&[nginx()], Some(&stored), vec![profile(Restricted)]).unwrap();
        assert_eq!(
            plan.warnings,
            vec![Warning::ProjectCanBeRestricted { level: Baseline }]
        );
        assert_eq!(plan.objects.len(), 1);
        let other = deployment(
            "worker",
            &json!({"containers": [{"name": "w", "image": "w"}]}),
        );
        let plan = planned(&[nginx(), other], Some(&stored), vec![profile(Restricted)]).unwrap();
        assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
        let unlabelled = namespace(None);
        let plan = planned(&[nginx()], Some(&unlabelled), vec![profile(Restricted)]).unwrap();
        assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
    }
}
