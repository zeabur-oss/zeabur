#!/usr/bin/env bash
# Builds Zeabur's container image, or Zeabur Proxy's, for this machine's architecture: a static
# musl binary in an otherwise empty image (Containerfile, Containerfile.proxy). Run on Linux
# inside `nix develop`; needs docker.
#   scripts/build-image.sh zeabur:dev
#   scripts/build-image.sh zeabur-proxy:dev zeabur-proxy
set -euo pipefail

tag="${1:?usage: build-image.sh <tag> [zeabur|zeabur-proxy]}"
binary="${2:-zeabur}"
root="$(cd "$(dirname "$0")/.." && pwd)"
target="$(uname -m | sed -e 's/^arm64$/aarch64/')-unknown-linux-musl"
context="$root/target/image"
rm -rf "$context"
mkdir -p "$context"

case "$binary" in
  zeabur)
    "$root/scripts/build-ui.sh"
    ZEABUR_UI_DIST="$root/web/dist" cargo build --release --locked --target "$target" -p zeabur --manifest-path "$root/Cargo.toml"
    # Staged with explicit modes, which COPY keeps (no `--chmod`, so the classic builder works
    # too): the binary 0555, directories 0755, the CA bundle 0444.
    install -m 0555 "$root/target/$target/release/zeabur" "$context/zeabur"
    install -d -m 0755 "$context/etc" "$context/etc/ssl" "$context/etc/ssl/certs"
    install -m 0444 "${ZEABUR_CA_BUNDLE:?run inside nix develop}" "$context/etc/ssl/certs/ca-certificates.crt"
    containerfile="$root/Containerfile"
    ;;
  zeabur-proxy)
    cargo build --release --locked --target "$target" -p zeabur-proxy --manifest-path "$root/Cargo.toml"
    install -m 0555 "$root/target/$target/release/zeabur-proxy" "$context/zeabur-proxy"
    containerfile="$root/Containerfile.proxy"
    ;;
  *) echo "error: no image for $binary" >&2; exit 2 ;;
esac
docker build --file "$containerfile" --tag "$tag" "$context"
size=$(docker image inspect --format '{{.Size}}' "$tag")
echo "image $tag: $size bytes"
