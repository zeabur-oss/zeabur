#!/usr/bin/env bash
# Takes the screenshots of one image set (docs/internal/screenshots.md), the Playwright tests in
# web/e2e/shots/<set>.ts, from the demo server (crates/zeabur-api/examples/demo.rs), the installer's
# pages with captured responses or the desktop app's pages with mocked commands, in each locale and
# theme, as AVIF. Run inside `nix develop`.
#   scripts/docs-shots.sh [set] [out] [-- playwright arguments]
# <set> defaults to docs. <out> defaults to the set's `out`, else target/docs-shots/<set>. Images
# in <out> that this run does not take stay, unless the set sets `clean`.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
set=docs
out=""
if [[ $# -gt 0 && "$1" != -- ]]; then set="$1" && shift; fi
if [[ $# -gt 0 && "$1" != -- ]]; then out="$1" && shift; fi
if [[ "${1:-}" == -- ]]; then shift; fi
# The demo server and the browser both take this as the time now, so dates are the same on every run.
export ZEABUR_DEMO_NOW="${ZEABUR_DEMO_NOW:-2026-10-26T09:00:00Z}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

settings="$(cd "$root/web" && bun e2e/shots/sets.ts "$set")"
kept="$(jq -r '.out // ""' <<<"$settings")"
file="$(jq -r .file <<<"$settings")"
budget=$(($(jq -r .budget <<<"$settings") * 1024))
out="${out:-${kept:+$root/$kept}}"
out="${out:-$root/target/docs-shots/$set}"
mkdir -p "$out"
out="$(realpath "$out")"

if jq -e '.apps | index("zeabur")' <<<"$settings" >/dev/null; then
  "$root/scripts/build-ui.sh"
  ZEABUR_UI_DIST="$root/web/dist" cargo build --locked -p zeabur-api --example demo
  export ZEABUR_SHOTS_DEMO="${CARGO_TARGET_DIR:-$root/target}/debug/examples/demo"
fi
if jq -e '.apps | index("installer")' <<<"$settings" >/dev/null; then
  "$root/scripts/build-installer-ui.sh"
  # Written once here, not by each preview server: they read them while the shots run.
  (cd "$root/crates/zeabur-installer/web" &&
    bun -e 'import { generated } from "./scripts/dev-api.ts"; import { writeFixtures } from "./scripts/fixtures.ts"; writeFixtures(generated)')
fi
if jq -e '.apps | index("desktop")' <<<"$settings" >/dev/null; then
  (cd "$root/desktop/web" && bun ci && bun run build)
fi

(
  cd "$root/web"
  ZEABUR_SHOTS_OUT="$work/png" bunx playwright test -c playwright.shots.config.ts \
    --project "$set/*" --output="$root/target/docs-shots-results" "$@"
)

# Encode into $work/avif first, so a failed run leaves <out> as it was.
mkdir -p "$work/avif"
over=()
while IFS= read -r png; do
  IFS=/ read -r locale theme id <<<"${png#"$work/png/$set/"}"
  id="${id%.png}"
  name="${file//\{locale\}/$locale}"
  # {docs} is the locale's docs folder: empty for English, "<locale>/" for a translation.
  docs="$locale/"
  [[ "$locale" == en ]] && docs=""
  name="${name//\{docs\}/$docs}"
  name="${name//\{theme\}/$theme}"
  name="${name//\{id\}/$id}"
  avif="$work/avif/$name"
  mkdir -p "$(dirname "$avif")"
  # Lower the quality in steps until the file fits the budget, down to 40.
  for quality in 70 65 60 55 50 45 40; do
    avifenc --jobs all --speed 4 --qcolor "$quality" --yuv 444 "$png" "$avif" >/dev/null
    size=$(wc -c <"$avif" | tr -d ' ')
    if ((size <= budget)); then break; fi
  done
  if ((size > budget)); then over+=("$name ($size bytes)"); fi
done < <(find "$work/png" -name '*.png' | sort)

if ((${#over[@]} > 0)); then
  echo "error: over the budget of $budget bytes:" >&2
  printf '  %s\n' "${over[@]}" >&2
  exit 1
fi

# `clean` empties each folder the run writes into, so images whose shot is gone go too. A run
# limited by Playwright arguments takes only some of them, so it keeps the others.
if [[ "$(jq -r .clean <<<"$settings")" == true && $# -eq 0 ]]; then
  (cd "$work/avif" && find . -name '*.avif' -exec dirname {} \; | sort -u) | while IFS= read -r dir; do
    rm -rf "${out:?}/$dir"
  done
fi
cp -R "$work/avif/." "$out/"
echo "$(find "$work/avif" -name '*.avif' | wc -l | tr -d ' ') images in $out"
