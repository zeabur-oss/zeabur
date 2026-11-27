//! The images the cluster runs: built here, tagged by content, pushed to the cluster's registry.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use sha2::{Digest as _, Sha256};

use super::cluster::{REGISTRY_CONTAINER, REGISTRY_HOST};
use super::{DevCluster, identity_release};
use crate::checkout::{self, Pin};
use crate::output;

pub(crate) const BUSYBOX: &str = "busybox:1.37";
const IDENTITY_DEV: &str = "docker.io/library/zeabur-identity:dev";
const ZEABUR_DEV: &str = "docker.io/library/zeabur:dev";
const PROXY_DEV: &str = "docker.io/library/zeabur-proxy:dev";
const DATABASE_UPDATER_DEV: &str = "docker.io/library/zeabur-database-updater:dev";

/// One plain busybox image: Docker's containerd store keeps busybox as a multi-platform index.
pub(crate) fn busybox(dev: &DevCluster) -> Result<(), String> {
    dev.cmd("docker").args(["pull", "-q", BUSYBOX]).quiet()?;
    dev.cmd("docker")
        .args(["build", "-q", "-t", BUSYBOX, "-"])
        .input(format!("FROM {BUSYBOX}\nLABEL com.zeabur.test=busybox\n"))
        .quiet()
}

/// What `up` built or was given.
#[derive(Debug)]
pub(crate) struct Built {
    pub(crate) identity: String,
    pub(crate) zeabur: String,
    /// Zeabur Proxy's image, when Zeabur's was built here.
    pub(crate) proxy: Option<String>,
    /// The full profile's CloudNativePG updater, built from this checkout.
    pub(crate) database_updater: Option<String>,
    /// The local images to push to the registry.
    pub(crate) push: Vec<String>,
}

/// Zeabur Identity and its `zeabur-identityctl`, and, unless `zeabur_image` is given, Zeabur and
/// Zeabur Proxy built from this checkout. Zeabur Identity is built from `identity_src`, else it is
/// the published release that `xtask/pins/zeabur-identity` names.
pub(crate) fn build(
    dev: &DevCluster,
    identity_src: Option<&Path>,
    zeabur_image: Option<&str>,
    include_updater: bool,
) -> Result<Built, String> {
    let mut push = Vec::new();
    let identity = if let Some(src) = identity_src {
        output::say(&format!(
            "Zeabur Identity: building ZEABUR_IDENTITY_SRC {}",
            src.display()
        ));
        let identity = build_identity(dev, src)?;
        push.push(identity.clone());
        identity
    } else {
        let tag = match read_pin(dev, "xtask/pins/zeabur-identity")? {
            Pin::Tag(tag) => tag,
            Pin::Commit(commit) => return Err(no_release(&commit)),
        };
        let release = identity_release::fetch(dev, &tag)?.ok_or_else(|| no_release(&tag))?;
        output::say(&format!(
            "Zeabur Identity: release {tag}, {}",
            release.image
        ));
        link_identityctl(dev, &release.identityctl)?;
        release.image
    };

    let (zeabur, proxy) = match zeabur_image {
        None => {
            let script = dev.root.join("scripts/build-image.sh");
            dev.cmd(&script).arg(ZEABUR_DEV).run()?;
            dev.cmd(&script).args([PROXY_DEV, "zeabur-proxy"]).run()?;
            let zeabur = content_tag(dev, ZEABUR_DEV)?;
            let proxy = content_tag(dev, PROXY_DEV)?;
            push.extend([zeabur.clone(), proxy.clone()]);
            (zeabur, Some(proxy))
        }
        Some(image)
            if dev
                .cmd("docker")
                .args(["image", "inspect", image])
                .attempt() =>
        {
            let zeabur = content_tag(dev, image)?;
            push.push(zeabur.clone());
            (zeabur, None)
        }
        Some(image) => (image.to_owned(), None),
    };
    let database_updater = if include_updater {
        dev.cmd(dev.root.join("scripts/build-image.sh"))
            .args([DATABASE_UPDATER_DEV, "zeabur-database-updater"])
            .run()?;
        let image = content_tag(dev, DATABASE_UPDATER_DEV)?;
        push.push(image.clone());
        Some(image)
    } else {
        None
    };
    Ok(Built {
        identity,
        zeabur,
        proxy,
        database_updater,
        push,
    })
}

fn no_release(pin: &str) -> String {
    format!(
        "xtask/pins/zeabur-identity is {pin}, which has no Zeabur Identity release: \
         pin a release tag, or set ZEABUR_IDENTITY_SRC to a checkout to build"
    )
}

