#!/usr/bin/env bash
# Builds Zeabur OSS's release files for this machine's architecture: the zeabur archive and the
# images of Zeabur OSS and Zeabur Proxy. It publishes nothing. zeaburctl builds on its own:
# scripts/release-build-zeaburctl.sh.
#
#   scripts/release-build-zeabur.sh <vX.Y.Z> <amd64|arm64> <dist>
#
# ZEABUR_UI_DIST names a built web/dist; without it, the script builds one. Runs on Linux inside
# `nix develop`. docs/internal/releasing.md, "Build files", lists what it writes.
set -euo pipefail

if [[ $# != 3 ]]; then
  echo "usage: release-build-zeabur.sh <vVERSION> <amd64|arm64> <output-dir>" >&2
  exit 2
fi
version="$1"
arch="$2"
root="$(cd "$(dirname "$0")/.." && pwd)"
source "$root/scripts/release-pack.sh"
: "${IN_NIX_SHELL:?run inside nix develop}"
[[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] ||
  { echo "$version is not vX.Y.Z or vX.Y.Z-<pre-release>" >&2; exit 2; }
# mkShell sets SOURCE_DATE_EPOCH to 1980; the commit's time replaces it.
SOURCE_DATE_EPOCH="$(git -C "$root" show -s --format=%ct HEAD)"
export SOURCE_DATE_EPOCH
case "$(uname -s):$arch:$(uname -m)" in
  Linux:amd64:x86_64) target=x86_64-unknown-linux-musl ;;
  Linux:arm64:aarch64) target=aarch64-unknown-linux-musl ;;
  *) echo "cannot build $arch here: zeabur runs on Linux, and each build is native" >&2; exit 2 ;;
esac

mkdir -p "$3"
output="$(cd "$3" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
target_dir="${CARGO_TARGET_DIR:-$root/target}"

# Zeabur's web interface is embedded in zeabur.
if [[ -z "${ZEABUR_UI_DIST:-}" ]]; then
  "$root/scripts/build-ui.sh"
  export ZEABUR_UI_DIST="$root/web/dist"
fi
# Release binaries are always compiled, never taken from a compiler cache (CONTRIBUTING.md).
unset RUSTC_WRAPPER
# cargo-auditable records the dependency tree in the binary, so a scanner reads exactly what was
# compiled in (https://github.com/rust-secure-code/cargo-auditable).
cargo auditable build --release --locked --target "$target" \
  --target-dir "$target_dir" --manifest-path "$root/Cargo.toml" -p zeabur -p zeabur-proxy

# A copy: what is checked below is what the archive holds.
mkdir -p "$work/zeabur"
file="$work/zeabur/zeabur"
install -m 0755 "$target_dir/$target/release/zeabur" "$file"
# Static: no program interpreter, no shared libraries.
elf="$(readelf --program-headers --dynamic "$file")"
if [[ "$elf" == *INTERP* || "$elf" == *'(NEEDED)'* ]]; then
  echo "zeabur is not a static executable" >&2
  exit 1
fi
install -m 0644 "$root/LICENSE" "$work/zeabur/LICENSE"
pack "$work/zeabur" "$output/zeabur-$version-linux-$arch.tar.gz" LICENSE zeabur

# The same context as scripts/build-image.sh: explicit modes, which COPY keeps.
context="$work/image"
install -D -m 0555 "$target_dir/$target/release/zeabur" "$context/zeabur"
install -d -m 0755 "$context/etc" "$context/etc/ssl" "$context/etc/ssl/certs"
install -m 0444 "${ZEABUR_CA_BUNDLE:?run inside nix develop}" "$context/etc/ssl/certs/ca-certificates.crt"
touch -d "@$SOURCE_DATE_EPOCH" "$context/zeabur" "$context/etc" "$context/etc/ssl" \
  "$context/etc/ssl/certs" "$context/etc/ssl/certs/ca-certificates.crt"
image="zeabur:release-$arch"
docker build --platform "linux/$arch" --network none --build-arg "SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH" \
  --file "$root/Containerfile" --tag "$image" "$context"
docker image save --output "$output/image.tar" "$image"
echo "built $image (not published)"

context="$work/proxy-image"
install -D -m 0555 "$target_dir/$target/release/zeabur-proxy" "$context/zeabur-proxy"
touch -d "@$SOURCE_DATE_EPOCH" "$context/zeabur-proxy"
image="zeabur-proxy:release-$arch"
docker build --platform "linux/$arch" --network none --build-arg "SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH" \
  --file "$root/Containerfile.proxy" --tag "$image" "$context"
docker image save --output "$output/proxy-image.tar" "$image"
echo "built $image (not published)"
ls -l "$output"
