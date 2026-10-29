//! A k3d cluster with Zeabur Identity and Zeabur, installed with zeabur-installer's own objects
//! and values, at <https://zeabur.localhost> (`scripts/dev-cluster.sh`).

mod catalog;
pub(crate) mod certificates;
mod cluster;
mod full;
mod identity;
mod images;
mod registries;
mod snapshot;
mod zeabur;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::output;
use crate::process::{Cmd, on_path};

/// Zeabur Identity's host name.
pub(crate) const IDENTITY_HOST: &str = "identity.localhost";
/// Zeabur's host name.
pub(crate) const ZEABUR_HOST: &str = "zeabur.localhost";
/// The catalog's zot, as zeabur-installer sets it up: the node's containerd pulls `ZOT_HOST`
/// through a mirror to zot's fixed ClusterIP, checked against zot's CA.
const ZOT_HOST: &str = "zot.zot.svc:5000";
const ZOT_IP: &str = "10.43.0.50";
const ZOT_CA: &str = "/etc/rancher/k3s/zot-ca.crt";

const TOOLS: &[&str] = &["docker", "k3d", "kubectl", "helm", "openssl", "git", "curl"];

/// What `up` installs besides the cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Profile {
    /// Only the cluster, for the cluster tests of `crates/*/tests` and zeaburctl in local mode.
    Bare,
    /// Zeabur Identity and Zeabur.
    Default,
    /// Also CloudNativePG, the Percona Operator for MongoDB, Keel, zot, a test S3 and Zeabur
    /// Builder.
    Full,
}

impl Profile {
    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "bare" => Ok(Self::Bare),
            "default" => Ok(Self::Default),
            "full" => Ok(Self::Full),
            _ => Err(format!(
                "--profile needs bare, default or full, not {name:?}"
            )),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Bare => "bare",
            Self::Default => "default",
            Self::Full => "full",
        }
    }
}

/// The options of `up`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Up {
    pub(crate) profile: Profile,
    /// A working tree of Zeabur Identity to build as it is, instead of the pinned commit.
    pub(crate) identity_src: Option<PathBuf>,
    /// A working tree of Zeabur Builder to build as it is (full profile).
    pub(crate) builder_src: Option<PathBuf>,
    /// An image to run instead of one built from this checkout.
    pub(crate) zeabur_image: Option<String>,
    /// A chart (directory, `.tgz` or `oci://`) instead of `deploy/chart/zeabur`.
    pub(crate) zeabur_chart: Option<String>,
}

impl Up {
    /// Reads `up`'s arguments; `env` gives `ZEABUR_IDENTITY_SRC` and `ZEABUR_BUILDER_SRC`.
    pub(crate) fn parse(
        args: &[&str],
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, String> {
        let from_env = |name| env(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        let mut up = Self {
            profile: Profile::Default,
            identity_src: from_env("ZEABUR_IDENTITY_SRC"),
            builder_src: from_env("ZEABUR_BUILDER_SRC"),
            zeabur_image: None,
            zeabur_chart: None,
        };
        let mut args = args.iter();
        while let Some(&flag) = args.next() {
            let value = match args.next() {
                Some(value) if !value.is_empty() => (*value).to_owned(),
                _ => return Err(format!("{flag} needs a value")),
            };
            match flag {
                "--profile" => up.profile = Profile::parse(&value)?,
                "--identity-src" => up.identity_src = Some(value.into()),
                "--builder-src" => up.builder_src = Some(value.into()),
                "--zeabur-image" => up.zeabur_image = Some(value),
                "--zeabur-chart" => up.zeabur_chart = Some(value),
                _ => return Err(format!("unknown option {flag}")),
            }
        }
        Ok(up)
    }
}

/// This checkout and `target/dev-cluster`, and the commands that act on the cluster.
#[derive(Debug)]
struct DevCluster {
    root: PathBuf,
    state: PathBuf,
}

impl DevCluster {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let state = root.join("target/dev-cluster");
        Self { root, state }
    }

    fn kubeconfig(&self) -> PathBuf {
        self.state.join("kubeconfig")
    }

    fn tls(&self) -> PathBuf {
        self.state.join("tls")
    }

    fn identityctl(&self) -> PathBuf {
        self.state.join("identityctl")
    }

    /// A tool pointed at this cluster's kubeconfig. No attestation manifests in built images:
    /// k3d's import of a multi-manifest index can fail on the node.
    fn cmd(&self, program: impl AsRef<std::ffi::OsStr>) -> Cmd {
        Cmd::new(program)
            .env("KUBECONFIG", self.kubeconfig())
            .env("BUILDX_NO_DEFAULT_ATTESTATIONS", "1")
    }

    fn kubectl(&self) -> Cmd {
        self.cmd("kubectl")
    }

    fn apply(&self, manifest: impl Into<Vec<u8>>) -> Result<(), String> {
        self.kubectl()
            .args(["apply", "-f", "-"])
            .input(manifest)
            .quiet()
    }

    /// `kubectl create <args>` as YAML, applied, so that a second `up` updates the object.
    fn apply_created(&self, namespace: &str, args: &[&str]) -> Result<(), String> {
        let yaml = self
            .kubectl()
            .args(["-n", namespace, "create"])
            .args(args)
            .args(["--dry-run=client", "-o", "yaml"])
            .read()?;
        self.apply(yaml)
    }

    /// Marks namespaces as the platform's, as zeabur-installer does: a project's NetworkPolicy
    /// lets them in, and only then does Zeabur write one.
    fn platform(&self, namespace: &str) -> Result<(), String> {
        self.kubectl()
            .args(["label", "namespace", namespace, "zeabur.com/platform=true"])
            .arg("--overwrite")
            .quiet()
    }

    fn rollout(&self, namespace: &str, workload: &str, timeout: Duration) -> Result<(), String> {
        self.kubectl()
            .args(["-n", namespace, "rollout", "status", workload])
            .arg(format!("--timeout={}s", timeout.as_secs()))
            .run()
    }

    /// Notes an image this cluster tagged, for `down` to remove.
    fn record_image(&self, image: &str) -> Result<(), String> {
        use std::io::Write as _;
        let path = self.state.join("images");
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut file| writeln!(file, "{image}"))
            .map_err(|e| format!("writing {}: {e}", path.display()))
    }

