#!/usr/bin/env bash
# Fails if crates/zeabur-catalog/bundle differs from the recommendations catalogue at the commit
# pinned in its PINNED file. Reads the catalogue from RECS_DIR (a local clone) when set, otherwise
# from GitHub.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
bundle="$root/crates/zeabur-catalog/bundle"
commit=$(sed -n 's/^commit = //p' "$bundle/PINNED")
[[ "$commit" =~ ^[0-9a-f]{40}$ ]] || { echo "error: no commit in $bundle/PINNED" >&2; exit 1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/upstream"
if [[ -n "${RECS_DIR:-}" ]]; then
  git -C "$RECS_DIR" archive "$commit" capabilities LICENSE | tar -x -C "$tmp/upstream"
else
  curl --fail --silent --show-error --location --proto '=https' \
    "https://codeload.github.com/zeabur/catalog/tar.gz/$commit" |
    tar -xz -C "$tmp/upstream" --strip-components=1 "recommendations-$commit/capabilities" \
      "recommendations-$commit/LICENSE"
fi
if ! diff -r --exclude PINNED "$bundle" "$tmp/upstream" >&2; then
  echo "error: crates/zeabur-catalog/bundle differs from recommendations $commit" >&2
  exit 1
fi
echo "crates/zeabur-catalog/bundle matches recommendations $commit"
