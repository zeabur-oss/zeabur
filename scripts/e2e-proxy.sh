#!/usr/bin/env bash
# Zeabur Proxy's canary against the real VictoriaLogs and VictoriaMetrics: no person reads another
# namespace's log lines or series, however the request is put (`cargo xtask e2e-proxy`,
# xtask/src/e2e_proxy.rs).
#
# Needs a cluster where the chart runs with `proxy.enabled` and the catalog's logs capability
# (VictoriaLogs, vlagent, its NetworkPolicy) is installed; KUBECONFIG points at it. Run inside
# `nix develop`. It removes what it made unless ZEABUR_PROXY_E2E_KEEP=1.
#   KUBECONFIG=target/dev-cluster/kubeconfig scripts/e2e-proxy.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
exec cargo -q run --locked --manifest-path "$root/Cargo.toml" -p xtask -- e2e-proxy "$@"
