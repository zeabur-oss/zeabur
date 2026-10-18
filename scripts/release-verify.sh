#!/usr/bin/env bash
# Checks a downloaded release as someone installing it would, with no registry login and nothing
# rebuilt: the signed checksums, every signature and provenance attestation, the chart's pinned
# image, then the artifacts themselves. On Linux it installs the published chart on a disposable
# k3s cluster, whose node pulls the published image (release-smoke.sh); on macOS it runs
# engawactl. Run inside `nix develop` on Linux amd64/arm64 or macOS arm64; `gh` must be signed in
# (the attestations are read from GitHub's API).
#   scripts/release-verify.sh v0.0.2 downloaded \
#     https://github.com/engawahq/engawa/.github/workflows/release.yml@refs/tags/v0.0.2
set -euo pipefail

if [[ $# != 3 ]]; then
  echo "usage: release-verify.sh <vVERSION> <directory> <certificate-identity>" >&2
  exit 2
fi
version="$1"
directory="$(cd "$2" && pwd)"
identity="$3"
root="$(cd "$(dirname "$0")/.." && pwd)"
: "${IN_NIX_SHELL:?run inside nix develop}"
if [[ ! "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ || -z "$identity" ]]; then
  echo "expected a version like v0.0.2 and the exact signing identity" >&2
  exit 2
fi
case "$(uname -s):$(uname -m)" in
  Linux:x86_64) platform=linux-amd64 ;;
  Linux:aarch64) platform=linux-arm64 ;;
  Darwin:arm64) platform=darwin-arm64 ;;
  *) echo "release verification runs on Linux amd64/arm64 or macOS arm64" >&2; exit 2 ;;
esac
plain="${version#v}"
repository="${GITHUB_REPOSITORY:-engawahq/engawa}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
# A fresh, empty registry configuration: a publisher's login must not hide a private package.
export DOCKER_CONFIG="$work/docker" HELM_REGISTRY_CONFIG="$work/docker/config.json"
mkdir -p "$DOCKER_CONFIG"
certificate=(--certificate-identity "$identity" --certificate-oidc-issuer https://token.actions.githubusercontent.com)
attestation=(--repo "$repository" --cert-identity "$identity")

# 1. The checksums: signed by the release workflow at this tag, and covering every asset.
cosign verify-blob "${certificate[@]}" --bundle "$directory/SHA256SUMS.sigstore.json" "$directory/SHA256SUMS"
expected=(digests.txt "engawa-$plain.tgz")
for build in linux-amd64 linux-arm64 darwin-arm64; do
  expected+=("engawactl-$version-$build.tar.gz")
done
expected+=("engawa-$version-linux-amd64.tar.gz" "engawa-$version-linux-arm64.tar.gz")
listed="$(awk '{ print $2 }' "$directory/SHA256SUMS" | sort)"
if [[ "$listed" != "$(printf '%s\n' "${expected[@]}" | sort)" ]]; then
  echo "SHA256SUMS does not list exactly the release assets:" >&2
  diff <(printf '%s\n' "${expected[@]}" | sort) <(echo "$listed") >&2 || true
  exit 1
fi
(cd "$directory" && sha256sum --check --strict SHA256SUMS)

# 2. Provenance of every asset: built by this repository's release workflow at this tag.
for asset in "${expected[@]}"; do
  gh attestation verify "$directory/$asset" "${attestation[@]}" >/dev/null
  echo "provenance verified: $asset"
done

# 3. The image and the chart: the digests the signed digests.txt records, each signed and attested.
# Exactly one line of each kind.
recorded() { # $1 = kind
  local refs
  refs="$(awk -v kind="$1" '$1 == kind { print $2 }' "$directory/digests.txt")"
  [[ "$refs" =~ ^[^[:space:]@]+@sha256:[0-9a-f]{64}$ ]] || { echo "digests.txt: no single $1 digest" >&2; return 1; }
  echo "$refs"
}
image="$(recorded index)"
image_amd64="$(recorded linux/amd64)"
image_arm64="$(recorded linux/arm64)"
chart="$(recorded chart)"
for ref in "$image" "$image_amd64" "$image_arm64" "$chart"; do
  cosign verify "${certificate[@]}" "$ref" >/dev/null
  echo "signature verified: $ref"
done
platforms="$(skopeo inspect --raw "docker://$image" |
  jq -r '[.manifests[] | "\(.platform.os)/\(.platform.architecture) \(.digest)"] | sort | join(",")')"
if [[ "$platforms" != "linux/amd64 ${image_amd64#*@},linux/arm64 ${image_arm64#*@}" ]]; then
  echo "the image index holds $platforms, not the two recorded platforms" >&2
  exit 1
fi
for ref in "$image" "$chart"; do
  gh attestation verify "oci://$ref" "${attestation[@]}" >/dev/null
  echo "provenance verified: $ref"
done

# 4. The chart in the registry is the checksummed asset, and it installs that image by default.
mkdir "$work/chart"
helm pull "oci://$chart" --destination "$work/chart"
cmp "$work/chart/engawa@sha256-${chart##*@sha256:}.tgz" "$directory/engawa-$plain.tgz"
values="$(helm show values "$directory/engawa-$plain.tgz")"
pinned="$(yq '.image.repository + "@" + .image.digest' <<<"$values")"
[[ "$pinned" == "$image" ]] || { echo "the chart installs $pinned, not $image" >&2; exit 1; }

# 5. The artifacts themselves.
if [[ "$platform" == linux-* ]]; then
  "$root/scripts/release-smoke.sh" "$directory/engawactl-$version-$platform.tar.gz" "oci://$chart"
else
  tar -xOzf "$directory/engawactl-$version-$platform.tar.gz" engawactl >"$work/engawactl"
  chmod 0755 "$work/engawactl"
  said="$("$work/engawactl" --version)"
  [[ "$said" == "engawactl $plain" ]] || { echo "engawactl says '$said'" >&2; exit 1; }
fi
echo "release $version verified on $platform"
