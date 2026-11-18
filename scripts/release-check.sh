#!/usr/bin/env bash
# Checks that this commit can be released as <version>, and lists every problem it finds.
#
#   scripts/release-check.sh <vX.Y.Z>
set -euo pipefail

version=${1:?usage: release-check.sh <vX.Y.Z>}
plain=${version#v}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

problems=()
problem() {
  problems+=("$1")
}

# expect_version <file> <what> <found>
expect_version() {
  if [[ "$3" != "$plain" ]]; then
    problem "$1 has $2 $3, not $plain"
  fi
}

check_crates() {
  local manifest=$1
  local others
  # --locked also fails when the lock file has another version.
  others=$(
    cargo metadata --manifest-path "$manifest" --locked --no-deps --format-version 1 |
      jq -r --arg version "$plain" \
        '.packages[] | select(.version != $version) | "\(.name) \(.version)"'
  )
  local name found
  while read -r name found; do
    if [[ -n "$name" ]]; then
      problem "$manifest: crate $name is $found, not $plain"
    fi
  done <<<"$others"
}

# The release publishes only ghcr.io/zeabur-oss/zeabur and ghcr.io/zeabur-oss/proxy.
check_chart_images() {
  local images
  images=$(scripts/chart-images.sh deploy/chart/zeabur)
  local ref repository
  while read -r ref; do
    repository=${ref%%@*}
    if [[ "$repository" =~ ^(.*):[^:/]+$ ]]; then
      repository=${BASH_REMATCH[1]}
    fi
    case "$repository" in
      ghcr.io/zeabur-oss/zeabur | ghcr.io/zeabur-oss/proxy) ;;
      ghcr.io/zeabur-oss/*) problem "the chart runs $ref, which the release does not publish" ;;
    esac
  done <<<"$images"
}

# check_pin <repository> <commit or tag>: the pin must carry a release tag that this version may pin.
check_pin() {
  local error
  if ! error=$(
    git ls-remote --tags "https://github.com/zeabur-oss/$1" |
      bun scripts/versions.ts pin-tag "zeabur-oss/$1" "$version" "$2" 2>&1 >/dev/null
  ); then
    problem "$error"
  fi
}

check_crates Cargo.toml
check_crates desktop/Cargo.toml

chart=deploy/chart/zeabur/Chart.yaml
expect_version "$chart" version "$(yq .version "$chart")"
expect_version "$chart" appVersion "$(yq .appVersion "$chart")"

for package in web/package.json crates/zeabur-installer/web/package.json; do
  expect_version "$package" version "$(jq -r .version "$package")"
done

api=crates/zeabur-api/openapi.yaml
expect_version "$api" version "$(yq .info.version "$api")"

client=web/src/api/zeabur.ts
expect_version "$client" "OpenAPI spec version" \
  "$(sed -n 's/^ \* OpenAPI spec version: //p' "$client")"

check_chart_images

check_pin catalog "${ZEABUR_CATALOG_COMMIT:?run inside nix develop}"
check_pin identity "$(<ZEABUR_IDENTITY_PINNED)"

if ((${#problems[@]} > 0)); then
  printf '%s\n' "${problems[@]}" >&2
  exit 1
fi
