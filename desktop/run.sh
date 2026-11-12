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

# GNOME takes a window's name and icon from the desktop entry named after its app ID (the
# identifier in tauri.conf.json); this one starts the dev build.
if [[ "$(uname -s)" == Linux ]]; then
  apps=${XDG_DATA_HOME:-$HOME/.local/share}/applications
  mkdir -p "$apps"
  cat > "$apps/com.zeabur.desktop-poc.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Zeabur
Comment=Open Zeabur for the clusters in your kubeconfig
Exec="$root/desktop/target/debug/zeabur-desktop"
Icon=$root/desktop/icons/icon.png
Terminal=false
Categories=Development;
StartupWMClass=com.zeabur.desktop-poc
EOF
fi

pids=()
trap 'kill "${pids[@]}" 2>/dev/null || true' EXIT
(cd "$root" && cargo run -q -p zeabur-api --example demo) &
pids+=($!)
(cd "$root/web" && bun install --frozen-lockfile >/dev/null && bun run dev --strictPort) &
pids+=($!)
until curl -sf -o /dev/null http://localhost:5173/desktop.html; do sleep 1; done
cd "$root/desktop" && cargo run
