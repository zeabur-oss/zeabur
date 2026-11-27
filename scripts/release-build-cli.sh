#!/usr/bin/env bash
# Builds zeaburctl for the platforms the Nix shell cannot build: Windows amd64 on Windows, and Intel
# Macs on a Mac with Apple silicon. It publishes nothing.
#
#   scripts/release-build-cli.sh compile <vX.Y.Z> <windows|darwin> amd64 <dist>
#   scripts/release-build-cli.sh package <vX.Y.Z> <windows|darwin> amd64 <dist>
#
# compile builds the binary; package packs it into zeaburctl-<vX.Y.Z>-<os>-amd64.zip (Windows) or
# .tar.gz (macOS) in <dist>, signed when the signing variables are set. They are two steps so no
# build script of a dependency runs with the signing variables set.
#
# Runs outside the Nix shell, with the system's compiler and linker, like scripts/package-desktop.sh,
# and needs the same: on PATH cargo and rustc of the nightly flake.lock pins, with the target, and
# cargo-auditable; in the environment ZEABUR_CATALOG_RELEASE, ZEABUR_CATALOG_COMMIT and
# ZEABUR_UI_DIST. The desktop-prepare job of .github/workflows/release.yml prepares them.
#
# Signing:
#   macOS    APPLE_CERTIFICATE_P12 and the rest of scripts/release-sign.sh.
#   Windows  AZURE_SIGNING_ENDPOINT and the rest of scripts/azure-signtool.sh, after `az login`.
set -euo pipefail

usage="usage: release-build-cli.sh <compile|package> <vX.Y.Z> <windows|darwin> amd64 <dist>"
if [[ $# -ne 5 || ( "$1" != compile && "$1" != package ) ]]; then
  echo "$usage" >&2
  exit 2
fi
stage=$1 version=$2 os=$3 arch=$4
mkdir -p "$5"
dist=$(cd "$5" && pwd)
root=$(cd "$(dirname "$0")/.." && pwd)
source "$root/scripts/release-pack.sh"

[[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] ||
  { echo "$version is not vX.Y.Z or vX.Y.Z-<pre-release>" >&2; exit 2; }
case "$(uname -s):$(uname -m):$os:$arch" in
  MINGW64_NT*:x86_64:windows:amd64) target=x86_64-pc-windows-msvc exe=zeaburctl.exe archive=zip ;;
  Darwin:arm64:darwin:amd64) target=x86_64-apple-darwin exe=zeaburctl archive=tar.gz ;;
  *) echo "$usage; Windows builds on Windows, the Intel Mac build on Apple silicon" >&2; exit 2 ;;
esac
if [[ -n "${NIX_CC:-}${NIX_LDFLAGS:-}${IN_NIX_SHELL:-}" ]]; then
  echo "run this outside the Nix shell; it links with the system's tools" >&2
  exit 2
fi
export CARGO_TARGET_DIR="$root/target/cli-release"
binary="$CARGO_TARGET_DIR/$target/release/$exe"
SOURCE_DATE_EPOCH=$(git -C "$root" show -s --format=%ct HEAD)
export SOURCE_DATE_EPOCH

if [[ "$stage" == compile ]]; then
  for var in ZEABUR_CATALOG_RELEASE ZEABUR_CATALOG_COMMIT ZEABUR_UI_DIST; do
    [[ -n "${!var:-}" ]] || { echo "$var is not set" >&2; exit 1; }
  done
  # Release binaries are always compiled, never taken from a compiler cache (CONTRIBUTING.md).
  unset RUSTC_WRAPPER
  cargo auditable build --release --locked --target "$target" \
    --manifest-path "$root/Cargo.toml" -p zeaburctl
  if [[ "$os" == darwin ]]; then
    # Only libraries every Mac has; the first line of `otool -L` names the file itself.
    while read -r library _; do
      case "$library" in
        /usr/lib/* | /System/Library/*) ;;
        *) echo "zeaburctl links $library, which is not part of macOS" >&2; exit 1 ;;
      esac
    done < <(otool -L "$binary" | tail -n +2)
  else
    "$binary" --version
  fi
  exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir "$work/zeaburctl"
install -m 0755 "$binary" "$work/zeaburctl/$exe"
install -m 0644 "$root/LICENSE" "$work/zeaburctl/LICENSE"
file="$dist/zeaburctl-$version-$os-$arch.$archive"

if [[ "$os" == windows ]]; then
  source "$root/scripts/azure-signtool.sh"
  sign=()
  azure_signtool "$work"
  if [[ ${#sign[@]} -gt 0 ]]; then
    MSYS2_ARG_CONV_EXCL="*" "${sign[@]}" "$(cygpath -w "$work/zeaburctl/$exe")"
    MSYS2_ARG_CONV_EXCL="*" "${sign[0]}" verify /pa "$(cygpath -w "$work/zeaburctl/$exe")"
  fi
  pack "$work/zeaburctl" "$file" LICENSE "$exe"
else
  pack "$work/zeaburctl" "$file" LICENSE "$exe"
  "$root/scripts/release-sign.sh" "$version" "$dist" "$arch"
fi
ls -l "$dist"
