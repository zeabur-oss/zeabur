#!/usr/bin/env bash
# Zeabur in a cluster with Zeabur Identity, from their images and Zeabur's chart, through a browser:
# passkey sign-in, terminal, rights, memory, removal. ZEABUR_CI_KEEP=1 keeps the cluster.
set -euo pipefail
cd "$(dirname "$0")/../.."

# The image's size and Zeabur's memory, in the run's summary when there is one. The workflow
# uploads the measurements file.
report() {
  local measurements="target/dev-cluster/measurements.json"
  {
    echo "### Zeabur in the cluster, \`ci\` build (${K3S_IMAGE:-the default k3s})"
    echo "| Measure | Bytes |"
    echo "|---|---|"
    echo "| Image \`zeabur\` | $(docker image inspect --format '{{.Size}}' zeabur:dev 2>/dev/null || echo n/a) |"
    echo "| Pod working set when idle | $(jq -r .pod_working_set_bytes "$measurements" 2>/dev/null || echo n/a) |"
  } | tee -a "${GITHUB_STEP_SUMMARY:-/dev/null}"
}

status=0
ZEABUR_E2E_KEEP="${ZEABUR_CI_KEEP:-}" ./scripts/e2e.sh || status=$?
report
exit "$status"
