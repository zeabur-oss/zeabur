#!/usr/bin/env bash
# Tries release artifacts as they will ship, never rebuilding them: the zeaburctl archive, and the
# chart installed on the development cluster (scripts/dev-cluster.sh). With an image (a local
# candidate from release-build.sh), the chart runs exactly that image, imported into the node;
# without one, the node pulls the image the chart names, as an installation would
# (`release.sh verify`). Run inside `nix develop` on Linux amd64 or arm64 with Docker; it uses port
# 443 and never touches the caller's kubeconfig.
#   scripts/release-smoke.sh dist/zeaburctl-v0.0.2-linux-arm64.tar.gz deploy/chart/zeabur zeabur:release-arm64
#   scripts/release-smoke.sh zeaburctl-v0.0.2-linux-arm64.tar.gz oci://ghcr.io/zeabur-oss/charts/zeabur@sha256:…
set -euo pipefail

if [[ $# != 2 && $# != 3 ]]; then
  echo "usage: release-smoke.sh <zeaburctl-archive> <chart> [<image>]" >&2
  exit 2
fi
archive="$1"
chart="$2"
image="${3:-}"
: "${IN_NIX_SHELL:?run inside nix develop}"
case "$(uname -s):$(uname -m)" in
  Linux:x86_64) arch=amd64 ;;
  Linux:aarch64) arch=arm64 ;;
  *) echo "release smoke runs on Linux amd64 or arm64 only" >&2; exit 2 ;;
esac
work="$(mktemp -d)"
root="$(cd "$(dirname "$0")/.." && pwd)"
state="$root/target/dev-cluster"

cleanup() {
  local status=$?
  trap - EXIT
  "$root/scripts/dev-cluster.sh" down >&2 || true
  rm -rf "$work"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# The archive holds the binary and the licence, nothing else; the binary is streamed out rather
# than extracted, so no ownership, link or path in the archive is honoured.
listing="$(tar -tzf "$archive")"
if [[ "$listing" != $'LICENSE\nzeaburctl' ]]; then
  echo "expected an archive of LICENSE and zeaburctl, got: $listing" >&2
  exit 1
fi
tar -xOzf "$archive" zeaburctl >"$work/zeaburctl"
chmod 0755 "$work/zeaburctl"
version="$("$work/zeaburctl" --version)"
echo "$version"
version="${version#zeaburctl }"
expected="$(basename "$archive")"
expected="${expected#zeaburctl-v}"
expected="${expected%-linux-*}"
if [[ "$version" != "$expected" ]]; then
  echo "zeaburctl says version $version, the archive is $expected" >&2
  exit 1
fi

# The chart's own image when no candidate is given, as an installation would pull it.
if [[ -n "$image" ]]; then
  platform="$(docker image inspect --format '{{.Os}}/{{.Architecture}}' "$image")"
  if [[ "$platform" != "linux/$arch" ]]; then
    echo "image $image is $platform, this machine linux/$arch" >&2
    exit 1
  fi
else
  image="$(helm show values "$chart" | yq '.image.repository + "@" + .image.digest')"
fi

# The chart on the development cluster, with zeabur-installer's values, Zeabur Identity, and Zeabur
# Proxy off as the chart's default.
"$root/scripts/dev-cluster.sh" up --zeabur-chart "$chart" --zeabur-image "$image"
export KUBECONFIG="$state/kubeconfig"
running="$(kubectl -n zeabur get pods -l app.kubernetes.io/name=zeabur \
  -o jsonpath='{.items[0].status.containerStatuses[0].imageID}')"
echo "zeabur runs $running"
health="$(curl --fail --silent --show-error --max-time 10 --cacert "$state/tls/ca.crt" \
  --resolve zeabur.localhost:443:127.0.0.1 https://zeabur.localhost/api/healthz)"
[[ "$health" == ok ]] || { echo "/api/healthz answered '$health'" >&2; exit 1; }

# zeaburctl against the same cluster as its admin: a JSON report and an exit code of 0 (all ok)
# or 1 (warnings), as in scripts/ci.sh.
code=0
"$work/zeaburctl" doctor --output json >"$work/doctor.json" || code=$?
[[ $code -eq 0 || $code -eq 1 ]] || { cat "$work/doctor.json" >&2; echo "zeaburctl doctor exited $code" >&2; exit 1; }
jq -e '.checks | length > 0' "$work/doctor.json" >/dev/null

echo "smoke passed: $(basename "$archive"), chart $chart, image $image"