/// Builds Zeabur Identity's image and `zeabur-identityctl` from `src`; returns the image.
fn build_identity(dev: &DevCluster, src: &Path) -> Result<String, String> {
    let src = fs::canonicalize(src)
        .map_err(|e| format!("Zeabur Identity's source {}: {e}", src.display()))?;
    dev.cmd("nix")
        .arg("develop")
        .arg(&src)
        .args(["--command", "bash", "-c"])
        .arg(format!(
            "./scripts/build-image.sh {IDENTITY_DEV} && cargo build --release --locked -p zeabur-identityctl"
        ))
        .current_dir(&src)
        .run()
        .map_err(|e| format!("building Zeabur Identity: {e}"))?;
    link_identityctl(dev, &src.join("target/release/zeabur-identityctl"))?;
    content_tag(dev, IDENTITY_DEV)
}

fn link_identityctl(dev: &DevCluster, to: &Path) -> Result<(), String> {
    let link = dev.identityctl();
    if fs::symlink_metadata(&link).is_ok() {
        fs::remove_file(&link).map_err(|e| format!("removing {}: {e}", link.display()))?;
    }
    symlink(to, &link).map_err(|e| format!("linking {}: {e}", link.display()))
}

fn read_pin(dev: &DevCluster, pin_file: &str) -> Result<Pin, String> {
    let path = dev.root.join(pin_file);
    let pin = fs::read_to_string(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    Pin::parse(&pin)
}

/// `$url_env`, or `url` when it is unset or empty.
fn pin_url(url_env: &str, url: &str) -> String {
    std::env::var(url_env)
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| url.to_owned())
}

/// A checkout of the pin in file `pin_file` of this repository, from `$url_env` or `url`, kept
/// between runs for its build cache.
pub(crate) fn pinned_checkout(
    dev: &DevCluster,
    dir: &Path,
    pin_file: &str,
    url_env: &str,
    url: &str,
) -> Result<(), String> {
    checkout::checkout(dir, &read_pin(dev, pin_file)?, &pin_url(url_env, url))
}

/// Tags `image` by its content, under the registry's name in the cluster, so that a changed image
/// gets a new tag and the cluster a new value.
pub(crate) fn content_tag(dev: &DevCluster, image: &str) -> Result<String, String> {
    let id = image_id(dev, image)?;
    let (repository, _) = split_tag(image);
    let digits: String = id.trim_start_matches("sha256:").chars().take(12).collect();
    let tagged = format!("{REGISTRY_HOST}/{}:dev-{digits}", short_name(repository));
    tag(dev, image, &tagged)?;
    Ok(tagged)
}

pub(crate) fn image_id(dev: &DevCluster, image: &str) -> Result<String, String> {
    dev.cmd("docker")
        .args(["image", "inspect", "--format", "{{.Id}}", image])
        .read()
        .map(|id| id.trim().to_owned())
}

pub(crate) fn tag(dev: &DevCluster, image: &str, tagged: &str) -> Result<(), String> {
    dev.cmd("docker").args(["tag", image, tagged]).quiet()?;
    dev.record_image(tagged)
}

/// One tag for images built together, from all their ids.
pub(crate) fn joint_tag(ids: &[String]) -> String {
    let mut hash = Sha256::new();
    for id in ids {
        hash.update(id.as_bytes());
        hash.update(b"\n");
    }
    // Each byte becomes two hex digits, high nibble first, so the tag is the first 12 digits that
    // `sha256sum` prints for the same input.
    let digits: String = hash
        .finalize()
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0xF])
        .take(12)
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect();
    format!("dev-{digits}")
}

/// Pushes each local image to the cluster's registry, from the host's side of it. An image named
/// `REGISTRY_HOST/<path>` is pulled by the node under that name; `busybox:1.37` and other
/// Docker Hub images are pulled from the registry first, as `docker.io/library/busybox:1.37`.
pub(crate) fn push(dev: &DevCluster, images: &[String]) -> Result<(), String> {
    let port = host_port(dev)?;
    for image in images {
        let (repository, version) = split_tag(image);
        let pushed = format!(
            "localhost:{port}/{}:{}",
            registry_path(repository),
            version.unwrap_or("latest")
        );
        tag(dev, image, &pushed)?;
        dev.cmd("docker")
            .args(["push", "--quiet", &pushed])
            .quiet()?;
    }
    Ok(())
}

/// The registry's port on the host's loopback.
fn host_port(dev: &DevCluster) -> Result<String, String> {
    let ports = dev
        .cmd("docker")
        .args(["port", REGISTRY_CONTAINER, "5000/tcp"])
        .read()
        .map_err(|e| format!("the cluster has no registry (run `down`, then `up`): {e}"))?;
    ports
        .lines()
        .find_map(|line| {
            line.rsplit_once(':')
                .map(|(_, port)| port.trim().to_owned())
        })
        .ok_or_else(|| format!("{REGISTRY_CONTAINER} publishes no port: {ports}"))
}

