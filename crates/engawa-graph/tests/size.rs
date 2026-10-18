//! The size of the project document on generated namespaces.

mod generated;

#[cfg(test)]
mod tests {
    use engawa_graph::project;

    use crate::generated::{Distribution, snapshot};

    const MIB: usize = 1 << 20;

    /// The length of the project of `deployments` Deployments in `distribution`, as JSON.
    fn size(distribution: Distribution, deployments: usize) -> usize {
        let snapshot = snapshot(distribution, deployments).unwrap();
        serde_json::to_vec(&project(&snapshot)).unwrap().len()
    }

    /// When every Service selects every Deployment, an endpoint or route does not repeat the
    /// owners of its object for each workload it is listed under. With that copy, 100
    /// Deployments made a document of about 90 MiB.
    #[test]
    fn overlapping_selectors_keep_the_document_small() {
        let overlap = size(Distribution::Overlap, 100);
        assert!(overlap < 16 * MIB, "{overlap} bytes");
        let one_to_one = size(Distribution::OneToOne, 100);
        assert!(one_to_one < MIB, "{one_to_one} bytes");
    }
}
