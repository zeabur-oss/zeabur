//! The size of the project document on generated namespaces.

mod generated;

#[cfg(test)]
mod tests {
    use zeabur_graph::project;

    use crate::generated::{Distribution, snapshot};

    const MIB: usize = 1 << 20;

    /// The length of the project of `deployments` Deployments in `distribution`, as JSON.
    fn size(distribution: Distribution, deployments: usize) -> usize {
        let snapshot = snapshot(distribution, deployments).unwrap();
        serde_json::to_vec(&project(&snapshot)).unwrap().len()
    }

    /// When every Service selects every Deployment, each Service and Ingress is written once and
    /// the services name theirs. With a copy under every Deployment it selects, 100 Deployments
    /// made a document of about 11 MiB; now it is about 1.4 MiB.
    #[test]
    fn overlapping_selectors_keep_the_document_small() {
        let overlap = size(Distribution::Overlap, 100);
        assert!(overlap < 2 * MIB, "{overlap} bytes");
        let one_to_one = size(Distribution::OneToOne, 100);
        assert!(one_to_one < MIB, "{one_to_one} bytes");
    }
}
