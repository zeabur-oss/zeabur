#!/usr/bin/env bash
# Zeabur Proxy's canary (scripts/e2e-proxy.sh) against the real VictoriaLogs, on the development
# cluster: the catalog's VictoriaMetrics operator and VictoriaLogs go in as the Helm commands
# Zeabur Catalog gives. The cluster is deleted at the end; ZEABUR_CI_KEEP=1 keeps it.
set -euo pipefail
cd "$(dirname "$0")/../.."

# The catalog's commands for `$1` (<capability>/<id>), then the deployments of namespace `$2`.
install() {
  local work
  work=$(mktemp -d)
  cargo run -q --locked -p zeabur-catalog --example install -- "$1" single-node >"$work/install.sh"
  (cd "$work" && bash -euo pipefail install.sh)
  kubectl label namespace "$2" zeabur.com/platform=true --overwrite
  kubectl -n "$2" wait deployment --all --for=condition=Available --timeout=300s
}

if [[ -z "${ZEABUR_CI_KEEP:-}" ]]; then
  trap './scripts/dev-cluster.sh down' EXIT
fi
./scripts/dev-cluster.sh up --profile bare
export KUBECONFIG="$PWD/target/dev-cluster/kubeconfig"
install observability-operator/victoria-metrics-operator observability
install logs/victoria-logs observability
# The log store and its collector report operational once they are up.
kubectl -n observability wait vlsingle/victoria-logs --for=jsonpath='{.status.updateStatus}'=operational --timeout=300s
kubectl -n observability wait vlagent/collector --for=jsonpath='{.status.updateStatus}'=operational --timeout=300s
./scripts/e2e-proxy.sh
