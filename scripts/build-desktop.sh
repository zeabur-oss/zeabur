#!/usr/bin/env bash
# Builds Zeabur Desktop for this machine with the Tauri bundler: the web interface the local
# service embeds (web/dist), the app's own pages (desktop/web/dist), the binary, then the bundles
# under target/desktop/release/bundle/. Run inside `nix develop .#desktop`.
#   scripts/build-desktop.sh            # macOS: app,dmg
#   scripts/build-desktop.sh deb        # the bundles to make, as `cargo tauri bundle --bundles` takes
# Signing follows the Tauri bundler's variables (APPLE_SIGNING_IDENTITY, and APPLE_API_* or
# APPLE_ID with APPLE_PASSWORD and APPLE_TEAM_ID for notarization); without them the app is
# signed ad hoc.
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
binary="$CARGO_TARGET_DIR/release/zeabur-desktop"
if [[ "$(uname -s)" == Darwin ]]; then
  # Rust's standard library links libiconv, which the Nix shell resolves to its own build of
  # Apple's libiconv; the binary is pointed at the system's copy of that same library, as in
  # scripts/release-build.sh, before the bundler copies and signs it.
  while IFS= read -r library; do
    install_name_tool -change "$library" /usr/lib/libiconv.2.dylib "$binary"
  done < <(otool -L "$binary" | sed -n 's|^\t\(/nix/store/[^ ]*-libiconv-[^ ]*/lib/libiconv\.2\.dylib\) .*|\1|p')
fi
# Built in the Nix shell, the binary must still run where there is no Nix.
"$root/scripts/check-portable.sh" "$binary"
cargo tauri bundle --bundles "$bundles"
if [[ "$(uname -s)" == Darwin ]]; then
  "$root/scripts/check-portable.sh" "$CARGO_TARGET_DIR/release/bundle/macos/Zeabur.app"
fi
