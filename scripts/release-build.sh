#!/usr/bin/env bash
# Builds one platform's release candidates; publishes nothing. .github/workflows/release.yml runs
# it on a runner of each platform, then smokes what it built (release-smoke.sh) before anything
# is published (release-publish.sh). Run inside `nix develop`, from a git checkout: archive
# entries carry the commit's time (SOURCE_DATE_EPOCH), not the build's.
#   scripts/release-build.sh v0.0.2 arm64 dist
#
# On Linux (native architecture only; Docker for the image):
#   engawactl-<version>-linux-<arch>.tar.gz, engawa-<version>-linux-<arch>.tar.gz: one static musl
#     binary each, with LICENSE;
#   image.tar: the image of Containerfile around that same `engawa` binary (`docker image save`).
# On macOS, arm64 only (Apple silicon):
#   engawactl-<version>-darwin-arm64.tar.gz. The server only runs in a pod
#   (`ClusterEndpoint::in_cluster`), so there is no macOS `engawa`.
set -euo pipefail

if [[ $# != 3 ]]; then
  echo "usage: release-build.sh <vVERSION> <amd64|arm64> <output-dir>" >&2
  exit 2
fi
version="$1"
arch="$2"
root="$(cd "$(dirname "$0")/.." && pwd)"
: "${IN_NIX_SHELL:?run inside nix develop}"
if [[ ! "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "expected a version like v0.0.2" >&2
  exit 2
fi
# mkShell sets SOURCE_DATE_EPOCH to 1980; the commit's time replaces it.
SOURCE_DATE_EPOCH="$(git -C "$root" show -s --format=%ct HEAD)"
export SOURCE_DATE_EPOCH
case "$(uname -s):$arch:$(uname -m)" in
  Linux:amd64:x86_64) os=linux target=x86_64-unknown-linux-musl ;;
  Linux:arm64:aarch64) os=linux target=aarch64-unknown-linux-musl ;;
  Darwin:arm64:arm64) os=darwin target=aarch64-apple-darwin ;;
  *) echo "cannot build $arch here: each build is native, and macOS is arm64 only" >&2; exit 2 ;;
esac
binaries=(engawactl)
[[ "$os" == linux ]] && binaries+=(engawa)

mkdir -p "$3"
output="$(cd "$3" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
target_dir="${CARGO_TARGET_DIR:-$root/target}"

# The web interface is embedded in both binaries.
"$root/scripts/build-ui.sh"
# cargo-auditable records the dependency tree in the binary, so a scanner reads exactly what was
# compiled in (https://github.com/rust-secure-code/cargo-auditable).
packages=()
for binary in "${binaries[@]}"; do packages+=(-p "$binary"); done
ENGAWA_UI_DIST="$root/web/dist" cargo auditable build --release --locked --target "$target" \
  --target-dir "$target_dir" --manifest-path "$root/Cargo.toml" "${packages[@]}"

for binary in "${binaries[@]}"; do
  # A copy: what is checked and changed below is what the archive holds.
  mkdir -p "$work/$binary"
  file="$work/$binary/$binary"
  install -m 0755 "$target_dir/$target/release/$binary" "$file"
  if [[ "$os" == linux ]]; then
    # Static: no program interpreter, no shared libraries.
    elf="$(readelf --program-headers --dynamic "$file")"
    if [[ "$elf" == *INTERP* || "$elf" == *'(NEEDED)'* ]]; then
      echo "$binary is not a static executable" >&2
      exit 1
    fi
  else
    # Only libraries every Mac has: one from the Nix store would be missing on the user's machine.
    # Rust's standard library links libiconv, which the Nix shell resolves to its own build of
    # Apple's libiconv; the binary is pointed at the system's copy of that same library. The first
    # line of `otool -L` names the file itself.
    libraries="$(otool -L "$file" | tail -n +2 | awk '{ print $1 }')"
    while IFS= read -r library; do
      case "$library" in
        /usr/lib/* | /System/Library/*) ;;
        /nix/store/*-libiconv-*/lib/libiconv.2.dylib)
          install_name_tool -change "$library" /usr/lib/libiconv.2.dylib "$file" ;;
        *) echo "$binary links $library, which is not part of macOS" >&2; exit 1 ;;
      esac
    done <<<"$libraries"
    # Apple silicon runs only signed code: the linker signs ad hoc (LC_CODE_SIGNATURE), and
    # install_name_tool signs again after its change. Running it proves the signature valid.
    load_commands="$(otool -l "$file")"
    [[ "$load_commands" == *LC_CODE_SIGNATURE* ]] || { echo "$binary has no code signature" >&2; exit 1; }
    "$file" --version
  fi
  install -m 0644 "$root/LICENSE" "$work/$binary/LICENSE"
  # Reproducible: sorted entries, the commit's time, no owner names, no gzip timestamp.
  tar --format=ustar --sort=name --mtime="@$SOURCE_DATE_EPOCH" \
    --owner=0 --group=0 --numeric-owner -C "$work/$binary" -cf - LICENSE "$binary" |
    gzip -n -9 >"$output/$binary-$version-$os-$arch.tar.gz"
done

if [[ "$os" == linux ]]; then
  # The same context as scripts/build-image.sh: explicit modes, which COPY keeps.
  context="$work/image"
  install -D -m 0555 "$target_dir/$target/release/engawa" "$context/engawa"
  install -d -m 0755 "$context/etc" "$context/etc/ssl" "$context/etc/ssl/certs"
  install -m 0444 "${ENGAWA_CA_BUNDLE:?run inside nix develop}" "$context/etc/ssl/certs/ca-certificates.crt"
  touch -d "@$SOURCE_DATE_EPOCH" "$context/engawa" "$context/etc" "$context/etc/ssl" \
    "$context/etc/ssl/certs" "$context/etc/ssl/certs/ca-certificates.crt"
  image="engawa:release-$arch"
  docker build --platform "linux/$arch" --network none --build-arg "SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH" \
    --file "$root/Containerfile" --tag "$image" "$context"
  docker image save --output "$output/image.tar" "$image"
  echo "built $image (not published)"
fi
ls -l "$output"
