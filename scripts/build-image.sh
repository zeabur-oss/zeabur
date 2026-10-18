#!/usr/bin/env bash
# Builds engawa's container image for this machine's architecture: a static musl binary in an
# otherwise empty image (Containerfile). Run on Linux inside `nix develop`; needs docker.
#   scripts/build-image.sh engawa:dev
set -euo pipefail

tag="${1:?usage: build-image.sh <tag>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
target="$(uname -m | sed -e 's/^arm64$/aarch64/')-unknown-linux-musl"

"$root/scripts/build-ui.sh"
ENGAWA_UI_DIST="$root/web/dist" cargo build --release --locked --target "$target" -p engawa --manifest-path "$root/Cargo.toml"
context="$root/target/image"
rm -rf "$context"
mkdir -p "$context"
# Staged with explicit modes, which COPY keeps (no `--chmod`, so the classic builder works too):
# the binary 0555, directories 0755, the CA bundle 0444.
install -m 0555 "$root/target/$target/release/engawa" "$context/engawa"
install -d -m 0755 "$context/etc" "$context/etc/ssl" "$context/etc/ssl/certs"
install -m 0444 "${ENGAWA_CA_BUNDLE:?run inside nix develop}" "$context/etc/ssl/certs/ca-certificates.crt"
docker build --file "$root/Containerfile" --tag "$tag" "$context"
size=$(docker image inspect --format '{{.Size}}' "$tag")
echo "image $tag: $size bytes"
