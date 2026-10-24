#!/usr/bin/env bash
# Builds the web interface into web/dist, with its brotli encodings and manifest.tsv, for a
# release build to embed. Run inside `nix develop`:
#   scripts/build-ui.sh
#   ZEABUR_INSTALLER_UI_DIST="$PWD/web/dist" cargo build --release --locked -p zeabur-installer
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root/web"
bun install
bun run build
