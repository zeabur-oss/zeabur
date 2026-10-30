//! Components installed with the Helm commands Zeabur Catalog gives (Cluster → Components).

use std::fs;

use super::DevCluster;

/// `CATALOG_STOP_AT`: the release from which on the catalog's commands are left out.
pub(crate) fn stop_at_from_env() -> Option<String> {
    std::env::var("CATALOG_STOP_AT")
        .ok()
        .filter(|release| !release.is_empty())
}

/// Runs the catalog's commands for `component` (`<capability>/<id>`) with `inputs`
/// (`<input>=<value>`), leaving out the releases from `stop_at` on, then waits for the
/// deployments in `namespace`.
pub(crate) fn install(
    dev: &DevCluster,
    component: &str,
    namespace: &str,
    inputs: &[&str],
    stop_at: Option<&str>,
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
    fs::write(&path, until_release(&script, stop_at))
        .map_err(|e| context(format!("writing {}: {e}", path.display())))?;
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
        .map_err(|e| format!("waiting for {component} in {namespace}: {e}"))
}

/// The script up to the line `# <stop_at>` that starts that release's commands.
fn until_release<'a>(script: &'a str, stop_at: Option<&str>) -> &'a str {
    let Some(stop_at) = stop_at else {
        return script;
    };
    let marker = format!("# {stop_at}");
    let mut offset = 0;
    for line in script.split_inclusive('\n') {
        if line.trim_end_matches('\n') == marker {
            return script.get(..offset).unwrap_or(script);
        }
        offset += line.len();
    }
    script
}

#[cfg(test)]
mod tests {
    use super::until_release;

    const SCRIPT: &str = "\
# zot
helm upgrade --install zot zot/zot
# builder-extra
echo kept
# builder
helm upgrade --install builder builder/builder
# after
echo gone
";

    #[test]
    fn the_releases_from_the_named_one_on_are_left_out() {
        let kept = until_release(SCRIPT, Some("builder"));
        assert!(kept.contains("helm upgrade --install zot"));
        assert!(kept.contains("echo kept"));
        assert!(!kept.contains("builder/builder"));
        assert!(!kept.contains("echo gone"));
    }

    #[test]
    fn without_a_stop_or_its_line_the_script_is_whole() {
        assert_eq!(until_release(SCRIPT, None), SCRIPT);
        assert_eq!(until_release(SCRIPT, Some("keel")), SCRIPT);
    }
}
