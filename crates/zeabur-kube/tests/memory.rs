//! The write contract (`contract/mod.rs`) against the in-memory cluster the API tests use. The
//! same behaviours run against k3s in `kubernetes.rs`.

#[cfg(test)]
mod contract;

#[cfg(test)]
mod tests {
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{AccessRule, AccessRules};

    use super::contract;

    const NAMESPACE: &str = "shop";

    fn cluster() -> MemoryCluster {
        MemoryCluster::new(State::default())
    }

    /// The person of the contract: ConfigMaps in full, Deployments to get, create and patch, in
    /// the namespace only.
    fn bound() -> MemoryCluster {
        let rule = |group: &str, resource: &str, verbs: &[&str]| AccessRule {
            verbs: verbs.iter().map(|&v| v.to_owned()).collect(),
            groups: vec![group.to_owned()],
            resources: vec![resource.to_owned()],
            names: Vec::new(),
        };
        let mut state = State::default();
        state.refuse_cluster_wide();
        state.bind(
            NAMESPACE,
            AccessRules {
                rules: vec![
                    rule(
                        "",
                        "configmaps",
                        &["get", "list", "create", "patch", "delete"],
                    ),
                    rule("apps", "deployments", &["get", "create", "patch"]),
                ],
                incomplete: false,
            },
        );
        MemoryCluster::new(state)
    }

    #[tokio::test]
    async fn the_rules_decide_reviews_writes_and_dry_runs_alike() {
        contract::the_rules_decide_reviews_writes_and_dry_runs_alike(&bound(), NAMESPACE).await;
    }

    #[tokio::test]
    async fn a_patch_whose_tests_hold_is_stored_as_computed() {
        contract::a_patch_whose_tests_hold_is_stored_as_computed(&cluster(), NAMESPACE).await;
    }

    #[tokio::test]
    async fn a_test_that_fails_refuses_the_write_and_stores_nothing() {
        contract::a_test_that_fails_refuses_the_write_and_stores_nothing(&cluster(), NAMESPACE)
            .await;
    }

    #[tokio::test]
    async fn a_taken_name_is_refused_and_a_missing_object_is_not_found() {
        contract::a_taken_name_is_refused_and_a_missing_object_is_not_found(&cluster(), NAMESPACE)
            .await;
    }

    #[tokio::test]
    async fn a_dry_run_validates_strictly_and_stores_nothing() {
        contract::a_dry_run_validates_strictly_and_stores_nothing(&cluster(), NAMESPACE).await;
    }

    #[tokio::test]
    async fn of_concurrent_patches_of_one_version_exactly_one_is_stored() {
        contract::of_concurrent_patches_of_one_version_exactly_one_is_stored(&cluster(), NAMESPACE)
            .await;
    }

    #[tokio::test]
    async fn a_deletion_keeps_to_its_preconditions() {
        contract::a_deletion_keeps_to_its_preconditions(&cluster(), NAMESPACE).await;
    }
}
