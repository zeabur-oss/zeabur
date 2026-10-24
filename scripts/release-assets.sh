#!/usr/bin/env bash
# Makes the release assets from the candidates, compiling nothing: the archives, the chart, the
# documentation screenshots and digests.txt that release-publish.sh wrote, and the checksums of all
# of them, signed keyless (Sigstore). It completes the release notes. The workflow then attests
# the assets and creates the draft release (docs/internal/releasing.md).
# CI only, from .github/workflows/release.yml, inside `nix develop`.
#   scripts/release-assets.sh v0.0.2 candidates release
#
# <candidates> holds one directory per build (release-linux-amd64, release-installer-amd64, …),
# release-docs-shots and release-published (release-publish.sh); <out> receives assets/ and
# notes.md.
set -euo pipefail

version="${1:?usage: release-assets.sh <vVERSION> <candidates> <out>}"
candidates="$(realpath "${2:?candidate directory required}")"
mkdir -p "${3:?output directory required}/assets"
out="$(realpath "$3")"
assets="$out/assets"
: "${GITHUB_REPOSITORY:?}" "${GITHUB_SHA:?}"
plain="${version#v}"
owner="${GITHUB_REPOSITORY%%/*}"
charts="ghcr.io/${owner,,}/charts"
published="$candidates/release-published"

files=(
  "$published/assets/digests.txt"
  "$published/assets/zeabur-$plain.tgz"
  "$candidates/release-darwin-arm64/zeaburctl-$version-darwin-arm64.tar.gz"
  "$candidates/release-docs-shots/docs-shots-$version.tar"
)
for arch in amd64 arm64; do
  files+=(
    "$candidates/release-linux-$arch/zeaburctl-$version-linux-$arch.tar.gz"
    "$candidates/release-linux-$arch/zeabur-$version-linux-$arch.tar.gz"
    "$candidates/release-installer-$arch/zeabur-installer-$version-linux-$arch.tar.gz"
  )
done
for file in "${files[@]}"; do
  [[ -f "$file" ]] || { echo "missing $file" >&2; exit 1; }
done
cp "${files[@]}" "$assets/"

(
  cd "$assets"
  sha256sum -- *.tar.gz *.tar *.tgz digests.txt >SHA256SUMS
  cosign sign-blob --yes --bundle SHA256SUMS.sigstore.json SHA256SUMS
)

cp "$published/notes.md" "$out/notes.md"
cat >>"$out/notes.md" <<EOF

Source commit: $GITHUB_SHA

## Install

The chart, pinned to the image below; [Install in a cluster](https://github.com/$GITHUB_REPOSITORY/blob/$version/docs/get-started/install.md)
shows how to verify both first and which values to set.

\`\`\`sh
helm install zeabur oci://$charts/zeabur --version $plain --namespace zeabur ...
\`\`\`

On a fresh Linux server, \`zeabur-installer\` installs RKE2, Zeabur Identity and Zeabur in one
step: see [Zeabur Installer](https://github.com/$GITHUB_REPOSITORY/blob/$version/docs/get-started/quickstart.md).

## Digests

\`\`\`text
$(cat "$assets/digests.txt")
\`\`\`

The macOS build of zeaburctl (Apple silicon) is signed with Sigstore but not notarized by
Apple: a browser download is quarantined (\`xattr -d com.apple.quarantine zeaburctl\`);
\`gh release download\` and \`curl\` are not.
EOF
