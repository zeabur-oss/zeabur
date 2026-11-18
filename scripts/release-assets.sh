#!/usr/bin/env bash
# Every asset of release <version> and the workflow artifact it comes from (release.yml), one
# `<file> <artifact>` line each: candidate-<os>-<arch>/ from build, published/ from publish,
# finish-linux-<arch>/ from finish, and Zeabur Desktop's (scripts/desktop-release.ts). Run inside
# `nix develop`, after `bun install`.
#   scripts/release-assets.sh v0.2.0
set -euo pipefail

version=${1:?usage: release-assets.sh <vVERSION>}
root=$(cd "$(dirname "$0")/.." && pwd)
echo "digests.txt published"
echo "zeabur-${version#v}.tgz published"
for build in linux-amd64 linux-arm64 darwin-arm64; do
  echo "zeaburctl-$version-$build.tar.gz candidate-$build"
done
for arch in amd64 arm64; do
  echo "zeabur-$version-linux-$arch.tar.gz candidate-linux-$arch"
  echo "zeabur-installer-$version-linux-$arch.tar.gz finish-linux-$arch"
done
bun "$root/scripts/desktop-release.ts" assets "$version"
