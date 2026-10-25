#!/usr/bin/env bash
# Builds the installer's web interface into crates/zeabur-installer/web/dist, with its brotli
# encodings and manifest.tsv, for a release build to embed. Run inside `nix develop`:
#   scripts/build-installer-ui.sh
#   ZEABUR_INSTALLER_UI_DIST="$PWD/crates/zeabur-installer/web/dist" cargo build --release --locked -p zeabur-installer
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root/crates/zeabur-installer/web"
# Installs exactly bun.lock; fails if package.json and the lock file disagree.
bun ci
bun run build