/// The path in the registry of the image the node pulls as `repository`: a name under
/// `REGISTRY_HOST` as it is, a Docker Hub name as the node's containerd asks for it.
fn registry_path(repository: &str) -> String {
    if let Some(path) = repository.strip_prefix(&format!("{REGISTRY_HOST}/")) {
        return path.to_owned();
    }
    let path = repository.strip_prefix("docker.io/").unwrap_or(repository);
    if path.contains('/') {
        path.to_owned()
    } else {
        format!("library/{path}")
    }
}

/// `repository` without its registry and without Docker Hub's `library/`. As in Docker's
/// reference grammar, the first component names a registry when it holds a `.` or a `:`, or is
/// `localhost`.
fn short_name(repository: &str) -> &str {
    let repository = match repository.split_once('/') {
        Some((host, path)) if host.contains(['.', ':']) || host == "localhost" => path,
        _ => repository,
    };
    repository.strip_prefix("library/").unwrap_or(repository)
}

/// The repository and tag of a reference such as `ghcr.io:443/a/b:tag`, without its `@sha256:…`.
pub(crate) fn split_tag(image: &str) -> (&str, Option<&str>) {
    let image = image.split_once('@').map_or(image, |(name, _)| name);
    match image.rsplit_once(':') {
        Some((repository, tag)) if !tag.contains('/') => (repository, Some(tag)),
        _ => (image, None),
    }
}

#[cfg(test)]
mod tests {
    use super::{REGISTRY_HOST, joint_tag, registry_path, short_name, split_tag};

    #[test]
    fn a_name_under_the_registry_is_pushed_as_it_is() {
        assert_eq!(
            registry_path(&format!("{REGISTRY_HOST}/zeabur-builder/builder")),
            "zeabur-builder/builder"
        );
    }

    #[test]
    fn a_docker_hub_name_is_pushed_where_containerd_asks_for_it() {
        assert_eq!(registry_path("busybox"), "library/busybox");
        assert_eq!(
            registry_path("docker.io/library/busybox"),
            "library/busybox"
        );
        assert_eq!(registry_path("docker.io/rancher/k3s"), "rancher/k3s");
    }

    #[test]
    fn a_content_tag_keeps_the_short_name() {
        assert_eq!(short_name("docker.io/library/zeabur"), "zeabur");
        assert_eq!(
            short_name("zeabur-builder/builder"),
            "zeabur-builder/builder"
        );
        assert_eq!(short_name("ghcr.io/zeabur-oss/zeabur"), "zeabur-oss/zeabur");
    }

    #[test]
    fn a_content_tag_drops_a_registry_with_a_port() {
        assert_eq!(short_name("localhost:5000/zeabur"), "zeabur");
        assert_eq!(short_name("localhost/zeabur"), "zeabur");
        assert_eq!(
            short_name(&format!("{REGISTRY_HOST}/zeabur-builder/builder")),
            "zeabur-builder/builder"
        );
    }

    #[test]
    fn a_registry_port_is_not_a_tag() {
        assert_eq!(
            split_tag("localhost:5000/tool:v1"),
            ("localhost:5000/tool", Some("v1"))
        );
        assert_eq!(
            split_tag("localhost:5000/tool"),
            ("localhost:5000/tool", None)
        );
    }

    #[test]
    fn a_digest_is_not_a_tag() {
        let digest = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(
            split_tag(&format!("ghcr.io/zeabur-oss/zeabur@{digest}")),
            ("ghcr.io/zeabur-oss/zeabur", None)
        );
        assert_eq!(
            split_tag(&format!("localhost:5000/tool:v1@{digest}")),
            ("localhost:5000/tool", Some("v1"))
        );
    }

    #[test]
    fn a_joint_tag_changes_with_any_image() {
        let ids = |last: &str| {
            vec![
                "sha256:a".to_owned(),
                "sha256:b".to_owned(),
                last.to_owned(),
            ]
        };
        let tag = joint_tag(&ids("sha256:c"));
        assert!(tag.starts_with("dev-"));
        assert_eq!(tag, joint_tag(&ids("sha256:c")));
        assert_ne!(tag, joint_tag(&ids("sha256:d")));
    }

    #[test]
    fn a_joint_tag_is_what_sha256sum_of_the_ids_gave() {
        // printf 'sha256:a\nsha256:b\n' | sha256sum | cut -c1-12
        assert_eq!(
            joint_tag(&["sha256:a".to_owned(), "sha256:b".to_owned()]),
            "dev-5d02884ff145"
        );
    }
}
