#!/usr/bin/env bash
# Takes the screenshots that a manifest declares (docs/internal/screenshots.md), from the demo
# server (crates/zeabur-api/examples/demo.rs) or the installer's pages with captured responses, in
# each locale and theme, as AVIF. Each docs/*shots.yaml is one image set. Run inside `nix develop`.
#   scripts/docs-shots.sh [manifest] [out]
# <out> defaults to the manifest's `out`, else target/docs-shots. Images in <out> that this run
# does not take stay, unless the manifest sets `clean`.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
manifest="$(realpath "${1:-$root/docs/shots.yaml}")"
port="${ZEABUR_SHOTS_PORT:-18300}"
# The demo server and the browser both take this as the time now, so dates are the same on every run.
export ZEABUR_DEMO_NOW="${ZEABUR_DEMO_NOW:-2026-10-26T09:00:00Z}"
work="$(mktemp -d)"
server=""
cleanup() {
  if [[ -n "$server" ]]; then kill "$server" 2>/dev/null || true; fi
  rm -rf "$work"
}
trap cleanup EXIT

yq -o=json '.' "$manifest" >"$work/shots.json"
settings="$(cd "$root/web" && bun e2e/shots/manifest.ts "$work/shots.json" "$manifest")"
kept="$(jq -r '.out // ""' <<<"$settings")"
file="$(jq -r .file <<<"$settings")"
budget=$(($(jq -r .budget <<<"$settings") * 1024))
out="${2:-${kept:+$root/$kept}}"
out="${out:-$root/target/docs-shots}"
mkdir -p "$out"
out="$(realpath "$out")"
servers="$(jq -r '.servers[]' <<<"$settings")"

mkdir -p "$work/png" "$work/avif"
if grep -q '"app":"zeabur"' <<<"$servers"; then
  "$root/scripts/build-ui.sh"
  ZEABUR_UI_DIST="$root/web/dist" cargo build --locked -p zeabur-api --example demo
  demo="${CARGO_TARGET_DIR:-$root/target}/debug/examples/demo"
fi
if grep -q '"app":"installer"' <<<"$servers"; then
  "$root/scripts/build-installer-ui.sh"
fi

# One demo server for each distinct app and environment. The installer's is Vite's preview
# server with the captured responses of crates/zeabur-installer/web/scripts/dev-api.ts.
while IFS= read -r line; do
  [[ -n "$line" ]] || continue
  app="$(jq -r .app <<<"$line")"
  mapfile -t vars < <(jq -r '.env | to_entries[] | "\(.key)=\(.value)"' <<<"$line")
  if [[ "$app" == installer ]]; then
    (
      cd "$root/crates/zeabur-installer/web"
      exec env "${vars[@]}" bun node_modules/vite/bin/vite.js preview \
        --host 127.0.0.1 --port "$port" --strictPort
    ) >"$work/demo.log" 2>&1 &
  else
    env "${vars[@]}" ZEABUR_DEMO_ADDR="127.0.0.1:$port" "$demo" >"$work/demo.log" 2>&1 &
  fi
  server=$!
  for _ in $(seq 1 100); do
    if (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; then break; fi
    sleep 0.1
  done
  if ! kill -0 "$server" 2>/dev/null; then
    echo "error: the demo server did not start ($line):" >&2
    cat "$work/demo.log" >&2
    exit 1
  fi
  (
    cd "$root/web"
    ZEABUR_SHOTS_URL="http://127.0.0.1:$port" ZEABUR_SHOTS_MANIFEST="$work/shots.json" \
      ZEABUR_SHOTS_SERVER="$line" ZEABUR_SHOTS_OUT="$work/png" \
      ZEABUR_SHOTS_SCALE="$(jq -r .scale <<<"$settings")" \
      bunx playwright test -c playwright.shots.config.ts --output="$root/target/docs-shots-results"
  )
  kill "$server"
  wait "$server" 2>/dev/null || true
  server=""
done <<<"$servers"

# Encode into $work/avif first, so a failed run leaves <out> as it was.
over=()
while IFS= read -r png; do
  IFS=/ read -r locale theme id <<<"${png#"$work/png/"}"
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

# `clean` empties each folder the run writes into, so images whose shot is gone go too.
if [[ "$(jq -r .clean <<<"$settings")" == true ]]; then
  (cd "$work/avif" && find . -name '*.avif' -exec dirname {} \; | sort -u) | while IFS= read -r dir; do
    rm -rf "${out:?}/$dir"
  done
fi
cp -R "$work/avif/." "$out/"
echo "$(find "$work/avif" -name '*.avif' | wc -l | tr -d ' ') images in $out"
