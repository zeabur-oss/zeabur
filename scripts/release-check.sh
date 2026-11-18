#!/usr/bin/env bash
# What a release of <version> needs before anything is built: every manifest carries the version,
# the chart runs no image of ghcr.io/zeabur-oss that the release does not publish, and the catalog
# and Zeabur Identity pins carry release tags that this release may pin. Run inside `nix develop`,
# after `bun install`. The Release pull request is opened only when this passes.
#   scripts/release-check.sh v0.2.0
set -euo pipefail

version=${1:?usage: release-check.sh <vVERSION>}
plain=${version#v}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
problems=()

# Every crate of both workspaces (their lock files too, through --locked).
for manifest in Cargo.toml desktop/Cargo.toml; do
  others=$(cargo metadata --manifest-path "$manifest" --locked --no-deps --format-version 1 |
    jq -r --arg v "$plain" '.packages[] | select(.version != $v) | "\(.name) \(.version)"')
  while read -r name found; do
    problems+=("$manifest: crate $name is $found, not $plain")
  done < <(printf '%s' "$others")
done
for field in version appVersion; do
  found=$(yq ".$field" deploy/chart/zeabur/Chart.yaml)
  [[ "$found" == "$plain" ]] ||
    problems+=("deploy/chart/zeabur/Chart.yaml has $field $found, not $plain")
done
for manifest in web/package.json crates/zeabur-installer/web/package.json; do
  found=$(jq -r .version "$manifest")
  [[ "$found" == "$plain" ]] || problems+=("$manifest has version $found, not $plain")
done
found=$(yq .info.version crates/zeabur-api/openapi.yaml)
[[ "$found" == "$plain" ]] ||
  problems+=("crates/zeabur-api/openapi.yaml has version $found, not $plain")
found=$(sed -n 's/^ \* OpenAPI spec version: //p' web/src/api/zeabur.ts)
[[ "$found" == "$plain" ]] ||
  problems+=("web/src/api/zeabur.ts has OpenAPI spec version $found, not $plain")

# The release publishes ghcr.io/zeabur-oss/zeabur and ghcr.io/zeabur-oss/proxy (release.yml).
images=$(scripts/chart-images.sh deploy/chart/zeabur)
while read -r ref; do
  repository=${ref%%@*}
  [[ "$repository" =~ ^(.*):[^:/]+$ ]] && repository=${BASH_REMATCH[1]}
  case "$repository" in
    ghcr.io/zeabur-oss/zeabur | ghcr.io/zeabur-oss/proxy) ;;
    ghcr.io/zeabur-oss/*) problems+=("the chart runs $ref, which the release does not publish") ;;
  esac
done <<<"$images"

# The catalog commit in flake.lock and the tag or commit in ZEABUR_IDENTITY_PINNED.
for pin in "catalog ${ZEABUR_CATALOG_COMMIT:?run inside nix develop}" \
  "identity $(<ZEABUR_IDENTITY_PINNED)"; do
  read -r repository commit <<<"$pin"
  if ! failed=$(git ls-remote --tags "https://github.com/zeabur-oss/$repository" |
    bun scripts/versions.ts pin-tag "zeabur-oss/$repository" "$version" "$commit" 2>&1 >/dev/null); then
    problems+=("$failed")
  fi
done

if ((${#problems[@]})); then
  printf '%s\n' "${problems[@]}" >&2
  exit 1
fi
