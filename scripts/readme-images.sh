#!/usr/bin/env bash
# Takes the README's screenshots (docs/readme-shots.yaml) from the demo into docs/images as
# <id>-<theme>.avif, each at most 30 KB. Run inside `nix develop`.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/target/readme-shots"
rm -rf "$out"
ZEABUR_SHOTS_SCALE=1 ZEABUR_SHOTS_BUDGET_KB=30 "$root/scripts/docs-shots.sh" "$root/docs/readme-shots.yaml" "$out"
for theme in light dark; do
  for avif in "$out/en/$theme"/*.avif; do
    cp "$avif" "$root/docs/images/$(basename "$avif" .avif)-$theme.avif"
  done
done
