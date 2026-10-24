#!/usr/bin/env bash
# Pins another RKE2 release: rewrites the version and the SHA-256 digests in
# crates/zeabur-installer-plan/src/pins.rs from the release's own checksum lists.
#
#   scripts/bump-rke2.sh v1.36.6+rke2r1
#
# Upstream signs nothing, so read the diff: the digests come from GitHub over HTTPS, the same
# source the installer would otherwise trust at install time. Pinning them here moves that
# trust to review time, once, instead of every install.
set -euo pipefail

version=${1:?usage: scripts/bump-rke2.sh <version, such as v1.36.6+rke2r1>}
[[ $version =~ ^v([0-9]+)\.([0-9]+)\.[0-9]+\+rke2r[0-9]+$ ]] || {
  echo "error: $version is not an RKE2 release tag" >&2
  exit 1
}
minor="${BASH_REMATCH[1]}.${BASH_REMATCH[2]}"
pins="$(cd "$(dirname "$0")/.." && pwd)/crates/zeabur-installer-plan/src/pins.rs"
base="https://github.com/rancher/rke2/releases/download/${version//+/%2B}"

digest() { # $1 = list, $2 = file
  awk -v f="$2" '$2 == f || $2 == "*" f { print $1 }' <<<"$1"
}

rke2='/^pub const RKE2: Rke2 = Rke2 {/,/^};/'
args=(-e "$rke2 s/version: \"v[^\"]*\"/version: \"$version\"/" -e "$rke2 s/kubernetes: \"[0-9.]*\"/kubernetes: \"$minor\"/")
for arch in amd64 arm64; do
  list=$(curl -fsSL "$base/sha256sum-$arch.txt")
  for pair in "tarball:rke2.linux-$arch.tar.gz" "images_core:rke2-images-core.linux-$arch.tar.zst" \
    "images_canal:rke2-images-canal.linux-$arch.tar.zst"; do
    field=${pair%%:*}
    sha=$(digest "$list" "${pair#*:}")
    [[ $sha =~ ^[0-9a-f]{64}$ ]] || {
      echo "error: no digest for ${pair#*:} in sha256sum-$arch.txt" >&2
      exit 1
    }
    # Each field appears once in the architecture's block of the `RKE2` constant.
    args+=(-e "/^    $arch: Rke2Files {/,/^    },/ s/$field: \"[0-9a-f]*\"/$field: \"$sha\"/")
  done
done
sed -i.bak "${args[@]}" "$pins" && rm "$pins.bak"
git --no-pager diff --stat -- "$pins"
