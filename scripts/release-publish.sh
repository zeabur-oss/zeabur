#!/usr/bin/env bash
# Publishes the candidates that release-build.sh built and release-smoke.sh tried, compiling
# nothing: the image by digest and the chart pinned to that digest, each signed. The installer is
# built next with these digests (`release.sh finish`), and release-assets.sh then makes the
# release assets. Every signature is keyless (Sigstore, with the workflow's identity token); the
# shared release workflow attests what subjects.json lists. A maintainer publishes the draft
# release (docs/internal/releasing.md).
# CI only, from `scripts/release.sh publish`, inside `nix develop`; Docker is the runner's.
#   scripts/release-publish.sh v0.1.1 artifacts published
#
# <artifacts> holds one directory per build (candidate-linux-amd64, candidate-darwin-arm64, …);
# candidate-linux-amd64 also has the documentation screenshots. <out> receives notes.md,
# assets/ with digests.txt and the chart, subjects.json (the image index and the chart, to attest)
# and outputs.env (the installer's ZEABUR_PIN_ZEABUR_* values).
set -euo pipefail

version="${1:?usage: release-publish.sh <vVERSION> <artifacts> <out>}"
candidates="$(realpath "${2:?artifact directory required}")"
mkdir -p "${3:?output directory required}/assets"
out="$(realpath "$3")"
assets="$out/assets"
: "${GITHUB_REPOSITORY:?}" "${GITHUB_SHA:?}" "${GITHUB_ACTOR:?}" "${GITHUB_RUN_ID:?}" "${GITHUB_RUN_ATTEMPT:?}" "${GH_TOKEN:?}"
root="$(cd "$(dirname "$0")/.." && pwd)"
plain="${version#v}"
owner="${GITHUB_REPOSITORY%%/*}"
owner="${owner,,}"
image="ghcr.io/$owner/zeabur"
charts="ghcr.io/$owner/charts"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
# Registry logins live in this run's own files, gone when it ends. Helm reads the same file.
export DOCKER_CONFIG="$work/docker" HELM_REGISTRY_CONFIG="$work/docker/config.json"
mkdir -p "$DOCKER_CONFIG"
auth=(--authfile "$DOCKER_CONFIG/config.json")

# Everything that can refuse does so before the first write.
# The notes: this version's section of CHANGELOG.md, without its heading.
if ! awk -v heading="## [$plain]" '
  index($0, heading) == 1 { found = 1; next }
  found && /^## / { exit }
  found { print }
  END { if (!found) exit 1 }
' "$root/CHANGELOG.md" >"$out/notes.md"; then
  echo "CHANGELOG.md has no section ## [$plain]" >&2
  exit 1
fi
# A release, an image tag or a chart version once published is never replaced: recovering from a
# failed run is a maintainer's decision (docs/internal/releasing.md).
if gh release view "$version" --repo "$GITHUB_REPOSITORY" >/dev/null 2>&1; then
  echo "release $version already exists; inspect it before trying again" >&2
  exit 1
fi
printf '%s' "$GH_TOKEN" | docker login ghcr.io --username "$GITHUB_ACTOR" --password-stdin
printf '%s' "$GH_TOKEN" | helm registry login ghcr.io --username "$GITHUB_ACTOR" --password-stdin
for ref in "$image:$plain" "$charts/zeabur:$plain"; do
  if skopeo inspect --raw "${auth[@]}" "docker://$ref" >/dev/null 2>&1; then
    echo "$ref already exists; inspect it before trying again" >&2
    exit 1
  fi
done
for build in linux-amd64 linux-arm64 darwin-arm64; do
  [[ -f "$candidates/candidate-$build/zeaburctl-$version-$build.tar.gz" ]] ||
    { echo "missing the $build build" >&2; exit 1; }
done
[[ -f "$candidates/candidate-linux-amd64/docs-shots-$version.tar" ]] ||
  { echo "missing the documentation screenshots" >&2; exit 1; }

# The image: each platform's candidate by digest, then one index of exactly those two.
refs=()
for arch in amd64 arm64; do
  dir="$candidates/candidate-linux-$arch"
  candidate="$image:candidate-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT-$arch"
  # The input is this run's own smoked build, not a registry image: no signature policy applies.
  skopeo --insecure-policy copy "${auth[@]}" "docker-archive:$dir/image.tar" "docker://$candidate"
  digest="$(skopeo inspect "${auth[@]}" --format '{{.Digest}}' "docker://$candidate")"
  refs+=("$image@$digest")
  cosign sign --yes "$image@$digest"
  printf '%s  %s\n' "linux/$arch" "$image@$digest" >>"$assets/digests.txt"
done
docker manifest create "$image:$plain" "${refs[@]}"
docker manifest push "$image:$plain"
index="sha256:$(skopeo inspect --raw "${auth[@]}" "docker://$image:$plain" | sha256sum | cut -d ' ' -f 1)"
cosign sign --yes "$image@$index"
printf 'index  %s\n' "$image@$index" >>"$assets/digests.txt"

# The chart: the image pinned to the index above, at the version of Chart.yaml (checked equal to
# the release's by the workflow).
cp -R "$root/deploy/chart/zeabur" "$work/chart"
# sed rather than `yq -i`, which would drop the blank lines between the documented values.
sed -i "s|^  digest: \"\"\$|  digest: \"$index\"|" "$work/chart/values.yaml"
INDEX="$index" yq -e '.image.digest == strenv(INDEX)' "$work/chart/values.yaml" >/dev/null ||
  { echo "could not pin image.digest in values.yaml" >&2; exit 1; }
helm lint --strict "$work/chart" --set publicUrl=https://zeabur.example.com \
  --set issuer.url=https://identity.example.com --set issuer.clientSecret.existingSecret=zeabur-oidc
helm package "$work/chart" --destination "$assets"
helm push "$assets/zeabur-$plain.tgz" "oci://$charts"
chart_digest="sha256:$(skopeo inspect --raw "${auth[@]}" "docker://$charts/zeabur:$plain" | sha256sum | cut -d ' ' -f 1)"
cosign sign --yes "$charts/zeabur@$chart_digest"
printf 'chart  %s\n' "$charts/zeabur@$chart_digest" >>"$assets/digests.txt"

# What the shared workflow attests, and what the installer pins.
jq -n --arg image "$image" --arg index "$index" --arg chart "$charts/zeabur" --arg chart_digest "$chart_digest" \
  '[{name: $image, digest: $index}, {name: $chart, digest: $chart_digest}]' >"$out/subjects.json"
{
  echo "ZEABUR_PIN_ZEABUR_IMAGE=$image@$index"
  echo "ZEABUR_PIN_ZEABUR_CHART_SHA256=$(sha256sum "$assets/zeabur-$plain.tgz" | cut -d ' ' -f 1)"
} >"$out/outputs.env"
