#!/usr/bin/env bash
# The catalog and Zeabur Identity releases that zeabur-installer of <version> installs, as
# ZEABUR_PIN_*=<value> lines (scripts/release-build.sh). Each value comes from the SHA256SUMS that
# the product's release signed, and cosign checks that signature first; every catalog file this
# build embeds (ZEABUR_CATALOG_RELEASE) must be one the catalog's release lists. Run inside
# `nix develop`, after `bun install`.
#   scripts/release-pins.sh v0.2.0
set -euo pipefail

version=${1:?usage: release-pins.sh <vVERSION>}
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -r "$work"' EXIT
fail() { echo "$*" >&2; exit 1; }

# pin_tag <repository> <pin>: the release tag of a pin (scripts/versions.ts).
pin_tag() {
  git ls-remote --tags "https://github.com/zeabur-oss/$1" |
    bun "$root/scripts/versions.ts" pin-tag "zeabur-oss/$1" "$version" "$2"
}
# download <repository> <tag> <file>
download() {
  curl --fail --silent --show-error --location --retry 3 --create-dirs \
    --output "$work/$1/$3" "https://github.com/zeabur-oss/$1/releases/download/$2/$3"
}
# signed_sums <repository> <tag>: SHA256SUMS, signed by that repository's release workflow, or by
# zeabur-oss/.github's shared one for the releases before it, run for that repository's tag.
signed_sums() {
  download "$1" "$2" SHA256SUMS
  download "$1" "$2" SHA256SUMS.sigstore.json
  cosign verify-blob --bundle "$work/$1/SHA256SUMS.sigstore.json" \
    --certificate-identity-regexp "^https://github\.com/zeabur-oss/(\.github|$1)/\.github/workflows/release\.yml@" \
    --certificate-github-workflow-repository "zeabur-oss/$1" \
    --certificate-github-workflow-ref "refs/tags/$2" \
    --certificate-oidc-issuer https://token.actions.githubusercontent.com \
    "$work/$1/SHA256SUMS" >&2
}
# listed <repository> <file>: the digest that repository's SHA256SUMS has for a file.
listed() {
  local digest
  digest=$(awk -v f="$2" '{ name = substr($0, 67) } name == f { print $1 }' "$work/$1/SHA256SUMS")
  [[ "$digest" =~ ^[0-9a-f]{64}$ ]] || fail "zeabur-oss/$1 SHA256SUMS lists no $2"
  echo "$digest"
}
# matches <repository> <path> <file>: the file at <path> is the one SHA256SUMS lists as <file>.
matches() {
  local want
  want=$(listed "$1" "$3")
  [[ "$(sha256sum "$2" | cut -d' ' -f1)" == "$want" ]] ||
    fail "$2 is not the $3 that zeabur-oss/$1 released"
}

catalog=$(pin_tag catalog "${ZEABUR_CATALOG_COMMIT:?run inside nix develop}")
identity=$(pin_tag identity "$(<"$root/ZEABUR_IDENTITY_PINNED")")

signed_sums catalog "$catalog"
for file in "${ZEABUR_CATALOG_RELEASE:?run inside nix develop}"/*; do
  matches catalog "$file" "$(basename "$file")"
done

signed_sums identity "$identity"
download identity "$identity" digests.txt
matches identity "$work/identity/digests.txt" digests.txt
image=$(awk 'NF == 2 && $1 == "index" { print $2 }' "$work/identity/digests.txt")
[[ "$image" =~ ^ghcr\.io/zeabur-oss/identity@sha256:[0-9a-f]{64}$ ]] ||
  fail "digests.txt names '$image' as the index, not one image of ghcr.io/zeabur-oss/identity"
download identity "$identity" resources.json
matches identity "$work/identity/resources.json" resources.json
request=$(jq -r .requestMemoryMiB "$work/identity/resources.json")
[[ "$request" =~ ^[1-9][0-9]{0,8}$ ]] || fail "resources.json gives $request as requestMemoryMiB"

catalog_index=$(listed catalog index.json)
ctl_amd64=$(listed identity "zeabur-identityctl-$identity-linux-amd64.tar.gz")
ctl_arm64=$(listed identity "zeabur-identityctl-$identity-linux-arm64.tar.gz")
cat <<EOF
ZEABUR_PIN_CATALOG_VERSION=$catalog
ZEABUR_PIN_CATALOG_SHA256=$catalog_index
ZEABUR_PIN_IDENTITY_VERSION=$identity
ZEABUR_PIN_IDENTITY_IMAGE=$image
ZEABUR_PIN_IDENTITYCTL_AMD64_SHA256=$ctl_amd64
ZEABUR_PIN_IDENTITYCTL_ARM64_SHA256=$ctl_arm64
ZEABUR_PIN_IDENTITY_REQUEST_MEMORY_MIB=$request
EOF
