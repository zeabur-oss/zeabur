#!/usr/bin/env bash
# Installs from a bundle on a host that cannot reach the internet, then runs `doctor`.
#   scripts/e2e-offline-bundle.sh <zeabur-installer> <config.yaml>
# It makes the bundle with the host still online, blocks every address outside the private ranges,
# and runs `plan`, `apply` and `doctor` with `bundle:` set. Images come through the
# `registryMirrors` in the configuration, which must name a mirror on a private address. A
# release-pinned installer also covers the pinned identityctl and Zeabur chart; a development
# build needs `sources` in the configuration. E2E_EXPECT_MIRROR_HOST names a registry host that
# `registries.yaml` must then list.
# It installs K3s or RKE2 and changes the firewall: run it on a disposable Linux VM
# (docs/internal/testing.md), as a user with sudo.
set -euo pipefail

if [[ $# != 2 ]]; then
  echo "usage: e2e-offline-bundle.sh <zeabur-installer> <config.yaml>" >&2
  exit 2
fi
installer="$(readlink -f "$1")"
config="$(readlink -f "$2")"
work="$(mktemp -d)"
chain=ZEABUR_E2E_OFFLINE

cleanup() {
  local status=$?
  trap - EXIT
  sudo iptables -D OUTPUT -j "$chain" 2>/dev/null || true
  sudo iptables -F "$chain" 2>/dev/null || true
  sudo iptables -X "$chain" 2>/dev/null || true
  sudo rm -rf "$work"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

echo "==> bundle (online)"
sudo "$installer" --no-telemetry -c "$config" bundle "$work/bundle"
sudo test -f "$work/bundle/oci-layout"
sudo test -f "$work/bundle/index.json"

offline="$work/offline.yaml"
{ cat "$config"; echo "bundle: $work/bundle"; } >"$offline"

echo "==> block everything outside the private ranges"
sudo iptables -N "$chain"
for range in 127.0.0.0/8 10.0.0.0/8 172.16.0.0/12 192.168.0.0/16; do
  sudo iptables -A "$chain" -d "$range" -j RETURN
done
sudo iptables -A "$chain" -j REJECT
sudo iptables -I OUTPUT 1 -j "$chain"
if curl --silent --max-time 5 --output /dev/null https://github.com; then
  echo "error: github.com is still reachable, the block did not work" >&2
  exit 1
fi

echo "==> plan and apply (offline)"
sudo "$installer" --no-telemetry -c "$offline" plan
sudo "$installer" --no-telemetry -c "$offline" apply

if [[ -n "${E2E_EXPECT_MIRROR_HOST:-}" ]]; then
  for file in /etc/rancher/k3s/registries.yaml /etc/rancher/rke2/registries.yaml; do
    if sudo test -f "$file"; then
      sudo grep -q "$E2E_EXPECT_MIRROR_HOST" "$file" ||
        { echo "error: $file does not list $E2E_EXPECT_MIRROR_HOST" >&2; exit 1; }
    fi
  done
fi

echo "==> doctor"
# Exit 1 means warnings only.
status=0
sudo "$installer" --no-telemetry -c "$offline" doctor || status=$?
if [[ "$status" != 0 && "$status" != 1 ]]; then
  echo "error: doctor failed with exit $status" >&2
  exit "$status"
fi

echo "offline install from a bundle: ok"
