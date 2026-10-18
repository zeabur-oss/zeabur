#!/usr/bin/env bash
# Engawa in a cluster, end to end: a k3d cluster whose kube-apiserver trusts Genkan, Genkan and
# Engawa installed from their images (Engawa with its Helm chart), both behind the cluster's
# Traefik over HTTPS, and a browser (web/e2e/l2) that enrols a passkey, signs in, confirms a
# terminal with the passkey again, runs a command, then reads and changes with admin rights. Then,
# from the outside: Engawa's service account can do nothing a fresh one cannot, the idle pod
# stays within its memory budget, and `helm uninstall` leaves the cluster as it was.
#
# Run inside `nix develop` on Linux with Docker; it uses this machine's port 443. It creates the
# k3d cluster `engawa-l2` and deletes it at the end (ENGAWA_L2_KEEP=1 keeps it).
#   GENKAN_SRC=../genkan scripts/e2e-l2.sh
#
# Genkan is built from GENKAN_SRC (default: ../genkan next to this checkout) at the commit in
# GENKAN_PINNED; the script refuses a checkout whose HEAD is another commit. K3S_IMAGE picks the
# k3s release (default as in CI).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
genkan_src="$(cd "${GENKAN_SRC:-$root/../genkan}" && pwd)"
k3s_image="${K3S_IMAGE:-rancher/k3s:v1.36.5-k3s1}"
cluster=engawa-l2
node="k3d-$cluster-server-0"
authn_volume="$cluster-authn"
out="$root/target/e2e-l2"
genkan_host=genkan.localhost
engawa_host=engawa.localhost
namespace=engawa             # Engawa's release
project=engawa-e2e           # the project the browser opens (web/e2e/manifests/talker.yaml)
genkan_image=docker.io/library/genkan:e2e-l2
engawa_image=docker.io/library/engawa:e2e-l2
memory_budget=$((30 * 1024 * 1024))

for tool in docker k3d kubectl helm openssl jq git bun curl nix; do
  command -v "$tool" >/dev/null || { echo "error: $tool is not on PATH" >&2; exit 1; }
done
mkdir -p "$out"

# --- Genkan, as pinned --------------------------------------------------------------------------
pinned=$(tr -d '[:space:]' <"$root/GENKAN_PINNED")
head=$(git -C "$genkan_src" rev-parse HEAD)
if [[ "$head" != "$pinned" ]]; then
  echo "error: $genkan_src is at $head; GENKAN_PINNED wants $pinned" >&2
  echo "       check out that commit, or write the new one to GENKAN_PINNED" >&2
  exit 1
fi
# Built from the pinned commit's tree, not the working copy, with that commit's own dev shell.
# The build directory stays between runs so Cargo can reuse it.
genkan_build="$out/genkan"
mkdir -p "$genkan_build"
find "$genkan_build" -mindepth 1 -maxdepth 1 ! -name target -exec rm -rf {} +
git -C "$genkan_src" archive "$pinned" | tar -x -C "$genkan_build"
genkan_shell=(nix develop "git+file://$genkan_src?rev=$pinned" --command)

