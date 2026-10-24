#!/usr/bin/env bash
# Takes the documentation screenshots that the manifest declares (docs/internal/screenshots.md),
# from the demo server (crates/zeabur-api/examples/demo.rs) or the installer's pages with captured
# responses, in each locale and theme, as AVIF.
# Run inside `nix develop`.
#   scripts/docs-shots.sh [manifest] [out] [version]
# <out> receives <locale>/<theme>/<id>.avif and manifest.json; with a version, also
# docs-shots-<version>.tar, which holds both.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
manifest="$(realpath "${1:-$root/docs/shots.yaml}")"
mkdir -p "${2:-$root/target/docs-shots}"
out="$(realpath "${2:-$root/target/docs-shots}")"
version="${3:-}"
# ZEABUR_SHOTS_BUDGET_KB sets the largest AVIF; the README images use 30.
budget=$((${ZEABUR_SHOTS_BUDGET_KB:-150} * 1024))
port=18300
if [[ -n "$version" && ! "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "expected a version like v0.0.2" >&2
  exit 2
fi
work="$(mktemp -d)"
server=""
cleanup() {
  if [[ -n "$server" ]]; then kill "$server" 2>/dev/null || true; fi
  rm -rf "$work"
}
trap cleanup EXIT

yq -o=json '.' "$manifest" >"$work/shots.json"
servers="$(cd "$root/web" && bun e2e/shots/manifest.ts "$work/shots.json" "$manifest")"

mkdir -p "$work/png"
touch "$work/sizes.tsv"
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
      bunx playwright test -c playwright.shots.config.ts --output="$root/target/docs-shots-results"
  )
  kill "$server"
  wait "$server" 2>/dev/null || true
  server=""
done <<<"$servers"

rm -rf "$out"/{en,zh-TW,zh-CN} "$out/manifest.json" "$out"/docs-shots-*.tar
mkdir -p "$out"/{en,zh-TW,zh-CN}
over=()
while IFS= read -r png; do
  name="${png#"$work/png/"}"
  avif="$out/${name%.png}.avif"
  mkdir -p "$(dirname "$avif")"
  # Lower the quality in steps until the file fits the budget, down to 40.
  for quality in 70 65 60 55 50 45 40; do
    avifenc --jobs all --speed 4 --qcolor "$quality" --yuv 444 "$png" "$avif" >/dev/null
    size=$(wc -c <"$avif" | tr -d ' ')
    if ((size <= budget)); then break; fi
  done
  printf '%s\t%s\n' "${name%.png}.avif" "$size" >>"$work/sizes.tsv"
  if ((size > budget)); then over+=("${name%.png}.avif ($size bytes)"); fi
done < <(find "$work/png" -name '*.png' | sort)

if ((${#over[@]} > 0)); then
  echo "error: over the budget of $budget bytes:" >&2
  printf '  %s\n' "${over[@]}" >&2
  exit 1
fi

jq -R -s 'split("\n") | map(select(. != "") | split("\t") | {path: .[0], bytes: (.[1] | tonumber)})
  | {files: .}' "$work/sizes.tsv" >"$out/manifest.json"
jq -r '.files | map(.bytes) | sort | "\(length) files: min \(.[0]) bytes, median \(.[length / 2 | floor]) bytes, max \(.[-1]) bytes"' \
  "$out/manifest.json"

if [[ -n "$version" ]]; then
  (
    cd "$out"
    tar --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner \
      -cf "docs-shots-$version.tar" manifest.json en zh-TW zh-CN
  )
  echo "$out/docs-shots-$version.tar"
fi
