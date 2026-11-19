//! Components installed with the Helm commands Zeabur Catalog gives (Cluster → Components).

use std::fs;

use super::DevCluster;

/// Runs the catalog's commands for `component` (its id) with `inputs` (`<input>=<value>`), then
/// waits for the deployments in `namespace`.
pub(crate) fn install(
    dev: &DevCluster,
    component: &str,
    namespace: &str,
    inputs: &[&str],
) -> Result<(), String> {
    let context = |e: String| format!("installing {component}: {e}");
    let script = dev
        .cmd("cargo")
        .current_dir(&dev.root)
        .args(["run", "-q", "--locked", "-p", "zeabur-catalog", "--example"])
        .args(["install", "--", component, "single-node"])
        .args(inputs)
        .read()
        .map_err(context)?;
    let work = tempfile::tempdir().map_err(|e| context(format!("a work directory: {e}")))?;
    let path = work.path().join("install.sh");
    fs::write(&path, script).map_err(|e| context(format!("writing {}: {e}", path.display())))?;
    dev.cmd("bash")
        .args(["-euo", "pipefail", "install.sh"])
        .current_dir(work.path())
        .run()
        .map_err(context)?;
    dev.platform(namespace)?;
    let deployments = dev
        .kubectl()
        .args(["-n", namespace, "get", "deployment", "-o", "name"])
        .read()?;
    if deployments.trim().is_empty() {
        return Ok(());
    }
    dev.kubectl()
        .args(["-n", namespace, "wait", "deployment", "--all"])
        .args(["--for=condition=Available", "--timeout=300s"])
        .run()
        .inspect_err(|_| dev.diagnose(namespace))
        .map_err(|e| format!("waiting for {component} in {namespace}: {e}"))
}
