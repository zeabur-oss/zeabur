# shellcheck shell=bash
# Sourced by the scripts that measure Zeabur's memory on the development cluster (e2e.sh,
# release-smoke.sh), with KUBECONFIG set.

# Zeabur's budget when idle (deploy/chart/zeabur/values.yaml), which e2e.sh checks.
memory_budget=$((30 * 1024 * 1024))

# Prints the idle working set in bytes of Zeabur's pod in namespace $1, as the kubelet reports it
# (Summary API, the one eviction uses). Right after the cluster comes up it moves by tens of MB
# while the pod settles, so it waits a minute, then reads every 10 s for up to a minute more and
# prints the first reading within the budget, or the lowest reading when none is.
idle_working_set() {
  local namespace=$1 pod node working_set='' reading
  pod=$(kubectl -n "$namespace" get pods -l app.kubernetes.io/name=zeabur -o jsonpath='{.items[0].metadata.name}')
  node=$(kubectl get nodes -o jsonpath='{.items[0].metadata.name}')
  sleep 60
  for _ in 1 2 3 4 5 6 7; do
    reading=$(kubectl get --raw "/api/v1/nodes/$node/proxy/stats/summary" |
      jq --arg ns "$namespace" --arg pod "$pod" \
        '.pods[] | select(.podRef.namespace == $ns and .podRef.name == $pod) | .memory.workingSetBytes')
    if [[ ! "$reading" =~ ^[0-9]+$ ]]; then
      echo "error: the kubelet reported no working set for pod $pod" >&2
      return 1
    fi
    echo "idle working set: $reading bytes ($((reading / 1024)) KiB; budget $((memory_budget / 1024)) KiB)" >&2
    if [[ -z "$working_set" ]] || ((reading < working_set)); then working_set=$reading; fi
    ((working_set <= memory_budget)) && break
    sleep 10
  done
  echo "$working_set"
}
