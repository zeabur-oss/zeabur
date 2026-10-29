#!/usr/bin/env bash
# Zeabur in a cluster, end to end, on the development cluster (scripts/dev-cluster.sh: Zeabur
# Identity and Zeabur from zeabur-installer's objects, behind Traefik over HTTPS): a browser
# (web/e2e/l2) enrols a passkey, signs in, confirms a terminal with the passkey again, runs a
# command, then reads and changes with admin rights; an admin invites a Developer and a Viewer,
# who each see what their role allows. Then, from the outside: Zeabur's service account can do
# nothing a fresh one cannot, the idle pod stays within its memory budget, and removing Zeabur
# leaves the cluster as it was.
#
# Run on Linux with Docker and Nix; it uses this machine's port 443. Arguments go to
# `dev-cluster.sh up` (`--profile full`, `--identity-src <dir>`). The cluster is deleted at the
# end; ZEABUR_L2_KEEP=1 keeps it, with Zeabur, and skips the removal check.
#   scripts/e2e-l2.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
if [[ -z "${ZEABUR_CA_BUNDLE:-}" ]]; then
  exec nix develop "$root" --command "$0" "$@"
fi
state="$root/target/dev-cluster"
namespace=zeabur             # Zeabur's release
project=zeabur-e2e           # the project the browser opens (web/e2e/manifests/talker.yaml)
memory_budget=$((30 * 1024 * 1024))
export KUBECONFIG="$state/kubeconfig"

