#!/usr/bin/env bash
# Switches the profile of one K3s install the way a server's owner would: install single-node,
# write into zot's volume and into a local-path volume, apply multi-node, apply single-node again,
# and check that the data and the claims are still there. `doctor` runs after each apply.
# Multi-node on one host runs Longhorn without its other replicas, which is enough here.
#
# Runs zeabur-installer as root on this host and leaves K3s installed: run it only on a fresh
# runner or a disposable VM, with no Docker (see .github/workflows/profile-switch.yml), inside
# `nix develop`.
set -euo pipefail
cd "$(dirname "$0")/../.."
: "${ZEABUR_CATALOG_RELEASE:?run inside nix develop}"

kubectl() {
  sudo /var/lib/rancher/k3s/data/current/bin/kubectl --kubeconfig=/etc/rancher/k3s/k3s.yaml "$@"
}
# shellcheck source=scripts/wait.sh
source scripts/wait.sh

work="$PWD/target/profile-switch"
mkdir -p "$work"
cargo build --locked -p zeabur-installer
installer=(sudo "$PWD/target/debug/zeabur-installer" --no-telemetry --config "$work/config.yaml")

failure_context() {
  local status=$?
  trap - EXIT
  if [[ $status -ne 0 ]]; then
    echo "::group::cluster state"
    kubectl get nodes,pods,pvc -A -o wide || true
    kubectl get helmcharts -n kube-system || true
    kubectl -n zot get statefulsets,pvc,secrets -o wide || true
    kubectl -n kube-system logs job/helm-install-zot --tail=40 || true
    kubectl get events -A --sort-by=.lastTimestamp | tail -60 || true
    echo "::endgroup::"
  fi
  exit "$status"
}
trap failure_context EXIT

# Longhorn stays enabled on the way back, so the switch does not also remove a component.
configure() {
  cat >"$work/config.yaml" <<EOF
profile: $1
enable: [builds, replicated-storage]
zeabur: false
telemetry: false
access: {mode: local, identity: identity.profile-switch.test, zeabur: zeabur.profile-switch.test}
sources:
  catalog: $ZEABUR_CATALOG_RELEASE/index.json
EOF
}

# doctor exits 1 for a warning, such as Longhorn volumes with fewer replicas than wanted.
doctor() {
  local code=0
  sudo "$PWD/target/debug/zeabur-installer" doctor || code=$?
  [[ $code -le 1 ]] || { echo "error: doctor exited $code" >&2; return 1; }
}

apply() {
  echo "::group::apply $1"
  configure "$1"
  "${installer[@]}" plan --json >"$work/plan-$1.json"
  "${installer[@]}" apply
  echo "::endgroup::"
  doctor
}

# Runs `sh -ec $3` in busybox with claim $2 of namespace $1 at /data until it exits; prints its
# output.
on_claim() {
  local namespace=$1 claim=$2 script=$3 pod
  pod="profile-switch-$RANDOM"
  jq -n --arg pod "$pod" --arg claim "$claim" --arg script "$script" '{
    apiVersion: "v1", kind: "Pod", metadata: {name: $pod},
    spec: {
      restartPolicy: "Never",
      securityContext: {runAsNonRoot: true, runAsUser: 65534, seccompProfile: {type: "RuntimeDefault"}},
      containers: [{
        name: "data", image: "docker.io/library/busybox:1.37.0", command: ["sh", "-ec", $script],
        securityContext: {allowPrivilegeEscalation: false, capabilities: {drop: ["ALL"]}},
        volumeMounts: [{name: "data", mountPath: "/data"}]
      }],
      volumes: [{name: "data", persistentVolumeClaim: {claimName: $claim}}]
    }
  }' | kubectl -n "$namespace" apply -f - >/dev/null
  wait_for "$namespace" kubectl -n "$namespace" wait "pod/$pod" \
    --for=jsonpath='{.status.phase}'=Succeeded --timeout=5m
  kubectl -n "$namespace" logs "$pod"
  kubectl -n "$namespace" delete pod "$pod" --wait=false >/dev/null
}

claim_of() {
  kubectl -n "$1" get pvc "$2" -o jsonpath='{.metadata.uid} {.spec.storageClassName} {.status.capacity.storage}'
}

apply single-node

zot_claim=$(kubectl -n zot get statefulset zot \
  -o jsonpath='{.spec.volumeClaimTemplates[0].metadata.name}')-zot-0
marker="written before the profile switch $(date -u +%s)"
on_claim zot "$zot_claim" "echo \"$marker\" > /data/profile-switch-marker"
kubectl create namespace profile-switch
kubectl -n profile-switch apply -f - <<EOF
apiVersion: v1
kind: PersistentVolumeClaim
metadata: {name: data}
spec:
  storageClassName: local-path
  accessModes: [ReadWriteOnce]
  resources: {requests: {storage: 1Gi}}
EOF
on_claim profile-switch data "echo \"$marker\" > /data/marker"
zot_before=$(claim_of zot "$zot_claim")
data_before=$(claim_of profile-switch data)

apply multi-node
jq -e '.warnings | any(.code == "statefulset.mayRecreate")' "$work/plan-multi-node.json" >/dev/null ||
  { echo "error: plan did not say that zot's StatefulSet may be recreated" >&2; exit 1; }
apply single-node

check() {
  [[ "$2" == "$3" ]] || { echo "error: $1: expected '$3', got '$2'" >&2; exit 1; }
}
check "zot's release" "$(kubectl -n zot get secrets -l owner=helm,name=zot -o json |
  jq -r '.items | max_by(.metadata.labels.version | tonumber) | .metadata.labels.status')" deployed
check "zot's claim" "$(claim_of zot "$zot_claim")" "$zot_before"
check "the local-path claim" "$(claim_of profile-switch data)" "$data_before"
check "zot's volume" "$(on_claim zot "$zot_claim" 'cat /data/profile-switch-marker')" "$marker"
check "the local-path volume" "$(on_claim profile-switch data 'cat /data/marker')" "$marker"
wait_for zot kubectl -n zot rollout status statefulset/zot --timeout=5m
echo "profile switch: ok"
