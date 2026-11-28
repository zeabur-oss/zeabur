//! The write contract (`contract/mod.rs`) against the in-memory cluster the API tests use. The
//! same behaviours run against k3s in `kubernetes.rs`.

#[cfg(test)]
mod contract;

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Cluster as _, ObjectTarget, WriteBody, WriteRequest};

    use super::contract;

    const NAMESPACE: &str = "shop";

    fn cluster() -> MemoryCluster {
        MemoryCluster::new(State::default())
    }

    /// The in-memory cluster returns the warnings a test sets for an object with each write of
    /// it, as kube-apiserver returns its `Warning` headers.
    #[tokio::test]
    async fn a_write_returns_the_warnings_set_for_its_object() {
        let warning = "would violate PodSecurity \"restricted:latest\": runAsNonRoot != true";
        let cluster = MemoryCluster::new(State {
            write_warnings: BTreeMap::from([(
                "ConfigMap/settings".to_owned(),
                vec![warning.to_owned()],
            )]),
            ..State::default()
        });
        let target = |name: &str| ObjectTarget {
            api_version: "v1".to_owned(),
            kind: "ConfigMap".to_owned(),
            plural: "configmaps".to_owned(),
            namespace: NAMESPACE.to_owned(),
            name: name.to_owned(),
        };
        let create = |name: &str, dry_run| WriteRequest {
            target: target(name),
            body: WriteBody::Create(json!({
                "apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": name}
            })),
            dry_run,
        };
        for dry_run in [true, false] {
            let written = cluster.write(&create("settings", dry_run)).await.unwrap();
            assert_eq!(written.warnings, [warning], "dry run: {dry_run}");
        }
        let other = cluster.write(&create("other", false)).await.unwrap();
        assert_eq!(other.warnings, Vec::<String>::new());
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
    async fn a_secret_read_generically_keeps_its_keys_but_not_its_values() {
        contract::a_secret_read_generically_keeps_its_keys_but_not_its_values(
            &cluster(),
            NAMESPACE,
        )
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