    fn write(&self, name: &str, content: &str) -> Result<PathBuf, String> {
        let path = self.state.join(name);
        fs::write(&path, content).map_err(|e| format!("writing {}: {e}", path.display()))?;
        Ok(path)
    }
}

/// Creates the cluster or brings a running one in line with the checkout.
pub(crate) fn up(options: &Up) -> Result<(), String> {
    let dev = DevCluster::new();
    if let Some(tool) = TOOLS.iter().find(|tool| !on_path(tool)) {
        return Err(format!("{tool} is not on PATH"));
    }
    fs::create_dir_all(&dev.state).map_err(|e| format!("creating {}: {e}", dev.state.display()))?;
    images::busybox(&dev)?;
    let built = if options.profile == Profile::Bare {
        None
    } else {
        Some(images::build(
            &dev,
            options.identity_src.as_deref(),
            options.zeabur_image.as_deref(),
        )?)
    };
    let chart = resolve_chart(&dev, options.zeabur_chart.as_deref())?;

    cluster::create(&dev)?;
    let mut import = vec![images::BUSYBOX.to_owned()];
    if let Some(built) = &built {
        import.extend(built.import.iter().cloned());
    }
    images::import(&dev, &import)?;
    cluster::traefik(&dev)?;
    let Some(built) = built else {
        dev.write("profile", "bare\n")?;
        output::say(&format!(
            "kubectl: export KUBECONFIG={}",
            dev.kubeconfig().display()
        ));
        return Ok(());
    };
    cluster::resolve_hosts(&dev)?;
    certificates::ensure(&dev.tls(), &[IDENTITY_HOST, ZEABUR_HOST])?;

    // cert-manager issues the certificate kube-apiserver checks on Zeabur Identity's internal
    // address, and the full profile's certificates.
    catalog::install(
        &dev,
        "certificates/cert-manager",
        "cert-manager",
        &[],
        catalog::stop_at_from_env().as_deref(),
    )?;
    // Before Zeabur, so that e2e-l2's removal check counts them as the cluster's own.
    if options.profile == Profile::Full {
        full::install(&dev, options.builder_src.as_deref())?;
    }
    identity::install(&dev, &built.identity)?;
    zeabur::install(
        &dev,
        &zeabur::Install {
            profile: options.profile,
            identity_image: &built.identity,
            zeabur_image: &built.zeabur,
            proxy_image: built.proxy.as_deref(),
            chart: &chart,
        },
    )
}

