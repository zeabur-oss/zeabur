#!/usr/bin/env bash
# A k3d cluster with Zeabur Identity and Zeabur at https://zeabur.localhost, for development and
# for the in-cluster tests (e2e-l2, the kubernetes CI jobs, the release smoke). Runs on Linux with
# Docker and Nix; it uses this machine's port 443 and enters `nix develop` by itself. The work is
# `cargo xtask dev-cluster` (xtask/src/dev_cluster.rs); without arguments it prints its options.
#
#   nix run .#dev-cluster -- up [--profile bare|full] [--identity-src <dir>] [--builder-src <dir>]
#                               [--zeabur-image <image>] [--zeabur-chart <chart>]
#   nix run .#dev-cluster -- down
#   scripts/dev-cluster.sh snapshot        # cluster-scoped objects and namespace metadata
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
# The default dev shell sets ZEABUR_CA_BUNDLE.
if [[ -z "${ZEABUR_CA_BUNDLE:-}" ]]; then
  exec nix develop "$root" --command "$0" "$@"
fi
# From the caller's directory, so that relative paths in the options keep their meaning.
exec cargo -q run --locked --manifest-path "$root/Cargo.toml" -p xtask -- dev-cluster "$@"
