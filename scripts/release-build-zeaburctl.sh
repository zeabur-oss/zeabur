#!/usr/bin/env bash
# Builds zeaburctl's release archive for this machine's platform. It publishes nothing.
#
#   scripts/release-build-zeaburctl.sh compile <vX.Y.Z> <os> <arch> <dist>
#   scripts/release-build-zeaburctl.sh package <vX.Y.Z> <os> <arch> <dist>
#
# <os>-<arch> is linux-amd64, linux-arm64, darwin-arm64 or windows-amd64, and each build is
# native. compile builds and checks the binary; package packs it into
# zeaburctl-<vX.Y.Z>-<os>-<arch>.tar.gz (.zip on Windows) in <dist>, signed when the signing
# variables are set. They are two steps so no build script of a dependency runs with the signing
# variables set.
#
# Linux and macOS build inside `nix develop`. Windows builds outside it, like
# scripts/package-desktop.sh, as the Nix shell has no Windows toolchain: it needs on PATH cargo and
# rustc of the nightly flake.lock pins, and cargo-auditable, and in the environment
# ZEABUR_CATALOG_RELEASE, ZEABUR_CATALOG_COMMIT and ZEABUR_UI_DIST. The prepare job of
# .github/workflows/release.yml prepares them. In the Nix shell, ZEABUR_UI_DIST is optional: without
# it, compile builds web/dist.
#
# Signing:
#   macOS    APPLE_CERTIFICATE_P12 and the rest of scripts/release-sign.sh.
#   Windows  AZURE_SIGNING_ENDPOINT and the rest of scripts/azure-signtool.sh, after `az login`.
set -euo pipefail

usage="usage: release-build-zeaburctl.sh <compile|package> <vX.Y.Z> <os> <arch> <dist>"
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
nix=yes exe=zeaburctl archive=tar.gz
case "$(uname -s):$(uname -m):$os:$arch" in
  Linux:x86_64:linux:amd64) target=x86_64-unknown-linux-musl ;;
  Linux:aarch64:linux:arm64) target=aarch64-unknown-linux-musl ;;
  Darwin:arm64:darwin:arm64) target=aarch64-apple-darwin ;;
  MINGW64_NT*:x86_64:windows:amd64) target=x86_64-pc-windows-msvc nix="" exe=zeaburctl.exe archive=zip ;;
  *) echo "$usage; each build is native, on linux-amd64, linux-arm64, darwin-arm64 or windows-amd64" >&2; exit 2 ;;
esac
if [[ -n "$nix" ]]; then
  : "${IN_NIX_SHELL:?run inside nix develop}"
elif [[ -n "${NIX_CC:-}${NIX_LDFLAGS:-}${IN_NIX_SHELL:-}" ]]; then
  echo "build for Windows outside the Nix shell; it links with the system's tools" >&2
  exit 2
fi
target_dir="${CARGO_TARGET_DIR:-$root/target}"
binary="$target_dir/$target/release/$exe"
# mkShell sets SOURCE_DATE_EPOCH to 1980; the commit's time replaces it.
SOURCE_DATE_EPOCH=$(git -C "$root" show -s --format=%ct HEAD)
export SOURCE_DATE_EPOCH

if [[ "$stage" == compile ]]; then
  if [[ -z "$nix" ]]; then
    for var in ZEABUR_CATALOG_RELEASE ZEABUR_CATALOG_COMMIT ZEABUR_UI_DIST; do
      [[ -n "${!var:-}" ]] || { echo "$var is not set" >&2; exit 1; }
    done
  elif [[ -z "${ZEABUR_UI_DIST:-}" ]]; then
    "$root/scripts/build-ui.sh"
    export ZEABUR_UI_DIST="$root/web/dist"
  fi
  # Release binaries are always compiled, never taken from a compiler cache (CONTRIBUTING.md).
  unset RUSTC_WRAPPER
  # cargo-auditable records the dependency tree in the binary, so a scanner reads exactly what was
  # compiled in (https://github.com/rust-secure-code/cargo-auditable).
  cargo auditable build --release --locked --target "$target" \
    --target-dir "$target_dir" --manifest-path "$root/Cargo.toml" -p zeaburctl
  case "$os" in
    linux)
      # Static: no program interpreter, no shared libraries.
      elf="$(readelf --program-headers --dynamic "$binary")"
      if [[ "$elf" == *INTERP* || "$elf" == *'(NEEDED)'* ]]; then
        echo "zeaburctl is not a static executable" >&2
        exit 1
      fi
      ;;
    darwin)
      # Only libraries every Mac has: one from the Nix store would be missing on the user's
      # machine. Rust's standard library links libiconv, which the Nix shell resolves to its own
      # build of Apple's libiconv; the binary is pointed at the system's copy of that same
      # library. The first line of `otool -L` names the file itself.
      libraries="$(otool -L "$binary" | tail -n +2)"
      while read -r library _; do
        case "$library" in
          /usr/lib/* | /System/Library/*) ;;
          /nix/store/*-libiconv-*/lib/libiconv.2.dylib)
            install_name_tool -change "$library" /usr/lib/libiconv.2.dylib "$binary" ;;
          *) echo "zeaburctl links $library, which is not part of macOS" >&2; exit 1 ;;
        esac
      done <<<"$libraries"
      # Apple silicon runs only signed code: the linker signs ad hoc (LC_CODE_SIGNATURE), and
      # install_name_tool signs again after its change. Running it below proves the signature.
      load_commands="$(otool -l "$binary")"
      [[ "$load_commands" == *LC_CODE_SIGNATURE* ]] || { echo "zeaburctl has no code signature" >&2; exit 1; }
      ;;
  esac
  said=$("$binary" --version | tr -d '\r')
  [[ "$said" == "zeaburctl ${version#v}" ]] || { echo "zeaburctl says '$said'" >&2; exit 1; }
  exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir "$work/zeaburctl"
install -m 0755 "$binary" "$work/zeaburctl/$exe"
install -m 0644 "$root/LICENSE" "$work/zeaburctl/LICENSE"
file="$dist/zeaburctl-$version-$os-$arch.$archive"

case "$os" in
  windows)
    source "$root/scripts/azure-signtool.sh"
    sign=()
    azure_signtool "$work"
    if [[ ${#sign[@]} -gt 0 ]]; then
      MSYS2_ARG_CONV_EXCL="*" "${sign[@]}" "$(cygpath -w "$work/zeaburctl/$exe")"
      MSYS2_ARG_CONV_EXCL="*" "${sign[0]}" verify /pa "$(cygpath -w "$work/zeaburctl/$exe")"
    fi
    pack "$work/zeaburctl" "$file" LICENSE "$exe"
    ;;
  darwin)
    pack "$work/zeaburctl" "$file" LICENSE "$exe"
    "$root/scripts/release-sign.sh" "$version" "$dist"
    ;;
  linux)
    pack "$work/zeaburctl" "$file" LICENSE "$exe"
    ;;
esac
ls -l "$dist"
