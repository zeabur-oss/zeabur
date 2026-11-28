//! Who reapplies the fields they own; the ownership itself is read with
//! [`zeabur_graph::fields`].

pub use zeabur_graph::fields::{Step, owners};
pub use zeabur_model::adapters::provenance::reapplying_tool;

#[cfg(test)]
mod tests {
    use zeabur_model::Tool;

    use super::*;

    #[test]
    fn tools_that_reapply_are_known_by_their_manager() {
        assert_eq!(reapplying_tool("argocd-controller"), Some(Tool::ArgoCd));
        assert_eq!(reapplying_tool("kubectl-client-side-apply"), None);
    }
}
