# Releases

Zeabur is released from CI only, by `.github/workflows/release.yml`, when a maintainer pushes a
tag `v<version>`. The workflow publishes the image and the chart, and leaves a **draft** GitHub
release; a maintainer checks the draft and publishes it by hand. CI never publishes a release.

```text
tag v0.0.2 ─▶ CI ─▶ build (Linux amd64/arm64, macOS arm64) ─▶ smoke ─▶ publish the image and
  the chart by digest, sign, attest ─▶ build the installer with those digests ─▶ draft release
  ─▶ verify the draft ─▶ a maintainer publishes it
```

## What a release contains

| Artifact | Where |
|---|---|
| Image, linux/amd64 and linux/arm64 | `ghcr.io/zeabur-oss/zeabur:<version>`; install by the index digest in `digests.txt` |
| Helm chart, its default image pinned to that digest | `oci://ghcr.io/zeabur-oss/charts/zeabur`, version `<version>`; also the asset `zeabur-<version>.tgz` |
| `zeaburctl`, Linux amd64 and arm64 (static, musl), macOS arm64 (Apple silicon) | `zeaburctl-v<version>-<os>-<arch>.tar.gz` |
| `zeabur`, the server binary in the image, Linux amd64 and arm64 | `zeabur-v<version>-linux-<arch>.tar.gz` |
| `zeabur-installer`, which installs a cluster with Zeabur on one server ([Quickstart](../get-started/quickstart.md)), Linux amd64 and arm64 (static, musl). It pins this release's image and chart, and the catalog and Zeabur Identity releases, by digest | `zeabur-installer-v<version>-linux-<arch>.tar.gz` |
| Documentation screenshots, AVIF, in each locale and theme | `docs-shots-v<version>.tar` |
| Digests of the image index, each platform's image and the chart | `digests.txt` |
| Checksums of every asset above, and their signature | `SHA256SUMS`, `SHA256SUMS.sigstore.json` |

The registry also keeps each platform image under a tag `candidate-<run>-<attempt>-<arch>`,
the run that pushed it before it joined the index. Every archive holds the binary and `LICENSE`.
There is no macOS `zeabur`: the server runs only in a pod. macOS builds are for Apple silicon
only. The image, its two platform images, the chart and `SHA256SUMS` are signed keyless with
[Sigstore](https://docs.sigstore.dev/) by the release workflow at the release's tag. The image,
the chart and every asset carry a
[build provenance attestation](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations/using-artifact-attestations-to-establish-provenance-for-builds)
from the same workflow.

The macOS build is not notarized by Apple. A file downloaded in a browser is quarantined and
refused; `gh release download` and `curl` do not quarantine. For a browser download, check it as
below, then `xattr -d com.apple.quarantine zeaburctl`.

## Download and verify zeaburctl

With `gh`, `cosign` 3 and GNU `sha256sum` (all in `nix develop`):

```sh
version=0.0.2
platform=darwin-arm64     # linux-amd64, linux-arm64
identity="https://github.com/zeabur-oss/zeabur/.github/workflows/release.yml@refs/tags/v$version"
gh release download "v$version" --repo zeabur-oss/zeabur --dir "zeabur-$version" \
  --pattern SHA256SUMS --pattern SHA256SUMS.sigstore.json \
  --pattern "zeaburctl-v$version-$platform.tar.gz"
cd "zeabur-$version"
cosign verify-blob --bundle SHA256SUMS.sigstore.json --certificate-identity "$identity" \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
gh attestation verify "zeaburctl-v$version-$platform.tar.gz" --repo zeabur-oss/zeabur \
  --cert-identity "$identity"
tar -xzf "zeaburctl-v$version-$platform.tar.gz" zeaburctl
./zeaburctl --version
```

The image and the chart: [Install on an existing cluster](../get-started/install.md), step 1.
