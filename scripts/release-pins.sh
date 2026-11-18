#!/usr/bin/env bash
# Checks that the Zeabur Catalog bundle this build embeds is the one the catalog's release at the
# pinned commit signed (flake.lock), and prints that release's tag.
#
#   scripts/release-pins.sh <vX.Y.Z>
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
download_sums catalog "$catalog"
for file in "${ZEABUR_CATALOG_RELEASE:?run inside nix develop}"/*; do
  check_listed catalog "$file" "$(basename "$file")"
done
echo "Zeabur Catalog $catalog"
