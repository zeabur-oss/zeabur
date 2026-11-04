# shellcheck shell=bash
# Sourced by the scripts that wait on a cluster. The cluster is gone by the time anyone reads the
# CI log, so a wait that times out prints the namespace's pods, events and failing containers first.

# Prints, to standard error, what shows why pods in namespace $1 do not get ready.
diagnose() {
  local namespace=$1 pod
  kubectl -n "$namespace" get pods -o wide >&2 || true
  kubectl -n "$namespace" get events --sort-by=.lastTimestamp >&2 || true
  kubectl get nodes -o wide >&2 || true
  while read -r pod; do
    kubectl -n "$namespace" describe pod "$pod" >&2 || true
    kubectl -n "$namespace" logs --all-containers --prefix --tail=40 "$pod" >&2 || true
  done < <(kubectl -n "$namespace" get pods -o json 2>/dev/null | jq -r '
    .items[] | select(.status.phase != "Succeeded")
    | select(any(.status.conditions[]?; .type == "Ready" and .status == "True") | not)
    | .metadata.name' || true)
}

# Runs one kubectl wait in namespace $1; when it times out, says which one and diagnoses.
wait_for() {
  local namespace=$1
  shift
  if ! "$@" >/dev/null; then
    echo "error: timed out: $*" >&2
    diagnose "$namespace"
    exit 1
  fi
}
