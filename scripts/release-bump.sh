#!/usr/bin/env bash
# Sets the release version everywhere that knope does not.
#
#   scripts/release-bump.sh <vX.Y.Z>
#
# knope runs it after it sets Cargo.toml and desktop/Cargo.toml (knope.toml, prepare-release).
set -euo pipefail

version=${1:?usage: release-bump.sh <vX.Y.Z>}
plain=${version#v}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
source scripts/lib.sh

# Replaces one line of a file. Stops when the line is not there, so a moved line is not missed.
replace_line() {
  local file=$1
  local pattern=$2
  local new_line=$3
  if ! grep -qE "$pattern" "$file"; then
    echo "$file has no line that matches $pattern" >&2
    exit 1
  fi
  sed -E -i.bak "s|$pattern|$new_line|" "$file"
  rm "$file.bak"
}

# The workspace crates in both lock files.
cargo update --workspace --offline
cargo update --workspace --offline --manifest-path desktop/Cargo.toml

replace_line deploy/chart/zeabur/Chart.yaml '^version: .*$' "version: $plain"
replace_line deploy/chart/zeabur/Chart.yaml '^appVersion: .*$' "appVersion: \"$plain\""

for package in web/package.json crates/zeabur-installer/web/package.json; do
  replace_line "$package" '^  "version": "[^"]*"' "  \"version\": \"$plain\""
done

# openapi.yaml and the web client take the version from CARGO_PKG_VERSION.
(cd web && bun ci)
cargo xtask openapi
