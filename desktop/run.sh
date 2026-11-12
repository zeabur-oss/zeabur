#!/usr/bin/env bash
# Starts the demo API, the web dev server and the desktop app. Run from anywhere:
#   nix develop -c desktop/run.sh             # your own kubeconfig
#   nix develop -c desktop/run.sh --fixtures  # the sample kubeconfigs in desktop/fixtures
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)

if [[ "${1:-}" == "--fixtures" ]]; then
  state=$(mktemp -d)
  export ZEABUR_DESKTOP_STATE=$state
  export KUBECONFIG=$root/desktop/fixtures/home/.kube/config
  printf '{"added":[{"kind":"folder","path":"%s"}]}\n' "$root/desktop/fixtures/team" > "$state/state.json"
fi

pids=()
trap 'kill "${pids[@]}" 2>/dev/null || true' EXIT
(cd "$root" && cargo run -q -p zeabur-api --example demo) &
pids+=($!)
(cd "$root/web" && bun install --frozen-lockfile >/dev/null && bun run dev --strictPort) &
pids+=($!)
until curl -sf -o /dev/null http://localhost:5173/desktop.html; do sleep 1; done
cd "$root/desktop" && cargo run
