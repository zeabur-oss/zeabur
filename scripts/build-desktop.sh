#!/usr/bin/env bash
# Builds Zeabur Desktop on this machine for local use: the web interface the local service embeds
# (web/dist), the app's own pages (desktop/web/dist), the binary, then the bundles under
# target/desktop/release/bundle/. Run inside `nix develop .#desktop`. The bundle is not meant to
# run on another Mac; releases are built on runners by the desktop-* jobs of
# .github/workflows/release.yml.
#   scripts/build-desktop.sh            # macOS: app,dmg
#   scripts/build-desktop.sh deb        # the bundles to make, as `cargo tauri bundle --bundles` takes
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
export CARGO_TARGET_DIR="$root/target/desktop"
bundles="${1:-}"
if [[ -z "$bundles" ]]; then
  [[ "$(uname -s)" == Darwin ]] || { echo "name the bundles to make, such as deb" >&2; exit 2; }
  bundles=app,dmg
fi

"$root/scripts/build-ui.sh"
(cd "$root/desktop/web" && bun ci && rm -rf dist && bun run build)

cd "$root/desktop"
ZEABUR_UI_DIST="$root/web/dist" cargo tauri build --no-bundle
cargo tauri bundle --bundles "$bundles"
