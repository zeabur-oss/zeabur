#!/usr/bin/env bash
# The images Zeabur runs by default and does not publish, as the code pins them
# (crates/zeabur-api/src/settings.rs), as digests.txt lines: `<label> index  <ref>`. Each must be a
# reference by digest to an index that the registry still serves for linux/amd64 and linux/arm64.
# digests.txt lists them for mirroring; they are neither signed nor attested. Run inside
# `nix develop`.
#   scripts/release-default-images.sh
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
settings=$root/crates/zeabur-api/src/settings.rs
for pair in debug:DEBUG_IMAGE tools:TOOLS_IMAGE; do
  label=${pair%%:*}
  constant=${pair#*:}
  ref=$(sed -nE "s/^pub const $constant: &str = \"([^\"]+)\";$/\1/p" "$settings")
  [[ "$ref" =~ ^[^[:space:]@]+@sha256:[0-9a-f]{64}$ ]] ||
    { echo "settings.rs has no $constant by digest: '$ref'" >&2; exit 1; }
  platforms=$(crane manifest "$ref" | jq -r '.manifests[]?.platform | "\(.os)/\(.architecture)"')
  for want in linux/amd64 linux/arm64; do
    grep -qx "$want" <<<"$platforms" || { echo "$ref has no $want image" >&2; exit 1; }
  done
  echo "$label index  $ref"
done
