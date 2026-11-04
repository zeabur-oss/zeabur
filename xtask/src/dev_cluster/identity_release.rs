//! A published Zeabur Identity release: its image and `zeabur-identityctl`, checked as
//! `scripts/release.ts` checks the pins of a Zeabur release. cosign checks the signature of the
//! release's `SHA256SUMS`, which then gives the sums of `digests.txt` and of the identityctl
//! archive; `digests.txt` gives the image by its digest.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use super::DevCluster;

const REPOSITORY: &str = "zeabur-oss/identity";
const IMAGE: &str = "ghcr.io/zeabur-oss/identity@sha256:";
/// The signer of every zeabur-oss release from v0.1.1 on: zeabur-oss/.github's shared release
/// workflow.
const SIGNER: &str = r"^https://github\.com/zeabur-oss/\.github/\.github/workflows/release\.yml@";

/// The image and identityctl of one release.
#[derive(Debug)]
pub(crate) struct Release {
    pub(crate) image: String,
    pub(crate) identityctl: PathBuf,
}

/// The release `tag` of Zeabur Identity, downloaded to `target/dev-cluster/identity-release` and
/// checked; `None` if `tag` has no published release. The node pulls the image's platform from
/// its index, and identityctl is the archive of the node's architecture: k3d runs the node in the
/// Docker that this machine uses.
pub(crate) fn fetch(dev: &DevCluster, tag: &str) -> Result<Option<Release>, String> {
    let docker_arch = dev
        .cmd("docker")
        .args(["info", "--format", "{{.Architecture}}"])
        .read()?;
    let arch = archive_arch(docker_arch.trim())
        .ok_or_else(|| format!("Zeabur Identity has no release for {}", docker_arch.trim()))?;
    let dir = dev.state.join("identity-release");
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("removing {}: {e}", dir.display()))?;
    }
    fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    let context = |e: String| format!("Zeabur Identity {tag}: {e}");

    let list = dir.join("SHA256SUMS");
    if !download(dev, tag, "SHA256SUMS", &list).map_err(context)? {
        return Ok(None);
    }
    let bundle = dir.join("SHA256SUMS.sigstore.json");
    if !download(dev, tag, "SHA256SUMS.sigstore.json", &bundle).map_err(context)? {
        return Err(context(
            "the release has no SHA256SUMS.sigstore.json".to_owned(),
        ));
    }
    dev.cmd("cosign")
        .arg("verify-blob")
        .arg("--bundle")
        .arg(&bundle)
        .args(["--certificate-identity-regexp", SIGNER])
        .args(["--certificate-github-workflow-repository", REPOSITORY])
        .arg("--certificate-github-workflow-ref")
        .arg(format!("refs/tags/{tag}"))
        .args([
            "--certificate-oidc-issuer",
            "https://token.actions.githubusercontent.com",
        ])
        .arg(&list)
        .quiet()
        .map_err(context)?;
    let sums = read(&list)?;

    let digests = checked(dev, tag, &sums, "digests.txt", &dir).map_err(context)?;
    let image = index_image(&read(&digests)?).map_err(context)?;
    let archive = format!("zeabur-identityctl-{tag}-linux-{arch}.tar.gz");
    let archive = checked(dev, tag, &sums, &archive, &dir).map_err(context)?;
    dev.cmd("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&dir)
        .arg("zeabur-identityctl")
        .quiet()
        .map_err(context)?;
    Ok(Some(Release {
        image,
        identityctl: dir.join("zeabur-identityctl"),
    }))
}

/// Downloads `file` of the release and checks it against `sums`.
fn checked(
    dev: &DevCluster,
    tag: &str,
    sums: &str,
    file: &str,
    dir: &Path,
) -> Result<PathBuf, String> {
    let want = listed(sums, file)?;
    let path = dir.join(file);
    if !download(dev, tag, file, &path)? {
        return Err(format!("the release has no {file}"));
    }
    let bytes = fs::read(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let got: String = Sha256::digest(&bytes)
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0xF])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect();
    if got == want {
        Ok(path)
    } else {
        Err(format!("{file} does not match SHA256SUMS"))
    }
}

/// Downloads `file` of release `tag` to `to`; `false` if the release has no such file.
fn download(dev: &DevCluster, tag: &str, file: &str, to: &Path) -> Result<bool, String> {
    let status = dev
        .cmd("curl")
        .args(["--silent", "--show-error", "--location", "--retry", "3"])
        .args(["--write-out", "%{http_code}", "--output"])
        .arg(to)
        .arg(format!(
            "https://github.com/{REPOSITORY}/releases/download/{tag}/{file}"
        ))
        .read()?;
    match status.trim() {
        "200" => Ok(true),
        "404" => Ok(false),
        other => Err(format!("downloading {file}: HTTP {other}")),
    }
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))
}

