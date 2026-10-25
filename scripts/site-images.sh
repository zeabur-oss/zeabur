#!/usr/bin/env bash
# Takes the landing page's screenshots (docs/site-shots.yaml) from the demo into
# docs/images/site/<locale>/<theme>/<id>.avif, at twice the pixel density and at most 150 KB each.
# Run inside `nix develop`.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/target/site-shots"
rm -rf "$out"
ZEABUR_SHOTS_SCALE=2 ZEABUR_SHOTS_BUDGET_KB=150 "$root/scripts/docs-shots.sh" "$root/docs/site-shots.yaml" "$out"
rm -rf "$root/docs/images/site"
for locale in en zh-TW zh-CN; do
  for theme in light dark; do
    mkdir -p "$root/docs/images/site/$locale/$theme"
    cp "$out/$locale/$theme"/*.avif "$root/docs/images/site/$locale/$theme/"
  done
done
