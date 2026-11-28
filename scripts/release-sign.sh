#!/usr/bin/env bash
# Signs zeaburctl for macOS with the Developer ID and notarizes it.
#
#   scripts/release-sign.sh <vX.Y.Z> <dist>
#
# Needs APPLE_CERTIFICATE_P12, APPLE_CERTIFICATE_PASSWORD, APPLE_API_KEY_P8, APPLE_API_KEY_ID and
# APPLE_API_ISSUER_ID. Without the certificate it leaves zeaburctl unsigned and only warns.
set -euo pipefail

usage="usage: release-sign.sh <vX.Y.Z> <dist>"
version=${1:?$usage}
dist=${2:?$usage}

if [[ "$(uname -sm)" != "Darwin arm64" ]]; then
  echo "only the macOS arm64 build is signed" >&2
  exit 2
fi
# Rehearsals and pull requests have no access to the certificate.
if [[ -z "${APPLE_CERTIFICATE_P12:-}" ]]; then
  echo "::warning::no Developer ID certificate: zeaburctl is not signed or notarized"
  exit 0
fi
: "${APPLE_CERTIFICATE_PASSWORD:?}" "${APPLE_API_KEY_P8:?}" "${APPLE_API_KEY_ID:?}"
: "${APPLE_API_ISSUER_ID:?}"

archive=$(cd "$dist" && pwd)/zeaburctl-$version-darwin-arm64.tar.gz
# The Nix shell's codesign is not Apple's, and its SDK has no notarytool.
unset DEVELOPER_DIR SDKROOT

work=$(mktemp -d)
keychain=$work/signing.keychain-db
# One quoted path per line.
old_keychains=()
while read -r path; do
  path=${path#\"}
  old_keychains+=("${path%\"}")
done < <(/usr/bin/security list-keychains -d user)

cleanup() {
  /usr/bin/security list-keychains -d user -s "${old_keychains[@]}" || true
  /usr/bin/security delete-keychain "$keychain" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

# A keychain of its own, so the certificate never lands in the runner's login keychain.
import_certificate() {
  local password
  password=$(uuidgen)
  /usr/bin/security create-keychain -p "$password" "$keychain"
  /usr/bin/security set-keychain-settings -lut 3600 "$keychain"
  /usr/bin/security unlock-keychain -p "$password" "$keychain"
  /usr/bin/security list-keychains -d user -s "$keychain" "${old_keychains[@]}"

  base64 --decode <<<"$APPLE_CERTIFICATE_P12" >"$work/certificate.p12"
  /usr/bin/security import "$work/certificate.p12" -k "$keychain" \
    -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
  /usr/bin/security set-key-partition-list -S apple-tool:,apple:,codesign: -s \
    -k "$password" "$keychain" >/dev/null
}

developer_id() {
  /usr/bin/security find-identity -v -p codesigning "$keychain" |
    rg -m 1 -o -r '$1' '\) ([0-9A-F]{40}) "Developer ID Application: ' || true
}

# A bare executable cannot hold the notarization ticket; Gatekeeper looks it up online.
notarize() {
  local file=$1
  printf '%s\n' "$APPLE_API_KEY_P8" >"$work/key.p8"
  local notary=(--key "$work/key.p8" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER_ID")

  /usr/bin/ditto -c -k --keepParent "$file" "$work/zeaburctl.zip"
  local result status id
  result=$(/usr/bin/xcrun notarytool submit "$work/zeaburctl.zip" "${notary[@]}" \
    --wait --timeout 30m --output-format json) || true
  status=$(jq -r '.status // empty' <<<"$result" 2>/dev/null || true)
  if [[ "$status" == Accepted ]]; then
    return
  fi

  echo "notarization: $result" >&2
  id=$(jq -r '.id // empty' <<<"$result" 2>/dev/null || true)
  if [[ -n "$id" ]]; then
    /usr/bin/xcrun notarytool log "$id" "${notary[@]}" >&2 || true
  fi
  echo "zeaburctl was not notarized" >&2
  exit 1
}

mkdir "$work/zeaburctl"
tar -xzf "$archive" -C "$work/zeaburctl"
zeaburctl=$work/zeaburctl/zeaburctl

import_certificate
identity=$(developer_id)
if [[ -z "$identity" ]]; then
  echo "the certificate is not a Developer ID Application certificate" >&2
  exit 1
fi

# Notarization needs the hardened runtime and a secure timestamp.
/usr/bin/codesign --force --options runtime --timestamp \
  --keychain "$keychain" --sign "$identity" "$zeaburctl"
/usr/bin/codesign --verify --strict --verbose=2 "$zeaburctl"
notarize "$zeaburctl"
"$zeaburctl" --version

# Pack the archive again the same way as release-build-zeaburctl.sh, so it stays reproducible.
commit_time=$(git show -s --format=%ct HEAD)
tar --format=ustar --sort=name --mtime="@$commit_time" \
  --owner=0 --group=0 --numeric-owner \
  -C "$work/zeaburctl" -cf - LICENSE zeaburctl |
  gzip -n -9 >"$archive"