finish() {
  local status=$?
  if ((status != 0)) && [[ -s "$KUBECONFIG" ]]; then
    echo "--- failure context" >&2
    kubectl get pods,events -A -o wide >&2 || true
    kubectl -n "$namespace" logs deploy/zeabur --tail=200 >&2 || true
    kubectl -n zeabur-identity logs deploy/zeabur-identity --tail=200 >&2 || true
    for context in "$state"/results/*/error-context.md; do
      [[ -f "$context" ]] && cat "$context" >&2
    done
  fi
  [[ -n "${ZEABUR_L2_KEEP:-}" ]] || "$root/scripts/dev-cluster.sh" down
  exit "$status"
}
trap finish EXIT

"$root/scripts/dev-cluster.sh" up "$@"

# --- The browser ----------------------------------------------------------------------------------
# The e2e users, made from scratch, and web/e2e/l2.
"$root/scripts/e2e-l2-browser.sh"
# The restart the browser confirmed reached the cluster.
restarted=$(kubectl -n "$project" get deployment/talker \
  -o jsonpath='{.spec.template.metadata.annotations.kubectl\.kubernetes\.io/restartedAt}')
[[ -n "$restarted" ]] || { echo "error: deployment/talker was not restarted" >&2; exit 1; }

# --- Zeabur's and Zeabur Proxy's own rights: none beyond what any service account has, except -----
# --- Zeabur's pods in the terminal namespace --------------------------------------------------------
# kubectl adds no groups to --as; these are the ones a service account's token carries. A name
# no object has stands for a fresh service account, to which only the cluster's defaults apply.
groups=(--as-group=system:serviceaccounts "--as-group=system:serviceaccounts:$namespace"
  --as-group=system:authenticated)
as_fresh=("--as=system:serviceaccount:$namespace:zeabur-e2e-fresh" "${groups[@]}")
for account in zeabur zeabur-proxy; do
  as_account=("--as=system:serviceaccount:$namespace:$account" "${groups[@]}")
  impersonate=$(kubectl auth can-i impersonate users "${as_account[@]}" || true)
  if [[ "$impersonate" != no ]]; then
    echo "error: service account $account may impersonate users ($impersonate)" >&2
    exit 1
  fi
  for scope in "$namespace" default "$project" kube-system; do
    if ! diff <(kubectl auth can-i --list -n "$scope" "${as_account[@]}") \
      <(kubectl auth can-i --list -n "$scope" "${as_fresh[@]}"); then
      echo "error: service account $account has rights a fresh one lacks (namespace $scope)" >&2
      exit 1
    fi
  done
  echo "service account $account: no rights beyond the defaults"
done
as_zeabur=("--as=system:serviceaccount:$namespace:zeabur" "${groups[@]}")
for right in "create pods" "create pods/exec" "delete pods"; do
  if [[ "$(kubectl auth can-i "${right% *}" "${right#* }" -n zeabur-terminals "${as_zeabur[@]}" || true)" != yes ]]; then
    echo "error: Zeabur may not $right in zeabur-terminals" >&2
    exit 1
  fi
done
for right in "create secrets" "create rolebindings" "patch pods"; do
  if [[ "$(kubectl auth can-i "${right% *}" "${right#* }" -n zeabur-terminals "${as_zeabur[@]}" || true)" != no ]]; then
    echo "error: Zeabur may $right in zeabur-terminals" >&2
    exit 1
  fi
done
echo "service account zeabur: pods in zeabur-terminals only"

# --- Domains of the system namespaces (crates/zeabur-installer/manifests/system-hosts.yaml) -------
# A project cannot take Zeabur's or Zeabur Identity's domain, or every domain; it can use its own.
admit() { # $1 = kind, $2 = spec as JSON
  jq -n --arg ns "$project" --arg kind "$1" --argjson spec "$2" '{kind: $kind, spec: $spec,
    apiVersion: (if $kind == "Ingress" then "networking.k8s.io/v1" else "gateway.networking.k8s.io/v1" end),
    metadata: {name: "zeabur-e2e-host", namespace: $ns}}' | kubectl apply --dry-run=server -f - 2>&1
}
backend='{"paths": [{"path": "/x", "pathType": "Prefix", "backend": {"service": {"name": "talker", "port": {"number": 80}}}}]}'
refused=(
  "Ingress|{\"rules\": [{\"host\": \"zeabur.localhost\", \"http\": $backend}]}"
  "Ingress|{\"rules\": [{\"host\": \"identity.localhost\", \"http\": $backend}]}"
  "Ingress|{\"rules\": [{\"http\": $backend}]}"
  "Ingress|{\"rules\": [{\"host\": \"*.localhost\", \"http\": $backend}]}"
  "Ingress|{\"tls\": [{\"hosts\": [\"zeabur.localhost\"]}], \"rules\": [{\"host\": \"shop.localhost\", \"http\": $backend}]}"
)
if kubectl get crd httproutes.gateway.networking.k8s.io >/dev/null 2>&1; then
  refused+=("HTTPRoute|{\"rules\": [{}]}" "HTTPRoute|{\"hostnames\": [\"*.localhost\"], \"rules\": [{}]}")
fi
for case in "${refused[@]}"; do
  if out=$(admit "${case%%|*}" "${case#*|}"); then
    echo "error: the cluster accepted ${case#*|} in $project: $out" >&2
    exit 1
  fi
  [[ "$out" == *zeabur-system-hosts* ]] || { echo "error: unexpected refusal: $out" >&2; exit 1; }
done
out=$(admit Ingress "{\"rules\": [{\"host\": \"shop.localhost\", \"http\": $backend}]}") ||
  { echo "error: the cluster refused a project's own domain: $out" >&2; exit 1; }
echo "domains: the system namespaces keep theirs"

# --- Memory when idle -----------------------------------------------------------------------------
# The figure is the pod's working set as the kubelet reports it (Summary API, the one eviction
# uses). Right after the cluster comes up it moves by tens of MB while the pod settles, so the
# check waits a minute, then reads every 10 s for up to a minute more and passes on the first
# reading within budget; it fails on the lowest reading when none is.
pod=$(kubectl -n "$namespace" get pods -l app.kubernetes.io/name=zeabur -o jsonpath='{.items[0].metadata.name}')
node_name=$(kubectl get nodes -o jsonpath='{.items[0].metadata.name}')
working_set_now() {
  kubectl get --raw "/api/v1/nodes/$node_name/proxy/stats/summary" |
    jq --arg ns "$namespace" --arg pod "$pod" \
      '.pods[] | select(.podRef.namespace == $ns and .podRef.name == $pod) | .memory.workingSetBytes'
}
sleep 60
working_set=
for _ in 1 2 3 4 5 6 7; do
  reading=$(working_set_now)
  if [[ ! "$reading" =~ ^[0-9]+$ ]]; then
    echo "error: the kubelet reported no working set for pod $pod" >&2
    exit 1
  fi
  echo "idle working set: $reading bytes ($((reading / 1024)) KiB; budget $((memory_budget / 1024)) KiB)"
  if [[ -z "$working_set" ]] || ((reading < working_set)); then working_set=$reading; fi
  ((working_set <= memory_budget)) && break
  sleep 10
done
printf '{"pod_working_set_bytes": %s}\n' "$working_set" >"$state/measurements.json"
if ((working_set > memory_budget)); then
  echo "error: idle working set $working_set bytes exceeds 30 MiB in every reading" >&2
  exit 1
fi

# --- Removal leaves the cluster as it was -----------------------------------------------------------
[[ -z "${ZEABUR_L2_KEEP:-}" ]] || exit 0
# As `zeabur-installer uninstall` removes Zeabur, and the values only this cluster sets.
kubectl -n kube-system delete helmchart.helm.cattle.io zeabur --wait --timeout=180s
kubectl -n kube-system delete helmchartconfig.helm.cattle.io zeabur --wait
kubectl delete namespace "$namespace" --wait --timeout=180s
kubectl wait --for=delete namespace/zeabur-terminals --timeout=180s
kubectl delete rolebindings,clusterrolebindings -A -l app.kubernetes.io/managed-by=zeabur,zeabur.com/member --wait
# The chart keeps the S3 destinations' namespace on purpose; the project's name is the person's.
kubectl delete namespace zeabur-s3-destinations --wait --timeout=180s
kubectl annotate namespace "$project" zeabur.com/display-name-
# Zeabur Builder keeps a buildkitd, with its cache volume, for every namespace that built something.
# The project stays, so its builds' daemon goes here (full profile only).
if kubectl get namespace builder-system >/dev/null 2>&1; then
  kubectl -n builder-system delete statefulset "buildkitd-$project" --ignore-not-found --wait --timeout=180s
fi
# A volume's PersistentVolume goes after its claim, when the provisioner has cleaned the disk.
for _ in $(seq 36); do
  "$root/scripts/dev-cluster.sh" snapshot >"$state/after.txt"
  diff -q "$state/before.txt" "$state/after.txt" >/dev/null && break
  sleep 5
done
if ! diff -u "$state/before.txt" "$state/after.txt"; then
  kubectl get persistentvolumes -o custom-columns='PV:.metadata.name,CLAIM:.spec.claimRef.name,NAMESPACE:.spec.claimRef.namespace,CLASS:.spec.storageClassName,RECLAIM:.spec.persistentVolumeReclaimPolicy,PHASE:.status.phase' >&2 || true
  echo "error: the cluster differs from before the installation (above)" >&2
  exit 1
fi
echo "removal: the cluster is as it was"
