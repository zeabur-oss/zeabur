#!/usr/bin/env bash
# Fails if the files vendored from Genkan (web/src/styles/genkan) differ from Genkan at the commit
# pinned in web/src/styles/genkan/VENDORED. Reads Genkan from GENKAN_DIR (a local clone) when set,
# otherwise from GitHub.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
dir="$root/web/src/styles/genkan"
commit=$(sed -n 's/^commit = //p' "$dir/VENDORED")
[[ "$commit" =~ ^[0-9a-f]{40}$ ]] || { echo "error: no commit in $dir/VENDORED" >&2; exit 1; }

upstream() { # $1 = path in Genkan
  if [[ -n "${GENKAN_DIR:-}" ]]; then
    git -C "$GENKAN_DIR" show "$commit:$1"
  else
    curl --fail --silent --show-error --location --proto '=https' \
      "https://raw.githubusercontent.com/engawahq/genkan/$commit/$1"
  fi
}

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
upstream ui/styles/shadcn.css >"$tmp/shadcn.css"
upstream crates/genkan-ui/assets/geist-latin-wght-normal.woff2 >"$tmp/geist-latin-wght-normal.woff2"
upstream crates/genkan-ui/assets/LICENSE-Geist.txt >"$tmp/LICENSE-Geist.txt"

status=0
for file in shadcn.css geist-latin-wght-normal.woff2 LICENSE-Geist.txt; do
  if ! cmp -s "$dir/$file" "$tmp/$file"; then
    echo "error: web/src/styles/genkan/$file differs from Genkan $commit" >&2
    status=1
  fi
done
if [[ $status -eq 0 ]]; then echo "web/src/styles/genkan matches Genkan $commit"; fi
exit $status
