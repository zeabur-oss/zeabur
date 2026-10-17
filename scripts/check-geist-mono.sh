#!/usr/bin/env bash
# Fails if the Geist Mono files in web/src/styles/geist-mono differ from the Vercel release pinned in
# web/src/styles/geist-mono/VENDORED, or if that release's archive no longer has the pinned SHA-256.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
dir="$root/web/src/styles/geist-mono"
field() { sed -n "s/^$1 = //p" "$dir/VENDORED"; }
url=$(field url)
sha=$(field sha256)
[[ "$url" == https://github.com/vercel/geist-font/releases/download/* ]] || {
  echo "error: unexpected url in $dir/VENDORED" >&2
  exit 1
}
[[ "$sha" =~ ^[0-9a-f]{64}$ ]] || { echo "error: no sha256 in $dir/VENDORED" >&2; exit 1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl --fail --silent --show-error --location --proto '=https' --output "$tmp/release.zip" "$url"
echo "$sha  $tmp/release.zip" | sha256sum --check --status || {
  echo "error: $url does not have the pinned SHA-256" >&2
  exit 1
}

status=0
while IFS= read -r line; do
  file=${line%% = *}
  path=${line#* = }
  # unzip reads [ and ] as wildcards, so escape them.
  pattern=${path//[/\\[}
  pattern=${pattern//]/\\]}
  unzip -p "$tmp/release.zip" "$pattern" >"$tmp/upstream"
  if ! cmp -s "$dir/$file" "$tmp/upstream"; then
    echo "error: web/src/styles/geist-mono/$file differs from $path in the release" >&2
    status=1
  fi
done < <(grep -E '^[A-Za-z0-9.-]+ = geist-font/' "$dir/VENDORED")
if [[ $status -eq 0 ]]; then echo "web/src/styles/geist-mono matches Geist $(field release)"; fi
exit $status
