#!/usr/bin/env bash
# The release, one stage per call, as zeabur-oss/.github's shared release workflow runs it
# (.github/workflows/release.yml, docs/internal/releasing.md). Each stage enters the Nix dev shell
# itself: the one of the checkout it is called from, so that verify-only checks a tag's release
# with a newer commit's scripts in the tag's dev shell. zeabur-oss-release (zeabur-oss/.github,
# release/) pushes, signs, writes the checksums and notes, and verifies; this script keeps what is
# Zeabur's own.
#
#   scripts/release.sh check   <version>                                the version is the
#                                                                       manifests', with a
#                                                                       CHANGELOG.md section
#   scripts/release.sh build   <version> <os> <arch> <dist>             build one platform and smoke
#                                                                       it (release-build.sh,
#                                                                       release-smoke.sh)
#   scripts/release.sh publish <version> <artifacts> <published>        the image and the chart by
#                                                                       digest, signed
#   scripts/release.sh finish  <version> <os> <arch> <artifacts> <dist> the installer, pinned to
#                                                                       what publish pushed
#   scripts/release.sh assets  <version> <artifacts> <release>          the assets, SHA256SUMS
#                                                                       signed, and the notes
#   scripts/release.sh verify  <version> <os> <arch> <downloaded>       the draft as a user gets it
#
# <artifacts> holds the earlier stages' outputs: candidate-<os>-<arch> (build), published (publish)
# and finish-<os>-<arch> (finish). The platforms are linux-amd64, linux-arm64 and darwin-arm64
# (zeaburctl only); each builds natively on its own runner.
set -euo pipefail

scripts="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$scripts/.." && pwd)"

# The marker tells a re-run inside the dev shell apart from a call outside it. The dev shell is
# the checkout's this is called from; a sparse checkout of scripts alone has no flake.
if [[ -z "${ZEABUR_RELEASE_SHELL:-}" ]]; then
  export ZEABUR_RELEASE_SHELL=1
  flake="$root"
  [[ -f "$flake/flake.nix" ]] || flake="$PWD"
  exec nix develop "$flake" --command "$0" "$@"
fi

usage() {
  cat >&2 <<'EOF'
usage: scripts/release.sh check   <vVERSION>
       scripts/release.sh build   <vVERSION> <os> <arch> <dist>
       scripts/release.sh publish <vVERSION> <artifacts> <published>
       scripts/release.sh finish  <vVERSION> <os> <arch> <artifacts> <dist>
       scripts/release.sh assets  <vVERSION> <artifacts> <release>
       scripts/release.sh verify  <vVERSION> <os> <arch> <downloaded>
EOF
  exit 2
}

stage="${1:-}"
version="${2:-}"
[[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || usage
plain="${version#v}"

# The platform of the arguments, which must be the machine's: every build is native.
platform() { # $1 = os, $2 = arch
  local here
  case "$(uname -s):$(uname -m)" in
    Linux:x86_64) here=linux-amd64 ;;
    Linux:aarch64) here=linux-arm64 ;;
    Darwin:arm64) here=darwin-arm64 ;;
    *) here="$(uname -s):$(uname -m)" ;;
  esac
  case "$1-$2" in
    linux-amd64 | linux-arm64 | darwin-arm64) ;;
    *) echo "no release for $1-$2: linux-amd64, linux-arm64 and darwin-arm64" >&2; exit 2 ;;
  esac
  [[ "$1-$2" == "$here" ]] || { echo "this machine is $here, not $1-$2" >&2; exit 2; }
}

# Every release asset; SHA256SUMS lists exactly these.
assets=(digests.txt "zeabur-$plain.tgz")
for build in linux-amd64 linux-arm64 darwin-arm64; do
  assets+=("zeaburctl-$version-$build.tar.gz")
done
for arch in amd64 arm64; do
  assets+=("zeabur-$version-linux-$arch.tar.gz" "zeabur-installer-$version-linux-$arch.tar.gz")
done

