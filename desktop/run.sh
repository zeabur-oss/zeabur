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
# identifier in tauri.conf.json), and the icon by name from the hicolor theme. The entry starts
# only the debug binary, which needs the servers below, so it is removed again on exit.
id=com.zeabur.desktop-poc
data=${XDG_DATA_HOME:-$HOME/.local/share}
installed=()
if [[ "$(uname -s)" == Linux ]]; then
  for icon in 32x32:32x32.png 128x128:128x128.png 256x256:128x128@2x.png 512x512:icon.png; do
    dir=$data/icons/hicolor/${icon%%:*}/apps
    mkdir -p "$dir"
    cp "$root/desktop/icons/${icon#*:}" "$dir/$id.png"
    installed+=("$dir/$id.png")
  done
  touch "$data/icons/hicolor"
  mkdir -p "$data/applications"
  cat > "$data/applications/$id.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Zeabur
Comment=Open Zeabur for the clusters in your kubeconfig
Exec="$root/desktop/target/debug/zeabur-desktop"
Icon=$id
Terminal=false
Categories=Development;
StartupWMClass=$id
EOF
  installed+=("$data/applications/$id.desktop")
fi

pids=()
trap 'kill "${pids[@]}" 2>/dev/null || true; rm -f "${installed[@]}"' EXIT
(cd "$root" && cargo run -q -p zeabur-api --example demo) &
pids+=($!)
(cd "$root/web" && bun install --frozen-lockfile >/dev/null && bun run dev --strictPort) &
pids+=($!)
until curl -sf -o /dev/null http://localhost:5173/desktop.html; do sleep 1; done
cd "$root/desktop" && cargo run
