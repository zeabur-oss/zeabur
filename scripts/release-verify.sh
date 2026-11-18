#!/usr/bin/env bash
# What a user does with a draft's files in <downloaded>, after verify-assets has checked SHA256SUMS
# and each asset's provenance: the draft has exactly the release's assets, the images are signed
# indexes of both platforms, digests.txt lists the default images as the code pins them, the chart
# runs the published images and no other, and the programs run. On Linux amd64 or arm64, the
# published chart on a disposable k3s whose node pulls the published image (release-smoke.sh); on
# macOS arm64, zeaburctl and the Zeabur Desktop .dmg, notarized when signed with a Developer ID.
# Run inside `nix develop`, after `bun install`.
#   scripts/release-verify.sh v0.2.0 downloaded
#
# RELEASE_SOURCE_REPOSITORY (zeabur-oss/zeabur) and RELEASE_SOURCE_REF (refs/tags/<version>) name
# the signer.
set -euo pipefail

usage="usage: release-verify.sh <vVERSION> <downloaded>"
version=${1:?$usage}
downloaded=$(cd "${2:?$usage}" && pwd)
root=$(cd "$(dirname "$0")/.." && pwd)
case "$(uname -sm)" in
  "Linux x86_64") platform=linux-amd64 ;;
  "Linux aarch64") platform=linux-arm64 ;;
  "Darwin arm64") platform=darwin-arm64 ;;
  *) echo "verify on Linux amd64 or arm64, or macOS arm64" >&2; exit 2 ;;
esac
fail() { echo "$*" >&2; exit 1; }
release() { bun x zeabur-oss-release "$@"; }
work=$(mktemp -d)
trap 'rm -r "$work"' EXIT
# A fresh, empty registry login: a publisher's login must not hide a private package.
export DOCKER_CONFIG=$work/docker HELM_REGISTRY_CONFIG=$work/docker/config.json
mkdir "$DOCKER_CONFIG"
echo "verifying $version on $platform"

want=$("$root/scripts/release-assets.sh" "$version" | cut -d' ' -f1 | sort)
got=$(cd "$downloaded" && printf '%s\n' * | grep -v '^SHA256SUMS' | sort)
[[ "$got" == "$want" ]] || fail "the draft has"$'\n'"$got"$'\n'"not"$'\n'"$want"

digests=$downloaded/digests.txt
chart=$downloaded/zeabur-${version#v}.tgz
zeabur=$(release verify-image --digests "$digests" linux/amd64 linux/arm64 | tail -n 1)
proxy=$(release verify-image --digests "$digests" --label proxy linux/amd64 linux/arm64 | tail -n 1)
indexes=$(printf '%s\n' "$zeabur" "$proxy" | sort)
defaults=$("$root/scripts/release-default-images.sh")
while read -r line; do
  label=${line%% *}
  if [[ "$(grep -c "^$label index " "$digests")" != 1 ]] || ! grep -qxF "$line" "$digests"; then
    fail "digests.txt does not list ${line##* } as the one $label image"
  fi
done <<<"$defaults"
chart_ref=$(release verify-chart --digests "$digests" "$chart" | tail -n 1)
pinned=$("$root/scripts/chart-images.sh" "$chart")
[[ "$pinned" == "$indexes" ]] ||
  fail "the chart runs"$'\n'"$pinned"$'\n'"not the images in digests.txt"$'\n'"$indexes"

archive=$downloaded/zeaburctl-$version-$platform.tar.gz
if [[ "$platform" == linux-* ]]; then
  "$root/scripts/release-smoke.sh" "$archive" "oci://$chart_ref"
else
  # Apple's tools, not the Nix shell's (release-sign.sh).
  unset DEVELOPER_DIR SDKROOT
  # notarized <file> <name>: Gatekeeper accepts a file signed with a Developer ID as notarized.
  notarized() {
    if /usr/bin/codesign -dvv "$1" 2>&1 | grep -q '^Authority=Developer ID Application: '; then
      local assessed
      if ! assessed=$(/usr/sbin/spctl --assess -vvv --type open \
        --context context:primary-signature "$1" 2>&1) ||
        ! grep -q 'source=Notarized Developer ID' <<<"$assessed"; then
        fail "$2 is not notarized: $assessed"
      fi
    else
      echo "$2 is not signed with a Developer ID"
    fi
  }
  tar -xOzf "$archive" zeaburctl >"$work/zeaburctl"
  chmod 0755 "$work/zeaburctl"
  said=$("$work/zeaburctl" --version)
  notarized "$work/zeaburctl" zeaburctl
  [[ "$said" == "zeaburctl ${version#v}" ]] || fail "zeaburctl says '$said'"
  dmg=$downloaded/zeabur-desktop-$version-$platform.dmg
  if /usr/bin/codesign -dvv "$dmg" 2>&1 | grep -q '^Authority=Developer ID Application: '; then
    /usr/bin/xcrun stapler validate "$dmg"
  fi
  notarized "$dmg" "the Zeabur Desktop .dmg"
fi
echo "release $version verified on $platform"
