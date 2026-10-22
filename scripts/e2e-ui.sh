#!/usr/bin/env bash
# Browser tests of the web interface in local mode: the release `zeaburctl ui`, with the
# interface embedded, against the current kubeconfig context (a disposable cluster), driven by
# Playwright. Run inside `nix develop` on Linux after scripts/build-ui.sh.
#   scripts/e2e-ui.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
namespace=zeabur-e2e
out="$root/target/e2e-ui"
mkdir -p "$out"

# Runs one kubectl wait; when it times out, says which one and shows the namespace's pods and
# events, since the cluster is gone by the time anyone reads the CI log.
wait_for() {
  if ! "$@" >/dev/null; then
    echo "error: timed out: $*" >&2
    kubectl -n "$namespace" get pods -o wide >&2 || true
    kubectl -n "$namespace" get events --sort-by=.lastTimestamp >&2 || true
    kubectl get nodes -o wide >&2 || true
    exit 1
  fi
}

# The write tests start from the manifest as applied: what an earlier run added (a variable, a
# volume, a policy) goes with the namespace.
wait_for kubectl delete namespace "$namespace" "$namespace-adopt" --ignore-not-found --wait --timeout=180s
kubectl apply -f "$root/web/e2e/manifests/talker.yaml" \
  -f "$root/web/e2e/manifests/second-project.yaml" >/dev/null
wait_for kubectl -n "$namespace" rollout status deployment/talker --timeout=180s
wait_for kubectl -n "$namespace" rollout status deployment/editor --timeout=180s
pod=$(kubectl -n "$namespace" get pods -l app=talker -o jsonpath='{.items[0].metadata.name}')
# The crashing Deployment must have crashed a few times before the Overview test looks for it.
# Between restarts the kubelet reports the container as `waiting: CrashLoopBackOff`,
# `terminated: Error`, or, for a moment after each restart, `running` (observed on k3s v1.36.5),
# so wait on the restart count, not the reason.
restarts=0
for _ in $(seq 1 90); do
  restarts=$(kubectl -n "$namespace" get pods -l app=crasher \
    -o jsonpath='{.items[0].status.containerStatuses[0].restartCount}' 2>/dev/null || true)
  if [[ "${restarts:-0}" -ge 2 ]]; then break; fi
  sleep 2
done
if [[ "${restarts:-0}" -lt 2 ]]; then
  echo "error: deployment/crasher has not restarted twice (restarts: ${restarts:-0})" >&2
  kubectl -n "$namespace" get pods -o wide >&2 || true
  exit 1
fi

ZEABUR_UI_DIST="$root/web/dist" cargo build --release --locked -p zeaburctl
"$root/target/release/zeaburctl" ui --no-browser >"$out/zeaburctl.log" 2>&1 &
server=$!
trap 'kill "$server" 2>/dev/null || true' EXIT
url=""
for _ in $(seq 1 100); do
  url=$(grep -o 'http://127.0.0.1:[0-9]*/?zeabur-launch=[0-9a-f]*' "$out/zeaburctl.log" || true)
  [[ -n "$url" ]] && break
  sleep 0.2
done
if [[ -z "$url" ]]; then
  echo "error: zeaburctl ui printed no link:" >&2
  cat "$out/zeaburctl.log" >&2
  exit 1
fi

cd "$root/web"
ZEABUR_E2E_URL="$url" ZEABUR_E2E_NAMESPACE="$namespace" ZEABUR_E2E_POD="$pod" \
  ZEABUR_E2E_ADOPT="$namespace-adopt" \
  bunx playwright test --reporter=line --output="$out/results"