/// The SHA-256 that `SHA256SUMS` lists for `file`.
fn listed(sums: &str, file: &str) -> Result<String, String> {
    sums.lines()
        .filter_map(|line| line.split_once(char::is_whitespace))
        .find(|(_, name)| name.trim_start().trim_start_matches('*') == file)
        .map(|(sum, _)| sum.to_ascii_lowercase())
        .filter(|sum| sum.len() == 64 && sum.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| format!("SHA256SUMS lists no SHA-256 for {file}"))
}

/// The multi-platform image that `digests.txt` records as `index`.
fn index_image(digests: &str) -> Result<String, String> {
    let image = digests
        .lines()
        .filter_map(|line| line.split_once(char::is_whitespace))
        .find(|(label, _)| *label == "index")
        .map(|(_, image)| image.trim())
        .ok_or("digests.txt records no index")?;
    let digest = image.strip_prefix(IMAGE).unwrap_or_default();
    if digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(image.to_owned())
    } else {
        Err(format!("digests.txt names {image}, not {IMAGE}<digest>"))
    }
}

/// The architecture in the name of a release's archive, from the one Docker reports.
fn archive_arch(docker: &str) -> Option<&'static str> {
    match docker {
        "x86_64" | "amd64" => Some("amd64"),
        "aarch64" | "arm64" => Some("arm64"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{archive_arch, index_image, listed};

    const SUMS: &str = "\
6ad5244e7aede0e1a48cffabdee2ebb8b4296d345e69a499417be9132276491c  digests.txt
C9D0849BFFA5183B8C98A91D6CF35EC73188949C498B49B86E22539C2D1FB3DE *zeabur-identityctl-v0.1.2-linux-amd64.tar.gz
0123  zeabur-identityctl-v0.1.2-linux-arm64.tar.gz
";

    #[test]
    fn sums_are_read_in_text_and_binary_mode() {
        assert_eq!(
            listed(SUMS, "digests.txt").unwrap(),
            "6ad5244e7aede0e1a48cffabdee2ebb8b4296d345e69a499417be9132276491c"
        );
        assert_eq!(
            listed(SUMS, "zeabur-identityctl-v0.1.2-linux-amd64.tar.gz").unwrap(),
            "c9d0849bffa5183b8c98a91d6cf35ec73188949c498b49b86e22539c2d1fb3de"
        );
    }

    #[test]
    fn a_missing_file_or_a_short_sum_is_refused() {
        assert!(listed(SUMS, "digests").is_err());
        assert!(listed(SUMS, "zeabur-identityctl-v0.1.2-linux-arm64.tar.gz").is_err());
    }

    #[test]
    fn the_image_is_the_index_by_digest() {
        let digests = "\
linux/amd64  ghcr.io/zeabur-oss/identity@sha256:9d28569f47cdf57f25e4193e9b565a8ad41fc4533622f6ec55ac225614160322
index  ghcr.io/zeabur-oss/identity@sha256:4b691461c6f333ee3771b740298135fa4e336489e150ba82381197baa3168f5d
";
        assert_eq!(
            index_image(digests).unwrap(),
            "ghcr.io/zeabur-oss/identity@sha256:4b691461c6f333ee3771b740298135fa4e336489e150ba82381197baa3168f5d"
        );
    }

    #[test]
    fn an_image_by_tag_or_of_another_repository_is_refused() {
        for image in [
            "ghcr.io/zeabur-oss/identity:v0.1.2",
            "ghcr.io/someone/identity@sha256:4b691461c6f333ee3771b740298135fa4e336489e150ba82381197baa3168f5d",
            "ghcr.io/zeabur-oss/identity@sha256:4b69",
        ] {
            assert!(
                index_image(&format!("index  {image}\n")).is_err(),
                "{image}"
            );
        }
        assert!(index_image("linux/amd64  ghcr.io/zeabur-oss/identity@sha256:aa\n").is_err());
    }

    #[test]
    fn only_amd64_and_arm64_have_an_archive() {
        assert_eq!(archive_arch("x86_64"), Some("amd64"));
        assert_eq!(archive_arch("aarch64"), Some("arm64"));
        assert_eq!(archive_arch("riscv64"), None);
    }
}