# --- Failure context and clean-up ---------------------------------------------------------------
cluster_up=""
finish() {
  local status=$?
  if ((status != 0)) && [[ -n "$cluster_up" ]]; then
    echo "--- failure context" >&2
    kubectl get pods,events -A -o wide >&2 || true
    kubectl -n "$namespace" logs deploy/engawa --tail=200 >&2 || true
    kubectl -n genkan-system logs deploy/genkan --tail=200 >&2 || true
    for context in "$out"/results/*/error-context.md; do
      [[ -f "$context" ]] && cat "$context" >&2
    done
  fi
  if [[ -z "${ENGAWA_L2_KEEP:-}" ]]; then
    k3d cluster delete "$cluster" >/dev/null 2>&1 || true
    docker volume rm -f "$authn_volume" >/dev/null 2>&1 || true
  fi
  exit "$status"
}
trap finish EXIT

# Polls `$@` every two seconds until it succeeds or `$1` seconds pass.
retry() { # $1 = seconds, $2 = what, $@ = command
  local deadline=$((SECONDS + $1)) what="$2"
  shift 2
  until "$@" >/dev/null 2>&1; do
    if ((SECONDS >= deadline)); then
      echo "error: timed out waiting for $what" >&2
      return 1
    fi
    sleep 2
  done
}

# --- Images -------------------------------------------------------------------------------------
# No attestation manifests: k3d's import of a multi-manifest index can fail on the node while
# reporting success (see the kubernetes job in .github/workflows/ci.yml).
export BUILDX_NO_DEFAULT_ATTESTATIONS=1
(cd "$genkan_build" && "${genkan_shell[@]}" bash -c \
  "./scripts/build-image.sh $genkan_image && cargo build --release --locked -p genkanctl")
genkanctl="$genkan_build/target/release/genkanctl"
"$root/scripts/build-image.sh" "$engawa_image"
docker pull busybox:1.37
printf 'FROM busybox:1.37\nLABEL dev.engawahq.engawa.test=busybox\n' |
  DOCKER_BUILDKIT=0 docker build -t busybox:1.37 -

# --- Cluster ------------------------------------------------------------------------------------
# kube-apiserver reads its AuthenticationConfiguration from a file it re-reads by path, starting
# with no issuer (as Genkan's CI does). The file lives in a Docker volume rather than a host
# directory so that this also works where the script itself runs in a container.
k3d cluster delete "$cluster" >/dev/null 2>&1 || true
docker volume rm -f "$authn_volume" >/dev/null
docker volume create "$authn_volume" >/dev/null
printf 'apiVersion: apiserver.config.k8s.io/v1\nkind: AuthenticationConfiguration\nanonymous:\n  enabled: false\njwt: []\n' |
  docker run --rm -i -v "$authn_volume:/authn" busybox:1.37 sh -c 'cat >/authn/authn.yaml'
# No k3d load balancer, so the port maps straight onto the server node (`direct`, see
# https://k3d.io/stable/usage/exposing_services/).
k3d cluster create "$cluster" --image "$k3s_image" --no-lb --wait --timeout 300s \
  --kubeconfig-update-default=false \
  --volume "$authn_volume:/etc/rancher/k3s/genkan@server:0" \
  --k3s-arg "--kube-apiserver-arg=authentication-config=/etc/rancher/k3s/genkan/authn.yaml@server:0" \
  --port "443:443@server:0:direct"
export KUBECONFIG="$out/kubeconfig"
cluster_up=1
k3d kubeconfig get "$cluster" >"$KUBECONFIG"
for image in "$genkan_image" "$engawa_image" busybox:1.37; do
  k3d image import "$image" --cluster "$cluster"
  name="${image%:*}"
  # Read the whole list before matching: `grep -q` exits at the first match, and under pipefail
  # the SIGPIPE that crictl then gets would fail the check.
  images=$(docker exec "$node" crictl images)
  if ! grep -q "${name#docker.io/} *${image##*:}" <<<"$images"; then
    echo "error: the node does not have $image after the import" >&2
    exit 1
  fi
done

# k3s installs Traefik (ports 80 and 443 on the node) from a bundled Helm chart, shortly after
# the node is ready.
retry 300 "Traefik to be installed" kubectl -n kube-system get deployment/traefik
kubectl -n kube-system rollout status deployment/traefik --timeout=300s

# Both host names resolve to Traefik inside the cluster too, so Engawa reaches Genkan at its
# public URL. k3s's CoreDNS imports `*.override` from the `coredns-custom` ConfigMap into its
# main server block (https://docs.k3s.io/networking/networking-services#coredns);
# `answer auto` names the answer as asked (https://coredns.io/plugins/rewrite/).
kubectl apply -f - <<EOF
apiVersion: v1
kind: ConfigMap
metadata: {name: coredns-custom, namespace: kube-system}
data:
  engawa-l2.override: |
    rewrite name exact $genkan_host traefik.kube-system.svc.cluster.local answer auto
    rewrite name exact $engawa_host traefik.kube-system.svc.cluster.local answer auto
EOF
kubectl -n kube-system rollout restart deployment/coredns
kubectl -n kube-system rollout status deployment/coredns --timeout=120s
# A pod per try, read through its log: `kubectl run --rm -i` may fail to attach to a pod that has
# already ended and then leaves it behind, so the next try would find the name taken.
resolves() {
  kubectl delete pod dns-check --ignore-not-found --wait >/dev/null
  kubectl run dns-check --restart=Never --image=busybox:1.37 --image-pull-policy=Never \
    -- nslookup "$genkan_host" >/dev/null &&
    kubectl wait pod/dns-check --for=jsonpath='{.status.phase}'=Succeeded --timeout=60s >/dev/null &&
    kubectl logs dns-check | grep -q "^Name:.*$genkan_host"
}
retry 120 "$genkan_host to resolve in the cluster" resolves
kubectl delete pod dns-check --wait >/dev/null

# --- TLS: a CA of this run, one certificate for both host names ----------------------------------
tls="$out/tls"
rm -rf "$tls"
mkdir -p "$tls"
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 2 \
  -subj "/CN=Engawa e2e CA" -keyout "$tls/ca.key" -out "$tls/ca.crt" \
  -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign" 2>/dev/null
openssl req -new -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes \
  -subj "/CN=Engawa e2e" -keyout "$tls/tls.key" -out "$tls/tls.csr" 2>/dev/null
printf 'subjectAltName=DNS:%s,DNS:%s\nextendedKeyUsage=serverAuth\nbasicConstraints=critical,CA:FALSE\n' \
  "$genkan_host" "$engawa_host" >"$tls/ext.cnf"
openssl x509 -req -in "$tls/tls.csr" -CA "$tls/ca.crt" -CAkey "$tls/ca.key" -days 2 \
  -set_serial "0x$(openssl rand -hex 16)" -extfile "$tls/ext.cnf" -out "$tls/tls.crt" 2>/dev/null
spki=$(openssl x509 -in "$tls/tls.crt" -pubkey -noout | openssl pkey -pubin -outform der |
  openssl dgst -sha256 -binary | base64)

# --- Genkan, installed as its get-started guide does ----------------------------------------------
export GENKAN_ISSUER="https://$genkan_host"
"$genkanctl" crds | kubectl apply --server-side -f -
"$genkanctl" manifests --image "$genkan_image" | kubectl apply --server-side -f -
kubectl -n genkan-system rollout status deployment/genkan --timeout=180s
kubectl -n genkan-system create secret tls genkan-tls --cert="$tls/tls.crt" --key="$tls/tls.key"
kubectl apply -f - <<EOF
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata: {name: genkan, namespace: genkan-system}
spec:
  ingressClassName: traefik
  tls: [{hosts: [$genkan_host], secretName: genkan-tls}]
  rules:
    - host: $genkan_host
      http:
        paths:
          - {path: /, pathType: Prefix, backend: {service: {name: genkan, port: {number: 80}}}}
EOF
genkan_up() {
  curl -sf --cacert "$tls/ca.crt" --resolve "$genkan_host:443:127.0.0.1" \
    "https://$genkan_host/.well-known/openid-configuration" | jq -e --arg i "$GENKAN_ISSUER" '.issuer == $i'
}
retry 120 "Genkan's discovery document over HTTPS" genkan_up

# kube-apiserver trusts Genkan once it has re-read its configuration (about once a minute):
# count its successful reloads before and after.
reloads() {
  kubectl get --raw /metrics | awk '/^apiserver_authentication_config_controller_automatic_reloads_total/ &&
    /status="success"/ { n += $NF } END { print n + 0 }'
}
before=$(reloads)
# (It also prints how to install the file on a k3s server; the next line does that here.)
"$genkanctl" kube-config --distro k3s >"$out/authn.yaml"
docker exec -i "$node" sh -c 'cat >/etc/rancher/k3s/genkan/.authn.yaml.tmp &&
  mv /etc/rancher/k3s/genkan/.authn.yaml.tmp /etc/rancher/k3s/genkan/authn.yaml' <"$out/authn.yaml"
reloaded() { (($(reloads) > before)); }
retry 180 "kube-apiserver to load Genkan's configuration" reloaded

# Engawa's client: confidential, may ask for elevation, gets tokens kube-apiserver accepts.
"$genkanctl" client create engawa --type confidential --allow-elevation --audience kubernetes \
  --redirect-uri "https://$engawa_host/auth/callback" 2>"$out/client.err" | kubectl apply -f -
client_secret=$(tail -n 1 "$out/client.err")
rm -f "$out/client.err"
setup_link=$("$genkanctl" setup-link --username admin)

# The project the browser opens.
kubectl apply -f "$root/web/e2e/manifests/talker.yaml" >/dev/null
kubectl -n "$project" rollout status deployment/talker --timeout=180s

# --- What the cluster looks like before Engawa ----------------------------------------------------
# Every cluster-scoped object by kind and name, and every namespace's labels and annotations.
# Sorted within each kind: not every list comes in a fixed order (componentstatuses have come back
# in another order between two reads).
snapshot() {
  local resource
  for resource in $(kubectl api-resources --namespaced=false --verbs=list -o name 2>/dev/null | sort); do
    kubectl get "$resource" -o name 2>/dev/null | sort | sed "s|^|$resource |"
  done
  kubectl get namespaces -o json |
    jq -r '.items[] | [.metadata.name, (.metadata.labels // {} | tojson), (.metadata.annotations // {} | tojson)] | @tsv'
}
snapshot >"$out/before.txt"

# --- Engawa, from the chart ---------------------------------------------------------------------
kubectl create namespace "$namespace"
printf %s "$client_secret" |
  kubectl -n "$namespace" create secret generic engawa-oidc --from-file=client-secret=/dev/stdin
unset client_secret
kubectl -n "$namespace" create secret tls engawa-tls --cert="$tls/tls.crt" --key="$tls/tls.key"
kubectl -n "$namespace" create configmap genkan-ca --from-file=ca.crt="$tls/ca.crt"
helm install engawa "$root/deploy/chart/engawa" --namespace "$namespace" --wait --timeout 3m \
  --set image.repository="${engawa_image%:*}" --set image.tag="${engawa_image##*:}" \
  --set publicUrl="https://$engawa_host" \
  --set issuer.url="$GENKAN_ISSUER" \
  --set issuer.clientSecret.existingSecret=engawa-oidc \
  --set issuer.caBundle.configMap=genkan-ca \
  --set ingress.enabled=true --set ingress.className=traefik \
  --set ingress.tls.secretName=engawa-tls
engawa_up() {
  curl -sf --cacert "$tls/ca.crt" --resolve "$engawa_host:443:127.0.0.1" "https://$engawa_host/api/healthz"
}
retry 120 "Engawa over HTTPS" engawa_up

# --- The browser ----------------------------------------------------------------------------------
rm -rf "$out/results"
# Chromium accepts the run's certificate by its key (ENGAWA_L2_SPKI); the test's own requests
# (page.request) go through Node, which takes the run's CA from NODE_EXTRA_CA_CERTS.
(cd "$root/web" && NODE_EXTRA_CA_CERTS="$tls/ca.crt" \
  ENGAWA_L2_URL="https://$engawa_host" ENGAWA_L2_SPKI="$spki" \
  ENGAWA_L2_SETUP_LINK="$setup_link" ENGAWA_L2_NAMESPACE="$project" ENGAWA_L2_SERVICE=talker \
  bunx playwright test -c playwright.l2.config.ts --reporter=line --output="$out/results")
# The restart the browser confirmed reached the cluster.
restarted=$(kubectl -n "$project" get deployment/talker \
  -o jsonpath='{.spec.template.metadata.annotations.kubectl\.kubernetes\.io/restartedAt}')
[[ -n "$restarted" ]] || { echo "error: deployment/talker was not restarted" >&2; exit 1; }

# --- Engawa's own rights: none beyond what any service account has -----------------------------------
# kubectl adds no groups to --as; these are the ones a service account's token carries. A name
# no object has stands for a fresh service account, to which only the cluster's defaults apply.
groups=(--as-group=system:serviceaccounts "--as-group=system:serviceaccounts:$namespace"
  --as-group=system:authenticated)
as_engawa=("--as=system:serviceaccount:$namespace:engawa" "${groups[@]}")
as_fresh=("--as=system:serviceaccount:$namespace:engawa-e2e-fresh" "${groups[@]}")
impersonate=$(kubectl auth can-i impersonate users "${as_engawa[@]}" || true)
if [[ "$impersonate" != no ]]; then
  echo "error: Engawa's service account may impersonate users ($impersonate)" >&2
  exit 1
fi
for scope in "$namespace" default "$project" kube-system; do
  if ! diff <(kubectl auth can-i --list -n "$scope" "${as_engawa[@]}") \
    <(kubectl auth can-i --list -n "$scope" "${as_fresh[@]}"); then
    echo "error: Engawa's service account has rights a fresh one lacks (namespace $scope)" >&2
    exit 1
  fi
done
echo "service account: no rights beyond the defaults"

# --- Memory when idle -----------------------------------------------------------------------------
# A fixed wait on purpose: the measure is the pod's working set after a minute of nothing, as
# the kubelet reports it (Summary API, the figure eviction uses).
pod=$(kubectl -n "$namespace" get pods -l app.kubernetes.io/name=engawa -o jsonpath='{.items[0].metadata.name}')
node_name=$(kubectl get nodes -o jsonpath='{.items[0].metadata.name}')
sleep 60
working_set=$(kubectl get --raw "/api/v1/nodes/$node_name/proxy/stats/summary" |
  jq --arg ns "$namespace" --arg pod "$pod" \
    '.pods[] | select(.podRef.namespace == $ns and .podRef.name == $pod) | .memory.workingSetBytes')
if [[ ! "$working_set" =~ ^[0-9]+$ ]]; then
  echo "error: the kubelet reported no working set for pod $pod" >&2
  exit 1
fi
printf '{"pod_working_set_bytes": %s}\n' "$working_set" >"$out/measurements.json"
echo "idle working set: $working_set bytes ($((working_set / 1024)) KiB; budget $((memory_budget / 1024)) KiB)"
if ((working_set > memory_budget)); then
  echo "error: idle working set $working_set bytes exceeds 30 MiB" >&2
  exit 1
fi

# --- Removal leaves the cluster as it was -----------------------------------------------------------
helm uninstall engawa --namespace "$namespace" --wait --timeout 3m
kubectl delete namespace "$namespace" --wait --timeout=180s
snapshot >"$out/after.txt"
if ! diff -u "$out/before.txt" "$out/after.txt"; then
  echo "error: the cluster differs from before the installation (above)" >&2
  exit 1
fi
echo "removal: the cluster is as it was"
