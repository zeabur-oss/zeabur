#!/usr/bin/env bash
# Prints the digests.txt lines of the debug and tools images that Zeabur uses by default.
#
#   scripts/release-default-images.sh
#
# The release does not publish these images. It lists them so that people can mirror them.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
settings=$root/crates/zeabur-api/src/settings.rs

fail() {
  echo "$*" >&2
  exit 1
}

# image_of <constant>: the image that settings.rs pins in a constant.
image_of() {
  rg -o -r '$1' "^pub const $1: &str = \"([^\"]+)\";$" "$settings" || true
}

# check_platforms <image>: the registry still serves the image for both platforms.
check_platforms() {
  local platforms
  platforms=$(crane manifest "$1" | jq -r '.manifests[]?.platform | "\(.os)/\(.architecture)"')
  for platform in linux/amd64 linux/arm64; do
    if ! rg -qxF -- "$platform" <<<"$platforms"; then
      fail "$1 has no $platform image"
    fi
  done
}

# print_image <label> <constant>
print_image() {
  local image
  image=$(image_of "$2")
  if [[ ! "$image" =~ ^[^[:space:]@]+@sha256:[0-9a-f]{64}$ ]]; then
    fail "settings.rs does not pin $2 by digest: '$image'"
  fi
  check_platforms "$image"
  echo "$1 index  $image"
}

print_image debug DEBUG_IMAGE
print_image tools TOOLS_IMAGE
