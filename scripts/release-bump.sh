#!/usr/bin/env bash
# The rest of a version bump, after knope has set Cargo.toml and desktop/Cargo.toml (knope.toml's
# prepare-release workflow): the workspace crates in both lock files, the chart, the web packages,
# and openapi.yaml with the web client generated from it. Run inside `nix develop`.
#   scripts/release-bump.sh v0.2.0
set -euo pipefail

version=${1:?usage: release-bump.sh <vVERSION>}
plain=${version#v}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
source scripts/lib.sh

# Each edit must change its file: a file that no longer has the line stops the bump.
set_line() {
  local file=$1 pattern=$2 line=$3
  grep -qE "$pattern" "$file" || { echo "$file has no line matching $pattern" >&2; exit 1; }
  sed -E -i.bak "s|$pattern|$line|" "$file"
  rm "$file.bak"
}

cargo update --workspace --offline
cargo update --workspace --offline --manifest-path desktop/Cargo.toml
set_line deploy/chart/zeabur/Chart.yaml '^version: .*$' "version: $plain"
set_line deploy/chart/zeabur/Chart.yaml '^appVersion: .*$' "appVersion: \"$plain\""
for manifest in web/package.json crates/zeabur-installer/web/package.json; do
  set_line "$manifest" '^  "version": "[^"]*"' "  \"version\": \"$plain\""
done
# Both take the version from CARGO_PKG_VERSION; the release job has only the root's packages.
(cd web && bun ci)
cargo xtask openapi
