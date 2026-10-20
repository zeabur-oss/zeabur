#!/usr/bin/env bash
# Builds the web interface into web/dist: Vite's output, brotli encodings, and
# manifest.tsv (web/scripts/postbuild.ts). The output is not committed; cargo embeds it when
# ZEABUR_UI_DIST points at it (crates/zeabur-ui/build.rs). Run inside `nix develop`.
#   scripts/build-ui.sh && ZEABUR_UI_DIST="$PWD/web/dist" cargo build --release -p zeabur
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root/web"
# Installs exactly bun.lock; fails if package.json and the lock file disagree.
bun ci
rm -rf dist
bun run build
