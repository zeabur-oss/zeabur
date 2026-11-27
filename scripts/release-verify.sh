#!/usr/bin/env bash
# Verifies a draft release the way a user would get it, on this machine's platform.
#
#   scripts/release-verify.sh <vX.Y.Z> <downloaded>
#
# Run `zeabur-oss-release verify-assets <downloaded>` first. RELEASE_SOURCE_REPOSITORY and
# RELEASE_SOURCE_REF name the workflow that must have signed the release.
set -euo pipefail

usage="usage: release-verify.sh <vX.Y.Z> <downloaded>"
version=${1:?$usage}
downloaded=$(cd "${2:?$usage}" && pwd)
root=$(cd "$(dirname "$0")/.." && pwd)

case "$(uname -sm)" in
  "Linux x86_64") platform=linux-amd64 ;;
  "Linux aarch64") platform=linux-arm64 ;;
  "Darwin arm64") platform=darwin-arm64 ;;
  *)
    echo "verify on Linux amd64 or arm64, or on macOS arm64" >&2
    exit 2
    ;;
esac

fail() {
  echo "$*" >&2
  exit 1
}

release() {
  bun x zeabur-oss-release "$@"
}

work=$(mktemp -d)
trap 'rm -r "$work"' EXIT
# An empty registry login, so that a maintainer's login cannot hide a private package.
export DOCKER_CONFIG=$work/docker
export HELM_REGISTRY_CONFIG=$work/docker/config.json
mkdir "$DOCKER_CONFIG"

digests=$downloaded/digests.txt
chart=$downloaded/zeabur-${version#v}.tgz

check_asset_list() {
  local expected actual
  expected=$("$root/scripts/release-assets.sh" "$version" | while read -r name _; do echo "$name"; done | sort)
  actual=$(cd "$downloaded" && printf '%s\n' * | rg -v '^SHA256SUMS' | sort)
  if [[ "$actual" != "$expected" ]]; then
    fail "the draft has these files:"$'\n'"$actual"$'\n'"and not these:"$'\n'"$expected"
  fi
}

# Each default image must be listed once, as the code pins it.
check_default_images() {
  local lines line label
  lines=$("$root/scripts/release-default-images.sh")
  while read -r line; do
    label=${line%% *}
    if [[ "$(rg -c "^$label index " "$digests")" != 1 ]] || ! rg -qxF -- "$line" "$digests"; then
      fail "digests.txt does not list ${line##* } as its one $label image"
    fi
  done <<<"$lines"
}

# has_developer_id <file>
has_developer_id() {
  /usr/bin/codesign -dvv "$1" 2>&1 | rg -q '^Authority=Developer ID Application: '
}

# check_notarized <file> <name>: a file with a Developer ID signature must also be notarized.
check_notarized() {
  if ! has_developer_id "$1"; then
    echo "$2 is not signed with a Developer ID"
    return
  fi
  local assessment
  if ! assessment=$(/usr/sbin/spctl --assess -vvv --type open \
    --context context:primary-signature "$1" 2>&1); then
    fail "$2 is not notarized: $assessment"
  fi
  if ! rg -q 'source=Notarized Developer ID' <<<"$assessment"; then
    fail "$2 is not notarized: $assessment"
  fi
}

verify_macos() {
  # The Nix shell's codesign is not Apple's.
  unset DEVELOPER_DIR SDKROOT

  tar -xOzf "$downloaded/zeaburctl-$version-$platform.tar.gz" zeaburctl >"$work/zeaburctl"
  chmod 0755 "$work/zeaburctl"
  local said
  said=$("$work/zeaburctl" --version)
  if [[ "$said" != "zeaburctl ${version#v}" ]]; then
    fail "zeaburctl says '$said'"
  fi
  check_notarized "$work/zeaburctl" zeaburctl

  local dmg=$downloaded/zeabur-desktop-$version-$platform.dmg
  if has_developer_id "$dmg"; then
    /usr/bin/xcrun stapler validate "$dmg"
  fi
  check_notarized "$dmg" "the Zeabur Desktop .dmg"
}

echo "verifying $version on $platform"
check_asset_list

zeabur_image=$(release verify-image --digests "$digests" linux/amd64 linux/arm64 | tail -n 1)
proxy_image=$(release verify-image --digests "$digests" --label proxy linux/amd64 linux/arm64 |
  tail -n 1)
database_updater_image=$(release verify-image --digests "$digests" --label database-updater linux/amd64 linux/arm64 |
  tail -n 1)
check_default_images
chart_ref=$(release verify-chart --digests "$digests" "$chart" | tail -n 1)

# Include optional workloads when verifying the published image pins.
expected_images=$(printf '%s\n' "$zeabur_image" "$proxy_image" "$database_updater_image" | sort)
chart_images=$("$root/scripts/chart-images.sh" "$chart")
if [[ "$chart_images" != "$expected_images" ]]; then
  fail "the chart runs:"$'\n'"$chart_images"$'\n'"not the images in digests.txt:"$'\n'"$expected_images"
fi

if [[ "$platform" == darwin-arm64 ]]; then
  verify_macos
else
  # The node pulls the published image, as it would for a user.
  "$root/scripts/release-smoke.sh" "$downloaded/zeaburctl-$version-$platform.tar.gz" \
    "oci://$chart_ref"
fi

echo "release $version verified on $platform"
