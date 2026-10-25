#!/usr/bin/env bash
# Usage: scripts/vendor-catalog.sh <ref> [dir]
# Writes components/, templates/error-pages/, LICENSE and, when it exists,
# measurements/templates/error-pages.json of Zeabur Catalog at <ref> (a tag, branch or commit) to dir
# (default crates/zeabur-catalog/bundle), and a PINNED file with the ref and its commit. Reads
# from RECS_DIR (a local clone) when set, otherwise from the GitHub API with GH_TOKEN, which
# needs read access to the repository while it is private.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
ref="${1:?usage: scripts/vendor-catalog.sh <ref> [dir]}"
dir="${2:-$root/crates/zeabur-catalog/bundle}"
api=https://api.github.com/repos/zeabur-oss/catalog
get() {
  curl --fail --silent --show-error --location --proto '=https' \
    ${GH_TOKEN:+--header "Authorization: Bearer $GH_TOKEN"} "$@"
}

if [[ -n "${RECS_DIR:-}" ]]; then
  commit=$(git -C "$RECS_DIR" rev-parse --verify --quiet "$ref^{commit}") || true
  if git -C "$RECS_DIR" show-ref --verify --quiet "refs/tags/$ref"; then tag=1; fi
else
  commit=$(get --header "Accept: application/vnd.github.sha" "$api/commits/$ref") || true
  if get --output /dev/null "$api/git/ref/tags/$ref" 2>/dev/null; then tag=1; fi
fi
[[ "${commit:-}" =~ ^[0-9a-f]{40}$ ]] || { echo "error: $ref is not a commit in Zeabur Catalog" >&2; exit 1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
if [[ -n "${RECS_DIR:-}" ]]; then
  paths=(components templates/error-pages LICENSE)
  if git -C "$RECS_DIR" cat-file -e "$commit:measurements/templates/error-pages.json" 2>/dev/null; then
    paths+=(measurements/templates/error-pages.json)
  fi
  git -C "$RECS_DIR" archive "$commit" "${paths[@]}" | tar -x -C "$tmp"
else
  get "$api/tarball/$commit" | tar -xz -C "$tmp" --strip-components=1
fi
[[ -d "$tmp/components" ]] || { echo "error: Zeabur Catalog $commit has no components/" >&2; exit 1; }
[[ -d "$tmp/templates/error-pages" ]] ||
  { echo "error: Zeabur Catalog $commit has no templates/error-pages/" >&2; exit 1; }

mkdir -p "$dir"
rm -rf "$dir/components" "$dir/templates" "$dir/measurements" "$dir/LICENSE"
mkdir -p "$dir/templates"
mv "$tmp/components" "$tmp/LICENSE" "$dir/"
mv "$tmp/templates/error-pages" "$dir/templates/"
if [[ -f "$tmp/measurements/templates/error-pages.json" ]]; then
  mkdir -p "$dir/measurements/templates"
  mv "$tmp/measurements/templates/error-pages.json" "$dir/measurements/templates/"
fi
if [[ -n "${tag:-}" ]]; then release=$ref; else release="$ref (unreleased)"; fi
printf 'release = %s\ncommit = %s\n' "$release" "$commit" >"$dir/PINNED"
echo "$dir: Zeabur Catalog $release at $commit"
