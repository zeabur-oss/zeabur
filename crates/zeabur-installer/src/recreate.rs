//! Deleting the objects an upgrade recreates ([`zeabur_installer_plan::recreate`]) before Helm
//! upgrades the release.

use zeabur_installer_host::kubectl::Kubectl;
use zeabur_installer_plan::catalog::Recreate;

use crate::{InstallError, out};

/// Deletes each object with `--cascade=orphan`, so its Pods and volumes stay and the upgrade
/// creates it again.
pub(crate) fn delete(kubectl: &Kubectl, objects: &[Recreate]) -> Result<(), InstallError> {
    for object in objects {
        out::line(&format!(
            "    recreating {} {}/{}; what it created stays",
            object.kind, object.namespace, object.name
        ));
        let kind = object.kind.to_ascii_lowercase();
        kubectl.run(
            &[
                "delete",
                &kind,
                &object.name,
                "--namespace",
                &object.namespace,
                "--cascade=orphan",
                "--ignore-not-found",
            ],
            None,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    /// A `kubectl` that logs its arguments.
    const FAKE: &str = r#"#!/bin/sh
dir=$(dirname "${1#--kubeconfig=}")
shift
echo "$*" >> "$dir/calls"
"#;

    #[test]
    fn a_listed_stateful_set_is_deleted_without_its_pods_and_volumes() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("kubectl");
        fs::write(&binary, FAKE).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        let kubectl = Kubectl::new(&binary, dir.path().join("kubeconfig"));
        let zot = Recreate {
            release: "zot".to_owned(),
            namespace: "zot".to_owned(),
            kind: "StatefulSet".to_owned(),
            name: "zot".to_owned(),
            fields: vec!["spec".to_owned()],
        };
        delete(&kubectl, &[zot]).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("calls")).unwrap(),
            "delete statefulset zot --namespace zot --cascade=orphan --ignore-not-found\n"
        );
    }
}
