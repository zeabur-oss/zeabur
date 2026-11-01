//! The images the cluster runs: built here, tagged by content, imported into the node.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use sha2::{Digest as _, Sha256};

use super::{DevCluster, cluster};
use crate::checkout::{self, Pin};

pub(crate) const BUSYBOX: &str = "busybox:1.37";
const IDENTITY_DEV: &str = "docker.io/library/zeabur-identity:dev";
const ZEABUR_DEV: &str = "docker.io/library/zeabur:dev";
const PROXY_DEV: &str = "docker.io/library/zeabur-proxy:dev";

/// One plain busybox image: Docker's containerd store keeps busybox as a multi-platform index.
pub(crate) fn busybox(dev: &DevCluster) -> Result<(), String> {
    dev.cmd("docker").args(["pull", "-q", BUSYBOX]).quiet()?;
    dev.cmd("docker")
        .args(["build", "-q", "-t", BUSYBOX, "-"])
        .env("DOCKER_BUILDKIT", "0")
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
    /// The local images to import into the node.
    pub(crate) import: Vec<String>,
}

/// Builds Zeabur Identity (and links its `zeabur-identityctl`) and, unless `zeabur_image` is
/// given, Zeabur and Zeabur Proxy from this checkout.
pub(crate) fn build(
    dev: &DevCluster,
    identity_src: Option<&Path>,
    zeabur_image: Option<&str>,
) -> Result<Built, String> {
    let identity_src = if let Some(src) = identity_src {
        src.to_path_buf()
    } else {
        let dir = dev.state.join("identity");
        pinned_checkout(
            dev,
            &dir,
            "ZEABUR_IDENTITY_PINNED",
            "ZEABUR_IDENTITY_GIT",
            "https://github.com/zeabur-oss/identity.git",
        )?;
        dir
    };
    let identity_src = fs::canonicalize(&identity_src)
        .map_err(|e| format!("Zeabur Identity's source {}: {e}", identity_src.display()))?;
    dev.cmd("nix")
        .arg("develop")
        .arg(&identity_src)
        .args(["--command", "bash", "-c"])
        .arg(format!(
            "./scripts/build-image.sh {IDENTITY_DEV} && cargo build --release --locked -p zeabur-identityctl"
        ))
        .current_dir(&identity_src)
        .run()
        .map_err(|e| format!("building Zeabur Identity: {e}"))?;
    let link = dev.identityctl();
    if fs::symlink_metadata(&link).is_ok() {
        fs::remove_file(&link).map_err(|e| format!("removing {}: {e}", link.display()))?;
    }
    symlink(
        identity_src.join("target/release/zeabur-identityctl"),
        &link,
    )
    .map_err(|e| format!("linking {}: {e}", link.display()))?;
    let identity = content_tag(dev, IDENTITY_DEV)?;
    let mut import = vec![identity.clone()];

    let (zeabur, proxy) = match zeabur_image {
        None => {
            let script = dev.root.join("scripts/build-image.sh");
            dev.cmd(&script).arg(ZEABUR_DEV).run()?;
            dev.cmd(&script).args([PROXY_DEV, "zeabur-proxy"]).run()?;
            let zeabur = content_tag(dev, ZEABUR_DEV)?;
            let proxy = content_tag(dev, PROXY_DEV)?;
            import.extend([zeabur.clone(), proxy.clone()]);
            (zeabur, Some(proxy))
        }
        Some(image)
            if dev
                .cmd("docker")
                .args(["image", "inspect", image])
                .attempt() =>
        {
            let zeabur = content_tag(dev, image)?;
            import.push(zeabur.clone());
            (zeabur, None)
        }
        Some(image) => (image.to_owned(), None),
    };
    Ok(Built {
        identity,
        zeabur,
        proxy,
        import,
    })
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
    let path = dev.root.join(pin_file);
    let pin = fs::read_to_string(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let url = std::env::var(url_env)
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| url.to_owned());
    checkout::checkout(dir, &Pin::parse(&pin)?, &url)
}

