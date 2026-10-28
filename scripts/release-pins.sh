#!/usr/bin/env bash
# Prints the pins of the catalog and Zeabur Identity releases that zeabur-installer installs, as
# `ZEABUR_PIN_*=value` lines. The catalog is the commit in flake.lock, Zeabur Identity the release
# tag (or a commit that carries one) in ZEABUR_IDENTITY_PINNED. Each value comes from a checksum
# list that the product's release signed, and cosign checks that signature first. The catalog files this build embeds (ZEABUR_CATALOG_RELEASE) must match that
# list. release-build.sh compiles the pins into the installer (`scripts/release.sh finish`). Run
# inside `nix develop`.
#   scripts/release-pins.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
: "${IN_NIX_SHELL:?run inside nix develop}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Prints the one release tag of a pin: the pin itself when it is a release tag that exists, else
# the one release tag on the commit.
tag_at() { # $1 = repository, $2 = release tag or commit
  local tags
  if [[ "$2" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    git ls-remote --exit-code --tags "https://github.com/zeabur-oss/$1" "refs/tags/$2" >/dev/null ||
      { echo "zeabur-oss/$1 has no release tag $2" >&2; return 1; }
    echo "$2"
    return
  fi
  tags="$(git ls-remote --tags "https://github.com/zeabur-oss/$1" |
    awk -v c="$2" '$1 == c { sub("^refs/tags/", "", $2); sub("\\^\\{\\}$", "", $2); print $2 }' |
    { grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' || true; } | sort -u)"
  if [[ -z "$tags" || "$tags" == *$'\n'* ]]; then
    echo "zeabur-oss/$1 $2 has no single release tag: tag it, or move the pin" >&2
    return 1
  fi
  echo "$tags"
}
catalog="$(tag_at catalog "$ZEABUR_CATALOG_COMMIT")"
identity="$(tag_at identity "$(tr -d '[:space:]' <"$root/ZEABUR_IDENTITY_PINNED")")"

# Downloads a release's checksum list and checks its signature: by zeabur-oss/.github's shared
# release workflow, run for that repository's tag (the signer of every zeabur-oss release from
# v0.1.1 on).
signed_list() { # $1 = repository, $2 = version, $3 = list
  local base="https://github.com/zeabur-oss/$1/releases/download/$2"
  mkdir -p "$work/$1"
  for file in "$3" "$3.sigstore.json"; do
    curl --fail --silent --show-error --location --retry 3 --output "$work/$1/$file" "$base/$file"
  done
  cosign verify-blob --bundle "$work/$1/$3.sigstore.json" \
    --certificate-identity-regexp '^https://github\.com/zeabur-oss/\.github/\.github/workflows/release\.yml@' \
    --certificate-github-workflow-repository "zeabur-oss/$1" --certificate-github-workflow-ref "refs/tags/$2" \
    --certificate-oidc-issuer https://token.actions.githubusercontent.com "$work/$1/$3" >&2
}

sha() { # $1 = list, $2 = file
  local digest
  digest="$(awk -v f="$2" '$2 == f { print $1 }' "$1")"
  [[ "$digest" =~ ^[0-9a-f]{64}$ ]] || { echo "$1 has no single digest of $2" >&2; return 1; }
  echo "$digest"
}

signed_list catalog "$catalog" checksums.txt
(cd "$ZEABUR_CATALOG_RELEASE" && sha256sum --check --strict "$work/catalog/checksums.txt") >&2
index="$(sha "$work/catalog/checksums.txt" index.json)"

signed_list identity "$identity" SHA256SUMS
curl --fail --silent --show-error --location --retry 3 --output "$work/identity/image-digests.txt" \
  "https://github.com/zeabur-oss/identity/releases/download/$identity/image-digests.txt"
listed="$(sha "$work/identity/SHA256SUMS" image-digests.txt)"
[[ "$(sha256sum "$work/identity/image-digests.txt" | cut -d ' ' -f 1)" == "$listed" ]] ||
  { echo "image-digests.txt does not match SHA256SUMS" >&2; exit 1; }
image="$(awk '$1 == "index" { print $2 }' "$work/identity/image-digests.txt")"
[[ "$image" =~ ^ghcr\.io/zeabur-oss/identity@sha256:[0-9a-f]{64}$ ]] ||
  { echo "image-digests.txt has no single index digest" >&2; exit 1; }

ctl_amd64="$(sha "$work/identity/SHA256SUMS" "zeabur-identityctl-$identity-linux-amd64.tar.gz")"
ctl_arm64="$(sha "$work/identity/SHA256SUMS" "zeabur-identityctl-$identity-linux-arm64.tar.gz")"

cat <<EOF
ZEABUR_PIN_CATALOG_VERSION=$catalog
ZEABUR_PIN_CATALOG_SHA256=$index
ZEABUR_PIN_IDENTITY_VERSION=$identity
ZEABUR_PIN_IDENTITY_IMAGE=$image
ZEABUR_PIN_IDENTITYCTL_AMD64_SHA256=$ctl_amd64
ZEABUR_PIN_IDENTITYCTL_ARM64_SHA256=$ctl_arm64
EOF
