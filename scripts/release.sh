#!/usr/bin/env bash
# The release, one stage per call, as zeabur-oss/.github's shared release workflow runs it
# (.github/workflows/release.yml, docs/internal/releasing.md). Each stage enters the Nix dev shell
# itself: the one of the checkout it is called from, so that verify-only checks a tag's release
# with a newer commit's scripts in the tag's dev shell.
#
#   scripts/release.sh check   <version>                                the version is the
#                                                                       manifests', with a
#                                                                       CHANGELOG.md section
#   scripts/release.sh build   <version> <os> <arch> <dist>             build one platform and smoke
#                                                                       it (release-build.sh,
#                                                                       release-smoke.sh)
#   scripts/release.sh publish <version> <artifacts> <published>        the image and the chart by
#                                                                       digest, signed
#                                                                       (release-publish.sh)
#   scripts/release.sh finish  <version> <os> <arch> <artifacts> <dist> the installer, pinned to
#                                                                       what publish pushed
#   scripts/release.sh assets  <version> <artifacts> <release>          the assets, SHA256SUMS
#                                                                       signed, and the notes
#                                                                       (release-assets.sh)
#   scripts/release.sh verify  <version> <os> <arch> <downloaded>       the draft as a user gets it
#                                                                       (release-verify.sh)
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
    grep -q "^## \[$plain\]" CHANGELOG.md ||
      { echo "CHANGELOG.md has no section ## [$plain]" >&2; status=1; }
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
    # The documentation screenshots (docs/internal/screenshots.md), once; the website downloads
    # them by version.
    if [[ "$os-$arch" == linux-amd64 ]]; then
      shots="$root/target/release-shots"
      rm -rf "$shots"
      "$scripts/docs-shots.sh" docs/shots.yaml "$shots" "$version"
      mv "$shots/docs-shots-$version.tar" "$dist/"
    fi
    ls -l "$dist"
    ;;

  publish)
    [[ $# == 4 ]] || usage
    "$scripts/release-publish.sh" "$version" "$3" "$4"
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
    "$scripts/release-assets.sh" "$version" "$3" "$4"
    ;;

  verify)
    [[ $# == 5 ]] || usage
    platform "$3" "$4"
    "$scripts/release-verify.sh" "$version" "$5"
    ;;

  *) usage ;;
esac
