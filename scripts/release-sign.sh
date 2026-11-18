#!/usr/bin/env bash
# Signs zeaburctl in <dist>/zeaburctl-<version>-darwin-arm64.tar.gz with the Developer ID, notarizes
# it, and writes the archive again as release-build.sh makes it. macOS only, inside `nix develop`.
# Without APPLE_CERTIFICATE_P12 (release.yml passes the secrets only on main and tags) zeaburctl
# keeps the linker's ad hoc signature, and the release goes on. With it, APPLE_CERTIFICATE_PASSWORD,
# APPLE_API_KEY_P8, APPLE_API_KEY_ID and APPLE_API_ISSUER_ID must be set too.
#   scripts/release-sign.sh v0.2.0 dist
set -euo pipefail

usage="usage: release-sign.sh <vVERSION> <dist>"
version=${1:?$usage}
dist=${2:?$usage}
[[ "$(uname -sm)" == "Darwin arm64" ]] || { echo "only the macOS build is signed" >&2; exit 2; }
if [[ -z "${APPLE_CERTIFICATE_P12:-}" ]]; then
  echo "::warning::no Developer ID certificate: zeaburctl is not signed or notarized"
  exit 0
fi
: "${APPLE_CERTIFICATE_PASSWORD:?}" "${APPLE_API_KEY_P8:?}" "${APPLE_API_KEY_ID:?}" \
  "${APPLE_API_ISSUER_ID:?}"
archive=$(cd "$dist" && pwd)/zeaburctl-$version-darwin-arm64.tar.gz
# Apple's tools, not the Nix shell's: its codesign is sigtool's, and its DEVELOPER_DIR and SDKROOT
# point xcrun at a bare SDK without notarytool.
unset DEVELOPER_DIR SDKROOT

work=$(mktemp -d)
keychain=$work/signing.keychain-db
mapfile -t keychains < <(/usr/bin/security list-keychains -d user | sed -E 's/^ *"?//; s/"? *$//')
cleanup() {
  /usr/bin/security list-keychains -d user -s "${keychains[@]}" || true
  /usr/bin/security delete-keychain "$keychain" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

mkdir "$work/zeaburctl"
tar -xzf "$archive" -C "$work/zeaburctl"
file=$work/zeaburctl/zeaburctl

secret=$(uuidgen)
/usr/bin/security create-keychain -p "$secret" "$keychain"
/usr/bin/security set-keychain-settings -lut 3600 "$keychain"
/usr/bin/security unlock-keychain -p "$secret" "$keychain"
/usr/bin/security list-keychains -d user -s "$keychain" "${keychains[@]}"
base64 --decode <<<"$APPLE_CERTIFICATE_P12" >"$work/certificate.p12"
/usr/bin/security import "$work/certificate.p12" -k "$keychain" -P "$APPLE_CERTIFICATE_PASSWORD" \
  -T /usr/bin/codesign
/usr/bin/security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$secret" \
  "$keychain" >/dev/null
identity=$(/usr/bin/security find-identity -v -p codesigning "$keychain" |
  awk '/"Developer ID Application: / { print $2; exit }')
[[ -n "$identity" ]] || { echo "the certificate is not a Developer ID Application one" >&2; exit 1; }
# The hardened runtime and a secure timestamp, which notarization requires.
/usr/bin/codesign --force --options runtime --timestamp --keychain "$keychain" --sign "$identity" \
  "$file"
/usr/bin/codesign --verify --strict --verbose=2 "$file"

# A bare executable cannot carry the ticket; Gatekeeper looks it up online.
printf '%s\n' "$APPLE_API_KEY_P8" >"$work/key.p8"
notary=(--key "$work/key.p8" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER_ID")
/usr/bin/ditto -c -k --keepParent "$file" "$work/zeaburctl.zip"
result=$(/usr/bin/xcrun notarytool submit "$work/zeaburctl.zip" "${notary[@]}" --wait --timeout 30m \
  --output-format json) || true
if [[ "$(jq -r '.status // empty' <<<"$result" 2>/dev/null)" != Accepted ]]; then
  echo "notarization: $result" >&2
  id=$(jq -r '.id // empty' <<<"$result" 2>/dev/null || true)
  [[ -z "$id" ]] || /usr/bin/xcrun notarytool log "$id" "${notary[@]}" >&2 || true
  echo "zeaburctl was not notarized" >&2
  exit 1
fi
"$file" --version

# The archive again, as release-build.sh makes it.
tar --format=ustar --sort=name --mtime="@$(git show -s --format=%ct HEAD)" --owner=0 --group=0 \
  --numeric-owner -C "$work/zeaburctl" -cf - LICENSE zeaburctl | gzip -n -9 >"$archive"