/// The chart as a local path; an `oci://` chart is pulled into `target/dev-cluster/chart`.
fn resolve_chart(dev: &DevCluster, chart: Option<&str>) -> Result<PathBuf, String> {
    let chart = match chart {
        None => dev.root.join("deploy/chart/zeabur"),
        Some(oci) if oci.starts_with("oci://") => {
            let dir = dev.state.join("chart");
            if dir.exists() {
                fs::remove_dir_all(&dir).map_err(|e| format!("removing {}: {e}", dir.display()))?;
            }
            fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
            dev.cmd("helm")
                .args(["pull", oci, "--destination"])
                .arg(&dir)
                .run()?;
            fs::read_dir(&dir)
                .map_err(|e| format!("reading {}: {e}", dir.display()))?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .find(|path| path.extension().is_some_and(|ext| ext == "tgz"))
                .ok_or_else(|| format!("helm pull {oci} left no .tgz"))?
        }
        Some(path) => PathBuf::from(path),
    };
    fs::canonicalize(&chart).map_err(|e| format!("no chart at {}: {e}", chart.display()))
}

/// Deletes the cluster, its volume, the images it tagged and its state, but not the checkouts.
pub(crate) fn down() -> Result<(), String> {
    let dev = DevCluster::new();
    dev.cmd("k3d")
        .args(["cluster", "delete", cluster::NAME])
        .attempt();
    dev.cmd("docker")
        .args(["volume", "rm", "-f", cluster::AUTHN_VOLUME])
        .attempt();
    if let Ok(images) = fs::read_to_string(dev.state.join("images")) {
        let images: Vec<&str> = images.lines().filter(|line| !line.is_empty()).collect();
        if !images.is_empty() {
            dev.cmd("docker")
                .args(["image", "rm", "-f"])
                .args(images)
                .attempt();
        }
    }
    for name in [
        "tls",
        "kubeconfig",
        "images",
        "profile",
        "before.txt",
        "after.txt",
        "registries.yaml",
        "install.yaml",
        "identityctl",
    ] {
        let path = dev.state.join(name);
        let removed = match fs::symlink_metadata(&path) {
            Err(_) => Ok(()),
            Ok(meta) if meta.is_dir() => fs::remove_dir_all(&path),
            Ok(_) => fs::remove_file(&path),
        };
        removed.map_err(|e| format!("removing {}: {e}", path.display()))?;
    }
    output::say(&format!("cluster {} deleted", cluster::NAME));
    Ok(())
}

/// Prints every cluster-scoped object and every namespace's labels and annotations.
pub(crate) fn snapshot() -> Result<(), String> {
    let dev = DevCluster::new();
    output::write(&snapshot::take(&dev)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Profile, Up};

    fn env(name: &str) -> Option<String> {
        match name {
            "ZEABUR_IDENTITY_SRC" => Some("../identity".to_owned()),
            "ZEABUR_BUILDER_SRC" => Some(String::new()),
            _ => None,
        }
    }

    #[test]
    fn up_defaults_to_the_default_profile_and_reads_sources_from_the_environment() {
        let up = Up::parse(&[], env).unwrap();
        assert_eq!(up.profile, Profile::Default);
        assert_eq!(up.identity_src, Some(PathBuf::from("../identity")));
        assert_eq!(up.builder_src, None);
        assert_eq!(up.zeabur_chart, None);
    }

    #[test]
    fn options_override_the_environment() {
        let up = Up::parse(
            &[
                "--profile",
                "full",
                "--identity-src",
                "/src/identity",
                "--builder-src",
                "b",
                "--zeabur-image",
                "ghcr.io/zeabur-oss/zeabur:0.1.1",
                "--zeabur-chart",
                "oci://ghcr.io/zeabur-oss/charts/zeabur",
            ],
            env,
        )
        .unwrap();
        assert_eq!(
            up,
            Up {
                profile: Profile::Full,
                identity_src: Some("/src/identity".into()),
                builder_src: Some("b".into()),
                zeabur_image: Some("ghcr.io/zeabur-oss/zeabur:0.1.1".to_owned()),
                zeabur_chart: Some("oci://ghcr.io/zeabur-oss/charts/zeabur".to_owned()),
            }
        );
    }

    #[test]
    fn up_refuses_unknown_profiles_options_and_missing_values() {
        assert!(Up::parse(&["--profile", "huge"], env).is_err());
        assert!(Up::parse(&["--profile"], env).is_err());
        assert!(Up::parse(&["--identity-src", ""], env).is_err());
        assert!(Up::parse(&["--fast", "yes"], env).is_err());
    }
}
