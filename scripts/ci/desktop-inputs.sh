#!/usr/bin/env bash
# What scripts/package-desktop.sh needs from the dev shell, for a runner that builds outside it
# (.github/workflows/desktop.yml): web/dist, desktop/web/dist and the catalog in
# target/desktop-inputs/catalog. Prints the step outputs: the version (VERSION, or the app's), the
# shell's nightly and tauri-cli, the catalog commit and whether the app has an updater key. Runs in
# `nix develop .#desktop`.
set -euo pipefail
cd "$(dirname "$0")/../.."

app=$(cargo metadata --manifest-path desktop/Cargo.toml --locked --no-deps --format-version 1 |
  jq -r '.packages[] | select(.name == "zeabur-desktop") | .version')
toolchain=$(readlink -f "$(command -v rustc)" | grep -o 'nightly-[0-9]\{4\}-[0-9]\{2\}-[0-9]\{2\}')
tauri_cli=$(cargo tauri --version)

./scripts/build-ui.sh >&2
(cd desktop/web && bun ci && rm -rf dist && bun run build) >&2
rm -rf target/desktop-inputs
mkdir -p target/desktop-inputs
cp -RL "$ZEABUR_CATALOG_RELEASE" target/desktop-inputs/catalog

echo "version=${VERSION:-v$app}"
echo "toolchain=$toolchain"
echo "tauri-cli=${tauri_cli#tauri-cli }"
echo "catalog-commit=$ZEABUR_CATALOG_COMMIT"
echo "updater=$(jq -r '.plugins.updater.pubkey != ""' desktop/tauri.conf.json)"
