#!/usr/bin/env bash
# The desktop app (desktop/, a Cargo workspace of its own) with the main workspace's rules: the
# rules check, format, lint, tests and cargo-deny for each platform it builds for. Runs in
# `nix develop .#desktop`, which has the GTK and WebKitGTK libraries on Linux. `--all-features`
# embeds web/dist, so the web interface is built first.
set -euo pipefail
cd "$(dirname "$0")/../.."
# Under target/, which CI caches.
export CARGO_TARGET_DIR="$PWD/target/desktop"

./scripts/check-desktop.sh
./scripts/build-ui.sh
(
  cd desktop
  cargo fmt --all --check
  cargo lint
  cargo nextest run --workspace --all-features --locked --no-tests=warn
)
# deny.toml's targets are the container image's; these are the desktop app's.
for target in aarch64-apple-darwin x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu \
  x86_64-pc-windows-msvc aarch64-pc-windows-msvc; do
  cargo deny --manifest-path desktop/Cargo.toml --config deny.toml --all-features --locked \
    --target "$target" check
done