/// Tags `image` by its content, so that a changed image gets a new tag and the cluster a new
/// value.
pub(crate) fn content_tag(dev: &DevCluster, image: &str) -> Result<String, String> {
    let id = image_id(dev, image)?;
    let (repository, _) = split_tag(image);
    let digits: String = id.trim_start_matches("sha256:").chars().take(12).collect();
    let tagged = format!("{repository}:dev-{digits}");
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
    let digits: String = hash
        .finalize()
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0xF])
        .take(12)
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect();
    format!("dev-{digits}")
}

/// Imports local images into the node and fails unless the node has them: k3d's import of a
/// multi-manifest index can fail on the node while reporting success.
pub(crate) fn import(dev: &DevCluster, images: &[String]) -> Result<(), String> {
    dev.cmd("k3d")
        .args(["image", "import"])
        .args(images)
        .args(["--cluster", cluster::NAME])
        .quiet()?;
    let listing = dev
        .cmd("docker")
        .args(["exec", cluster::NODE, "crictl", "images"])
        .read()?;
    images
        .iter()
        .find(|image| !node_has(&listing, image))
        .map_or(Ok(()), |image| {
            Err(format!("the node does not have {image} after the import"))
        })
}

/// The repository and tag of a reference such as `ghcr.io:443/a/b:tag`, without its `@sha256:…`.
pub(crate) fn split_tag(image: &str) -> (&str, Option<&str>) {
    let image = image.split_once('@').map_or(image, |(name, _)| name);
    match image.rsplit_once(':') {
        Some((repository, tag)) if !tag.contains('/') => (repository, Some(tag)),
        _ => (image, None),
    }
}

/// The name containerd lists for an image: `busybox` is `docker.io/library/busybox`.
fn full_name(repository: &str) -> String {
    let first = repository.split('/').next().unwrap_or_default();
    let registry = repository.contains('/')
        && (first.contains('.') || first.contains(':') || first == "localhost");
    if registry {
        repository.to_owned()
    } else if repository.contains('/') {
        format!("docker.io/{repository}")
    } else {
        format!("docker.io/library/{repository}")
    }
}

/// Whether `crictl images`' `listing` has `image`.
fn node_has(listing: &str, image: &str) -> bool {
    let (repository, tag) = split_tag(image);
    let name = full_name(repository);
    let tag = tag.unwrap_or("latest");
    listing.lines().any(|line| {
        let mut columns = line.split_whitespace();
        columns.next() == Some(name.as_str()) && columns.next() == Some(tag)
    })
}

#[cfg(test)]
mod tests {
    use super::{joint_tag, node_has, split_tag};

    const LISTING: &str = "\
IMAGE                                   TAG                 IMAGE ID            SIZE
docker.io/library/busybox               1.37                0a1b2c3d4e5f6       2.2MB
docker.io/library/zeabur                dev-0123456789ab    1a2b3c4d5e6f7       30MB
docker.io/zeabur-builder/builder        dev-aaaaaaaaaaaa    2a2b3c4d5e6f7       40MB
ghcr.io/zeabur-oss/zeabur               0.1.1               3a2b3c4d5e6f7       30MB
localhost:5000/tool                     v1                  4a2b3c4d5e6f7       1MB
";

    #[test]
    fn short_names_match_what_containerd_lists() {
        for image in [
            "busybox:1.37",
            "docker.io/library/zeabur:dev-0123456789ab",
            "zeabur-builder/builder:dev-aaaaaaaaaaaa",
            "ghcr.io/zeabur-oss/zeabur:0.1.1",
            "localhost:5000/tool:v1",
        ] {
            assert!(node_has(LISTING, image), "{image}");
        }
    }

    #[test]
    fn another_tag_or_a_missing_image_is_not_there() {
        for image in [
            "busybox:1.36",
            "zeabur:dev-ffffffffffff",
            "builder:dev-aaaaaaaaaaaa",
            "docker.io/library/zeabur-proxy:dev-0123456789ab",
        ] {
            assert!(!node_has(LISTING, image), "{image}");
        }
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
