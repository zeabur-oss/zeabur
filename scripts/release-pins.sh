#!/usr/bin/env bash
# Prints the catalog and Zeabur Identity pins of the installer as ZEABUR_PIN_*=<value> lines.
#
#   scripts/release-pins.sh <vX.Y.Z>
#
# Every value comes from a SHA256SUMS that cosign has verified.
set -euo pipefail

version=${1:?usage: release-pins.sh <vX.Y.Z>}
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -r "$work"' EXIT

fail() {
  echo "$*" >&2
  exit 1
}

# pin_tag <repository> <commit or tag>
pin_tag() {
  git ls-remote --tags "https://github.com/zeabur-oss/$1" |
    bun "$root/scripts/versions.ts" pin-tag "zeabur-oss/$1" "$version" "$2"
}

# download <repository> <tag> <file>
download() {
  curl --fail --silent --show-error --location --retry 3 --create-dirs \
    --output "$work/$1/$3" \
    "https://github.com/zeabur-oss/$1/releases/download/$2/$3"
}

# download_sums <repository> <tag>
# Older releases were signed by the shared workflow of zeabur-oss/.github, so both signers pass.
download_sums() {
  local repository=$1
  local tag=$2
  download "$repository" "$tag" SHA256SUMS
  download "$repository" "$tag" SHA256SUMS.sigstore.json
  local signer="^https://github\.com/zeabur-oss/(\.github|$repository)/\.github/workflows/release\.yml@"
  cosign verify-blob \
    --bundle "$work/$repository/SHA256SUMS.sigstore.json" \
    --certificate-identity-regexp "$signer" \
    --certificate-github-workflow-repository "zeabur-oss/$repository" \
    --certificate-github-workflow-ref "refs/tags/$tag" \
    --certificate-oidc-issuer https://token.actions.githubusercontent.com \
    "$work/$repository/SHA256SUMS" >&2
}

# listed <repository> <file>: the digest that the repository's SHA256SUMS has for the file.
listed() {
  local digest
  # Each line is "<64 hex digits>  <file>", so the name starts at column 67.
  digest=$(awk -v file="$2" 'substr($0, 67) == file { print $1 }' "$work/$1/SHA256SUMS")
  if [[ ! "$digest" =~ ^[0-9a-f]{64}$ ]]; then
    fail "SHA256SUMS of zeabur-oss/$1 does not list $2"
  fi
  echo "$digest"
}

# check_listed <repository> <path> <name>: the file at <path> is the <name> that SHA256SUMS lists.
check_listed() {
  local expected actual
  expected=$(listed "$1" "$3")
  actual=$(sha256sum "$2" | cut -d' ' -f1)
  if [[ "$actual" != "$expected" ]]; then
    fail "$2 is not the $3 that zeabur-oss/$1 released"
  fi
}

catalog=$(pin_tag catalog "${ZEABUR_CATALOG_COMMIT:?run inside nix develop}")
identity=$(pin_tag identity "$(<"$root/ZEABUR_IDENTITY_PINNED")")

# The catalog that this build embeds must be the released one.
download_sums catalog "$catalog"
for file in "${ZEABUR_CATALOG_RELEASE:?run inside nix develop}"/*; do
  check_listed catalog "$file" "$(basename "$file")"
done
catalog_sha256=$(listed catalog index.json)

download_sums identity "$identity"

download identity "$identity" digests.txt
check_listed identity "$work/identity/digests.txt" digests.txt
identity_image=$(awk 'NF == 2 && $1 == "index" { print $2 }' "$work/identity/digests.txt")
if [[ ! "$identity_image" =~ ^ghcr\.io/zeabur-oss/identity@sha256:[0-9a-f]{64}$ ]]; then
  fail "digests.txt of Zeabur Identity names '$identity_image' as its index"
fi

download identity "$identity" resources.json
check_listed identity "$work/identity/resources.json" resources.json
request_memory=$(jq -r .requestMemoryMiB "$work/identity/resources.json")
if [[ ! "$request_memory" =~ ^[1-9][0-9]{0,8}$ ]]; then
  fail "resources.json gives $request_memory as requestMemoryMiB"
fi

identityctl_amd64=$(listed identity "zeabur-identityctl-$identity-linux-amd64.tar.gz")
identityctl_arm64=$(listed identity "zeabur-identityctl-$identity-linux-arm64.tar.gz")

echo "ZEABUR_PIN_CATALOG_VERSION=$catalog"
echo "ZEABUR_PIN_CATALOG_SHA256=$catalog_sha256"
echo "ZEABUR_PIN_IDENTITY_VERSION=$identity"
echo "ZEABUR_PIN_IDENTITY_IMAGE=$identity_image"
echo "ZEABUR_PIN_IDENTITYCTL_AMD64_SHA256=$identityctl_amd64"
echo "ZEABUR_PIN_IDENTITYCTL_ARM64_SHA256=$identityctl_arm64"
echo "ZEABUR_PIN_IDENTITY_REQUEST_MEMORY_MIB=$request_memory"
