#!/usr/bin/env bash
# Fails if crates/zeabur-catalog/bundle differs from Zeabur Catalog at the commit
# pinned in its PINNED file. Reads it from RECS_DIR (a local clone) when set, otherwise
# from GitHub (scripts/vendor-catalog.sh).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
bundle="$root/crates/zeabur-catalog/bundle"
commit=$(sed -n 's/^commit = //p' "$bundle/PINNED")
[[ "$commit" =~ ^[0-9a-f]{40}$ ]] || { echo "error: no commit in $bundle/PINNED" >&2; exit 1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
"$root/scripts/vendor-catalog.sh" "$commit" "$tmp/upstream" >/dev/null
if ! diff -r --exclude PINNED "$bundle" "$tmp/upstream" >&2; then
  echo "error: crates/zeabur-catalog/bundle differs from Zeabur Catalog $commit" >&2
  exit 1
fi
echo "crates/zeabur-catalog/bundle matches Zeabur Catalog $commit"
