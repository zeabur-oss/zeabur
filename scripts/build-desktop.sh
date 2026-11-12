#!/usr/bin/env bash
# Builds Zeabur Desktop for this machine with the Tauri bundler: the web interface the local
# service embeds (web/dist), the app's own pages (desktop/web/dist), then the app and, on macOS, the
# .dmg under target/desktop/release/bundle/. Run inside `nix develop .#desktop`.
#   scripts/build-desktop.sh                 # macOS: --bundles app,dmg
#   scripts/build-desktop.sh --bundles deb   # other arguments go to `cargo tauri build`
# Signing follows the Tauri bundler's variables (APPLE_SIGNING_IDENTITY, and APPLE_API_* or
# APPLE_ID with APPLE_PASSWORD and APPLE_TEAM_ID for notarization); without them the app is
# signed ad hoc.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
export CARGO_TARGET_DIR="$root/target/desktop"

"$root/scripts/build-ui.sh"
(cd "$root/desktop/web" && bun ci && rm -rf dist && bun run build)

args=("$@")
if [[ ${#args[@]} -eq 0 && "$(uname -s)" == Darwin ]]; then
  args=(--bundles app,dmg)
fi
cd "$root/desktop"
ZEABUR_UI_DIST="$root/web/dist" cargo tauri build "${args[@]}"
