# shellcheck shell=bash
# Sourced by the scripts that measure Zeabur's memory on the development cluster (e2e.sh,
# release-smoke.sh), with KUBECONFIG set.

# Zeabur's budget when idle (deploy/chart/zeabur/values.yaml), which e2e.sh checks.
memory_budget=$((30 * 1024 * 1024))

# Prints the idle anonymous memory in bytes of Zeabur's pod in namespace $1, as the kubelet reports
# it (Summary API, rssBytes): the memory Zeabur allocates. The working set adds the page cache of
# the executable's mapped pages, which the kernel charges to the pod or not by which process read
# them first, so the same build reads 0 to 30 MiB more from run to run. After the browser tests,
# the keep-alive connections they leave hold memory until the proxy closes them (90 s), so it
# waits a minute, then reads every 10 s for up to a minute more and prints the first reading
# within the budget, or the lowest reading when none is.
idle_memory() {
  local namespace=$1 pod node anonymous='' reading working_set
  pod=$(kubectl -n "$namespace" get pods -l app.kubernetes.io/name=zeabur -o jsonpath='{.items[0].metadata.name}')
  node=$(kubectl get nodes -o jsonpath='{.items[0].metadata.name}')
  sleep 60
  for _ in 1 2 3 4 5 6 7; do
    read -r reading working_set < <(kubectl get --raw "/api/v1/nodes/$node/proxy/stats/summary" |
      jq -r --arg ns "$namespace" --arg pod "$pod" \
        '.pods[] | select(.podRef.namespace == $ns and .podRef.name == $pod) | .memory
          | "\(.rssBytes) \(.workingSetBytes)"')
    if [[ ! "$reading" =~ ^[0-9]+$ ]]; then
      echo "error: the kubelet reported no memory for pod $pod" >&2
      return 1
    fi
    echo "idle memory: $reading bytes anonymous ($((reading / 1024)) KiB; working set $((working_set / 1024)) KiB; budget $((memory_budget / 1024)) KiB)" >&2
    if [[ -z "$anonymous" ]] || ((reading < anonymous)); then anonymous=$reading; fi
    ((anonymous <= memory_budget)) && break
    sleep 10
  done
  echo "$anonymous"
}
