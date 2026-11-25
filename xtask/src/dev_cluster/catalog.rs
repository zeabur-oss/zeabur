//! Components installed with the Helm commands Zeabur Catalog gives (Cluster → Components).

use std::fs;
use std::time::Duration;

use super::DevCluster;

/// `CATALOG_STOP_AT`: the release from which on the catalog's commands are left out.
pub(crate) fn stop_at_from_env() -> Option<String> {
    std::env::var("CATALOG_STOP_AT")
        .ok()
        .filter(|release| !release.is_empty())
}

/// The catalog's commands for one component, in a work directory with the charts they install.
#[derive(Debug)]
pub(crate) struct Script {
    component: String,
    namespace: String,
    work: tempfile::TempDir,
}

/// Writes the catalog's commands for `component` (its id) with `inputs` (`<input>=<value>`),
/// leaving out the releases from `stop_at` on, and pulls their charts, so that `install` needs no
/// download.
pub(crate) fn prepare(
    dev: &DevCluster,
    component: &str,
    namespace: &str,
    inputs: &[&str],
    stop_at: Option<&str>,
) -> Result<Script, String> {
    let context = |e: String| format!("preparing {component}: {e}");
    let script = dev
        .cmd("cargo")
        .current_dir(&dev.root)
        .args(["run", "-q", "--locked", "-p", "zeabur-catalog", "--example"])
        .args(["install", "--", component, "single-node"])
        .args(inputs)
        .read()
        .map_err(context)?;
    let work = tempfile::tempdir().map_err(|e| context(format!("a work directory: {e}")))?;
    let script = local_charts(until_release(&script, stop_at), |chart| {
        let dir = work.path().join("charts").join(chart.release);
        let mut args = vec!["--version", chart.version];
        if let Some(repo) = chart.repo {
            args.extend(["--repo", repo]);
        }
        let archive = super::pull_chart(dev, chart.reference, &args, &dir)?;
        archive
            .strip_prefix(work.path())
            .map(|path| path.display().to_string())
            .map_err(|e| e.to_string())
    })
    .map_err(context)?;
    let path = work.path().join("install.sh");
    fs::write(&path, script).map_err(|e| context(format!("writing {}: {e}", path.display())))?;
    Ok(Script {
        component: component.to_owned(),
        namespace: namespace.to_owned(),
        work,
    })
}

/// Runs `script`, then waits for the rollout of each Deployment in its namespace.
pub(crate) fn install(dev: &DevCluster, script: &Script) -> Result<(), String> {
    let Script {
        component,
        namespace,
        work,
    } = script;
    let context = |e: String| format!("installing {component}: {e}");
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
    for deployment in deployments.lines().filter(|line| !line.trim().is_empty()) {
        dev.rollout(namespace, deployment.trim(), Duration::from_secs(300))
            .map_err(|e| format!("waiting for {component} in {namespace}: {e}"))?;
    }
    Ok(())
}

/// The chart of one `helm upgrade --install` in a script.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Chart<'a> {
    release: &'a str,
    reference: &'a str,
    repo: Option<&'a str>,
    version: &'a str,
}

/// `script` with each `helm upgrade --install <release> <chart> [--repo …] --version …` installing
/// the archive that `pull` returns for its chart instead.
fn local_charts<'a>(
    script: &'a str,
    mut pull: impl FnMut(&Chart<'a>) -> Result<String, String>,
) -> Result<String, String> {
    let mut out = String::new();
    let mut lines = script.split_inclusive('\n');
    while let Some(line) = lines.next() {
        let indent = line.strip_suffix(line.trim_start()).unwrap_or_default();
        let Some(rest) = line.trim().strip_prefix("helm upgrade --install ") else {
            out.push_str(line);
            continue;
        };
        let mut command = vec![line.trim_end()];
        while command.last().is_some_and(|line| line.ends_with('\\')) {
            let Some(next) = lines.next() else { break };
            command.push(next.trim_end());
        }
        let mut words = rest.split_whitespace();
        let (Some(release), Some(chart)) = (words.next(), words.next()) else {
            return Err(format!("no release and chart in `{}`", line.trim()));
        };
        let option = |name: &str| {
            command.iter().find_map(|line| {
                line.trim()
                    .strip_prefix(name)
                    .map(|value| value.trim_end_matches('\\').trim())
            })
        };
        let chart = Chart {
            release,
            reference: chart,
            repo: option("--repo "),
            version: option("--version ")
                .ok_or_else(|| format!("no --version for release {release}"))?,
        };
        let archive = pull(&chart)?;
        let mut kept: Vec<String> = vec![format!(
            "{indent}helm upgrade --install {release} {archive} \\"
        )];
        kept.extend(
            command
                .iter()
                .skip(1)
                .filter(|line| {
                    let line = line.trim();
                    !line.starts_with("--repo ") && !line.starts_with("--version ")
                })
                .map(|line| (*line).to_owned()),
        );
        if let Some(last) = kept.last_mut() {
            *last = last.trim_end_matches('\\').trim_end().to_owned();
        }
        for line in kept {
            out.push_str(&line);
            out.push('\n');
        }
    }
    Ok(out)
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
    use super::{Chart, local_charts, until_release};

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

    #[test]
    fn each_chart_is_installed_from_the_archive_pulled_for_it() {
        let script = "\
# zot
cat > zot-values.yaml <<'ZEABUR_VALUES'
a: 1
ZEABUR_VALUES
helm upgrade --install zot zot \\
  --repo https://zotregistry.dev/helm-charts \\
  --version 0.1.134 \\
  --namespace zot --create-namespace \\
  --values zot-values.yaml
if ! kubectl get crd builds.zeabur.com >/dev/null 2>&1; then
  helm upgrade --install builder oci://ghcr.io/zeabur-oss/charts/zeabur-builder \\
    --version 0.3.0 \\
    --namespace builder-system --create-namespace
fi
";
        let mut pulled = Vec::new();
        let local = local_charts(script, |chart| {
            pulled.push(chart.clone());
            Ok(format!("charts/{}/{}.tgz", chart.release, chart.version))
        })
        .unwrap();
        assert_eq!(
            pulled,
            [
                Chart {
                    release: "zot",
                    reference: "zot",
                    repo: Some("https://zotregistry.dev/helm-charts"),
                    version: "0.1.134",
                },
                Chart {
                    release: "builder",
                    reference: "oci://ghcr.io/zeabur-oss/charts/zeabur-builder",
                    repo: None,
                    version: "0.3.0",
                },
            ]
        );
        assert_eq!(
            local,
            "\
# zot
cat > zot-values.yaml <<'ZEABUR_VALUES'
a: 1
ZEABUR_VALUES
helm upgrade --install zot charts/zot/0.1.134.tgz \\
  --namespace zot --create-namespace \\
  --values zot-values.yaml
if ! kubectl get crd builds.zeabur.com >/dev/null 2>&1; then
  helm upgrade --install builder charts/builder/0.3.0.tgz \\
    --namespace builder-system --create-namespace
fi
"
        );
    }

    #[test]
    fn a_chart_without_a_version_is_refused() {
        let script = "helm upgrade --install zot zot \\\n  --namespace zot\n";
        assert!(local_charts(script, |_| Ok(String::new())).is_err());
    }
}
