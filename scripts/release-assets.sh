#!/usr/bin/env bash
# Lists every asset of a release and the workflow artifact it comes from.
#
#   scripts/release-assets.sh <vX.Y.Z>
#
# Prints one "<file> <artifact>" line for each asset.
set -euo pipefail

version=${1:?usage: release-assets.sh <vX.Y.Z>}
root=$(cd "$(dirname "$0")/.." && pwd)

echo "digests.txt published"
echo "zeabur-${version#v}.tgz published"

for platform in linux-amd64 linux-arm64 darwin-arm64; do
  echo "zeaburctl-$version-$platform.tar.gz candidate-$platform"
done

for arch in amd64 arm64; do
  echo "zeabur-$version-linux-$arch.tar.gz candidate-linux-$arch"
  echo "zeabur-installer-$version-linux-$arch.tar.gz finish-linux-$arch"
done

bun "$root/scripts/desktop-release.ts" assets "$version"