# The one reference digests.txt records for a kind: linux/amd64, linux/arm64, index or chart.
recorded() { # $1 = digests.txt, $2 = kind
  local ref
  ref="$(awk -v kind="$2" '$1 == kind { print $2 }' "$1")"
  [[ "$ref" =~ ^[^[:space:]@]+@sha256:[0-9a-f]{64}$ ]] || { echo "$1: no single $2 digest" >&2; return 1; }
  echo "$ref"
}

# KEY=value lines (release-pins.sh, published/outputs.env) into the environment, never evaluated.
export_lines() {
  local key value
  while IFS='=' read -r key value; do
    [[ "$key" =~ ^ZEABUR_PIN_[A-Z0-9_]+$ ]] || { echo "unexpected line: $key" >&2; return 1; }
    export "$key=$value"
  done
}

case "$stage" in
  check)
    [[ $# == 2 ]] || usage
    cd "$root"
    status=0
    # Every crate of the workspace (Cargo.lock included, through --locked), the chart, the web
    # packages and the API document carry the version.
    while read -r name crate; do
      [[ "$crate" == "$plain" ]] || { echo "crate $name is $crate, not $plain" >&2; status=1; }
    done < <(cargo metadata --locked --no-deps --format-version 1 |
      jq -r '.packages[] | "\(.name) \(.version)"')
    chart="$(yq '.version + " " + .appVersion' deploy/chart/zeabur/Chart.yaml)"
    [[ "$chart" == "$plain $plain" ]] ||
      { echo "deploy/chart/zeabur/Chart.yaml has version and appVersion $chart, not $plain" >&2; status=1; }
    for manifest in web/package.json crates/zeabur-installer/web/package.json; do
      [[ "$(jq -r .version "$manifest")" == "$plain" ]] ||
        { echo "$manifest has version $(jq -r .version "$manifest"), not $plain" >&2; status=1; }
    done
    [[ "$(yq .info.version crates/zeabur-api/openapi.yaml)" == "$plain" ]] ||
      { echo "crates/zeabur-api/openapi.yaml is not at $plain" >&2; status=1; }
    zeabur-oss-release notes CHANGELOG.md "$version" >/dev/null || status=1
    exit "$status"
    ;;

  build)
    [[ $# == 5 ]] || usage
    os="$3" arch="$4"
    platform "$os" "$arch"
    mkdir -p "$5"
    dist="$(cd "$5" && pwd)"
    cd "$root"
    # A tag's binaries embed the catalog bundle, which must be the one the catalog's release
    # signed, and its installer installs the pinned Zeabur Identity release: both pins must
    # resolve before anything is built. A rehearsal builds whatever the pins name.
    if [[ "${GITHUB_EVENT_NAME:-}" == push ]]; then
      "$scripts/release-pins.sh" >/dev/null
    fi
    "$scripts/release-build.sh" "$version" "$arch" "$dist"
    if [[ "$os" == linux ]]; then
      # The archive, the chart and the image as built, on the development cluster with Zeabur
      # Identity at ZEABUR_IDENTITY_PINNED (scripts/dev-cluster.sh fetches it).
      "$scripts/release-smoke.sh" "$dist/zeaburctl-$version-linux-$arch.tar.gz" deploy/chart/zeabur \
        "zeabur:release-$arch"
      # A rehearsal publishes nothing, so finish never runs: a development installer, without
      # pins, shows that it builds. It is not kept.
      if [[ "${GITHUB_EVENT_NAME:-}" != push ]]; then
        installer="$(mktemp -d)"
        "$scripts/release-build.sh" "$version" "$arch" "$installer" installer
        rm -rf "$installer"
      fi
    fi
    ls -l "$dist"
    ;;

  publish)
    [[ $# == 4 ]] || usage
    candidates="$(cd "$3" && pwd)"
    mkdir -p "$4"
    out="$(cd "$4" && pwd)"
    : "${GITHUB_REPOSITORY:?}" "${GITHUB_ACTOR:?}" "${GH_TOKEN:?}"
    owner="${GITHUB_REPOSITORY%%/*}"
    owner="${owner,,}"
    image="ghcr.io/$owner/zeabur"
    charts="ghcr.io/$owner/charts"
    work="$(mktemp -d)"
    trap 'rm -rf "$work"' EXIT
    # Registry logins live in this run's own files, gone when it ends. Helm reads the same file.
    export DOCKER_CONFIG="$work/docker" HELM_REGISTRY_CONFIG="$work/docker/config.json"
    mkdir -p "$DOCKER_CONFIG"

    # Everything that can refuse does so before the first write. zeabur-oss-release refuses an
    # existing tag for each push; this refuses both before the first, so a release never stops
    # halfway on a tag that was there from the start. Recovering from a failed run is a
    # maintainer's decision (docs/internal/releasing.md).
    if gh release view "$version" --repo "$GITHUB_REPOSITORY" >/dev/null 2>&1; then
      echo "release $version already exists; inspect it before trying again" >&2
      exit 1
    fi
    printf '%s' "$GH_TOKEN" | docker login ghcr.io --username "$GITHUB_ACTOR" --password-stdin
    printf '%s' "$GH_TOKEN" | helm registry login ghcr.io --username "$GITHUB_ACTOR" --password-stdin
    for ref in "$image:$plain" "$charts/zeabur:$plain"; do
      if skopeo inspect --raw --authfile "$DOCKER_CONFIG/config.json" "docker://$ref" >/dev/null 2>&1; then
        echo "$ref already exists; inspect it before trying again" >&2
        exit 1
      fi
    done
    for build in linux-amd64 linux-arm64 darwin-arm64; do
      [[ -f "$candidates/candidate-$build/zeaburctl-$version-$build.tar.gz" ]] ||
        { echo "missing the $build build" >&2; exit 1; }
    done

    rm -f "$out/digests.txt" "$out/subjects.json"
    zeabur-oss-release push-image --repository "$image" --tag "$plain" \
      --digests "$out/digests.txt" --subjects "$out/subjects.json" \
      "linux/amd64=$candidates/candidate-linux-amd64/image.tar" \
      "linux/arm64=$candidates/candidate-linux-arm64/image.tar"
    index="$(recorded "$out/digests.txt" index)"
    index="${index#*@}"

    # The chart: the image pinned to that index, at the version of Chart.yaml (checked equal to
    # the release's by `check`).
    cp -R "$root/deploy/chart/zeabur" "$work/chart"
    # sed rather than `yq -i`, which would drop the blank lines between the documented values.
    sed -i "s|^  digest: \"\"\$|  digest: \"$index\"|" "$work/chart/values.yaml"
    INDEX="$index" yq -e '.image.digest == strenv(INDEX)' "$work/chart/values.yaml" >/dev/null ||
      { echo "could not pin image.digest in values.yaml" >&2; exit 1; }
    helm lint --strict "$work/chart" --set publicUrl=https://zeabur.example.com \
      --set issuer.url=https://identity.example.com --set issuer.clientSecret.existingSecret=zeabur-oidc
    helm package "$work/chart" --destination "$out"
    zeabur-oss-release push-chart --repository "$charts" \
      --digests "$out/digests.txt" --subjects "$out/subjects.json" "$out/zeabur-$plain.tgz"

    # What the installer pins (`finish`).
    {
      echo "ZEABUR_PIN_ZEABUR_IMAGE=$image@$index"
      echo "ZEABUR_PIN_ZEABUR_CHART_SHA256=$(sha256sum "$out/zeabur-$plain.tgz" | cut -d ' ' -f 1)"
    } >"$out/outputs.env"
    cat "$out/subjects.json" "$out/digests.txt"
    ;;

  finish)
    [[ $# == 6 ]] || usage
    os="$3" arch="$4"
    platform "$os" "$arch"
    artifacts="$(cd "$5" && pwd)"
    mkdir -p "$6"
    dist="$(cd "$6" && pwd)"
    cd "$root"
    # The installer runs on Linux servers only; release.yml finishes on Linux only.
    [[ "$os" == linux ]] || { echo "no installer for $os-$arch" >&2; exit 2; }
    # The catalog and Zeabur Identity releases, and the image and the chart that publish pushed.
    export_lines < <("$scripts/release-pins.sh")
    export_lines <"$artifacts/published/outputs.env"
    "$scripts/release-build.sh" "$version" "$arch" "$dist" installer
    ;;

  assets)
    [[ $# == 4 ]] || usage
    candidates="$(cd "$3" && pwd)"
    mkdir -p "$4/assets"
    out="$(cd "$4" && pwd)"
    files=(
      "$candidates/published/digests.txt"
      "$candidates/published/zeabur-$plain.tgz"
      "$candidates/candidate-darwin-arm64/zeaburctl-$version-darwin-arm64.tar.gz"
    )
    for arch in amd64 arm64; do
      files+=(
        "$candidates/candidate-linux-$arch/zeaburctl-$version-linux-$arch.tar.gz"
        "$candidates/candidate-linux-$arch/zeabur-$version-linux-$arch.tar.gz"
        "$candidates/finish-linux-$arch/zeabur-installer-$version-linux-$arch.tar.gz"
      )
    done
    for file in "${files[@]}"; do
      [[ -f "$file" ]] || { echo "missing $file" >&2; exit 1; }
    done
    cp "${files[@]}" "$out/assets/"
    zeabur-oss-release checksums "$out/assets"
    zeabur-oss-release notes "$root/CHANGELOG.md" "$version" --docs https://oss.zeabur.com/docs/zeabur/ \
      >"$out/notes.md"
    ;;

  verify)
    [[ $# == 5 ]] || usage
    platform "$3" "$4"
    directory="$(cd "$5" && pwd)"
    source_ref="${RELEASE_SOURCE_REF:-refs/tags/$version}"
    [[ "$source_ref" == "refs/tags/$version" ]] ||
      { echo "RELEASE_SOURCE_REF is $source_ref, not the tag refs/tags/$version" >&2; exit 2; }
    work="$(mktemp -d)"
    trap 'rm -rf "$work"' EXIT
    # A fresh, empty registry configuration: a publisher's login must not hide a private package.
    export DOCKER_CONFIG="$work/docker" HELM_REGISTRY_CONFIG="$work/docker/config.json"
    mkdir -p "$DOCKER_CONFIG"
    release=(--repository zeabur-oss/zeabur --version "$version")

    zeabur-oss-release verify-assets "${release[@]}" "$directory" "${assets[@]}"
    zeabur-oss-release verify-image "${release[@]}" --digests "$directory/digests.txt" linux/amd64 linux/arm64
    zeabur-oss-release verify-chart "${release[@]}" --digests "$directory/digests.txt" "$directory/zeabur-$plain.tgz"
    # The chart installs that image by default.
    image="$(recorded "$directory/digests.txt" index)"
    pinned="$(yq '.image.repository + "@" + .image.digest' <<<"$(helm show values "$directory/zeabur-$plain.tgz")")"
    [[ "$pinned" == "$image" ]] || { echo "the chart installs $pinned, not $image" >&2; exit 1; }

    # The artifacts themselves: on Linux, the published chart on a disposable k3s cluster whose
    # node pulls the published image; on macOS, zeaburctl.
    if [[ "$3" == linux ]]; then
      "$scripts/release-smoke.sh" "$directory/zeaburctl-$version-$3-$4.tar.gz" "oci://$(recorded "$directory/digests.txt" chart)"
    else
      tar -xOzf "$directory/zeaburctl-$version-$3-$4.tar.gz" zeaburctl >"$work/zeaburctl"
      chmod 0755 "$work/zeaburctl"
      said="$("$work/zeaburctl" --version)"
      [[ "$said" == "zeaburctl $plain" ]] || { echo "zeaburctl says '$said'" >&2; exit 1; }
    fi
    echo "release $version verified on $3-$4"
    ;;

  *) usage ;;
esac
