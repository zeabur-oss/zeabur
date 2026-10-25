#!/usr/bin/env bash
# A k3d cluster with Zeabur Identity and Zeabur, installed with zeabur-installer's own objects and
# values, at https://zeabur.localhost. For development and for the in-cluster tests (e2e-l2,
# the kubernetes CI jobs, the release smoke). Runs on Linux with Docker and Nix; it uses this
# machine's port 443 and enters `nix develop` by itself.
#
#   nix run .#dev-cluster -- up [options]
#   nix run .#dev-cluster -- down
#   scripts/dev-cluster.sh snapshot        # cluster-scoped objects and namespace metadata
#
# Options of `up`:
#   --profile full          also CloudNativePG with the Barman Cloud plugin, the Percona Operator
#                           for MongoDB, Keel, cert-manager, a test S3 and Zeabur Builder
#   --identity-src <dir>    build Zeabur Identity from this working tree, as it is
#   --builder-src <dir>     build Zeabur Builder from this working tree, as it is (full profile)
#   --zeabur-image <image>  run this Zeabur image (a local image is imported, any other is pulled)
#                           instead of one built from this checkout; Zeabur Proxy stays off
#   --zeabur-chart <chart>  install this chart (directory, .tgz or oci://) instead of
#                           deploy/chart/zeabur
#
# Without --identity-src, Zeabur Identity is built from the commit in ZEABUR_IDENTITY_PINNED,
# fetched from ZEABUR_IDENTITY_GIT (no image of that commit is published yet). Zeabur Builder
# likewise comes from ZEABUR_BUILDER_PINNED and ZEABUR_BUILDER_GIT. ZEABUR_REGISTRY_MIRRORS
# (`docker.io=http://mirror:5001,ghcr.io=...`) makes the node pull through those mirrors; K3S_IMAGE
# picks the k3s release. State is in target/dev-cluster (kubeconfig, CA, zeabur-identityctl).
# A second `up` on a running cluster brings it in line with the checkout.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
# The default dev shell sets ZEABUR_CA_BUNDLE.
if [[ -z "${ZEABUR_CA_BUNDLE:-}" ]]; then
  exec nix develop "$root" --command "$0" "$@"
fi

cluster=zeabur-dev
node="k3d-$cluster-server-0"
authn_volume="$cluster-authn"
state="$root/target/dev-cluster"
tls="$state/tls"
identity_host=identity.localhost
zeabur_host=zeabur.localhost
# The Builder's registry: a fixed ClusterIP in the static band of k3s's service range
# (10.43.0.0/16), so that the node's containerd reaches the in-cluster name through a mirror.
builder_registry=registry.builder-registry.svc.cluster.local:5000
builder_registry_ip=10.43.0.50
export KUBECONFIG="$state/kubeconfig"

usage() { sed -n '2,/^set -euo/{/^set -euo/d;s/^# \{0,1\}//;p}' "$0" >&2; exit 2; }

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

# Every cluster-scoped object by kind and name, and every namespace's labels and annotations.
# Sorted within each kind: not every list comes in a fixed order.
snapshot() {
  local resource
  for resource in $(kubectl api-resources --namespaced=false --verbs=list -o name 2>/dev/null | sort); do
    kubectl get "$resource" -o name 2>/dev/null | sort | sed "s|^|$resource |"
  done
  kubectl get namespaces -o json |
    jq -r '.items[] | [.metadata.name, (.metadata.labels // {} | tojson), (.metadata.annotations // {} | tojson)] | @tsv'
}

down() {
  k3d cluster delete "$cluster" >/dev/null 2>&1 || true
  docker volume rm -f "$authn_volume" >/dev/null 2>&1 || true
  if [[ -f "$state/images" ]]; then
    xargs -r docker image rm -f <"$state/images" >/dev/null 2>&1 || true
  fi
  rm -rf "$tls" "$state/kubeconfig" "$state/images" "$state/profile" "$state/before.txt" "$state/after.txt" \
    "$state/registries.yaml" "$state/install.yaml" "$state/identityctl"
  echo "cluster $cluster deleted"
}

# A checkout of `$2` (a commit) from `$3` (a Git URL) in `$1`, kept between runs for its build
# cache. A server that refuses to send a single commit gets a fetch of its branches.
checkout() { # $1 = directory, $2 = commit, $3 = URL
  [[ -d "$1/.git" ]] || git init -q "$1"
  if ! git -C "$1" cat-file -e "$2^{commit}" 2>/dev/null; then
    git -C "$1" fetch -q --depth 1 "$3" "$2" 2>/dev/null ||
      git -C "$1" fetch -q "$3" '+refs/heads/*:refs/remotes/origin/*'
  fi
  git -C "$1" -c advice.detachedHead=false checkout -q --force "$2"
  git -C "$1" clean -qfdx -e target
}

# Tags image `$1` by its content, so that a changed image gets a new tag and the cluster a new
# value; prints the tag.
content_tag() {
  local id tag
  id=$(docker image inspect --format '{{.Id}}' "$1")
  id=${id#sha256:}
  tag="${1%:*}:dev-${id:0:12}"
  docker tag "$1" "$tag"
  echo "$tag" >>"$state/images"
  echo "$tag"
}

# Imports local images into the node and fails unless the node has them: k3d's import of a
# multi-manifest index can fail on the node while reporting success. Reads the whole list before
# matching (`grep -q` under pipefail).
import() {
  local image name images
  k3d image import "$@" --cluster "$cluster" >/dev/null
  images=$(docker exec "$node" crictl images)
  for image in "$@"; do
    name="${image%:*}"
    [[ "$name" == */* ]] || name="library/$name"
    [[ "$name" == *.*/* ]] || name="docker.io/$name"
    if ! grep -q "$name *${image##*:}" <<<"$images"; then
      echo "error: the node does not have $image after the import" >&2
      exit 1
    fi
  done
}

# Runs the Helm commands Zeabur Catalog gives for a component (Cluster → Components) and waits for
# its deployments.
catalog_install() { # $1 = <capability>/<id>, $@ = namespaces to wait for
  local component="$1" work namespace
  shift
  work=$(mktemp -d)
  cargo run -q --locked -p zeabur-catalog --example install -- "$component" single-node >"$work/install.sh"
  (cd "$work" && bash -euo pipefail install.sh)
  rm -rf "$work"
  for namespace in "$@"; do
    kubectl -n "$namespace" wait deployment --all --for=condition=Available --timeout=300s
  done
}

up() {
  local profile=default identity_src="${ZEABUR_IDENTITY_SRC:-}" builder_src="${ZEABUR_BUILDER_SRC:-}"
  local zeabur_image="" chart="$root/deploy/chart/zeabur"
  while (($#)); do
    case "$1" in
      --profile) profile="${2:?--profile needs default or full}"; shift 2 ;;
      --identity-src) identity_src="${2:?}"; shift 2 ;;
      --builder-src) builder_src="${2:?}"; shift 2 ;;
      --zeabur-image) zeabur_image="${2:?}"; shift 2 ;;
      --zeabur-chart) chart="${2:?}"; shift 2 ;;
      *) usage ;;
    esac
  done
  [[ "$profile" == default || "$profile" == full ]] || usage
  for tool in docker k3d kubectl helm openssl jq git curl; do
    command -v "$tool" >/dev/null || { echo "error: $tool is not on PATH" >&2; exit 1; }
  done
  mkdir -p "$state"
  # No attestation manifests: see `import`.
  export BUILDX_NO_DEFAULT_ATTESTATIONS=1

  # --- Images -----------------------------------------------------------------------------------
  if [[ -z "$identity_src" ]]; then
    identity_src="$state/identity"
    checkout "$identity_src" "$(tr -d '[:space:]' <"$root/ZEABUR_IDENTITY_PINNED")" \
      "${ZEABUR_IDENTITY_GIT:-https://github.com/zeabur-oss/identity.git}"
  fi
  identity_src="$(cd "$identity_src" && pwd)"
  (cd "$identity_src" && nix develop "$identity_src" --command bash -c \
    './scripts/build-image.sh docker.io/library/zeabur-identity:dev &&
     cargo build --release --locked -p zeabur-identityctl')
  ln -sfn "$identity_src/target/release/zeabur-identityctl" "$state/identityctl"
  local identityctl="$state/identityctl" identity_image zeabur_import=() proxy_image=""
  identity_image=$(content_tag docker.io/library/zeabur-identity:dev)
  if [[ -z "$zeabur_image" ]]; then
    "$root/scripts/build-image.sh" docker.io/library/zeabur:dev
    "$root/scripts/build-image.sh" docker.io/library/zeabur-proxy:dev zeabur-proxy
    zeabur_image=$(content_tag docker.io/library/zeabur:dev)
    proxy_image=$(content_tag docker.io/library/zeabur-proxy:dev)
    zeabur_import=("$zeabur_image" "$proxy_image")
  elif docker image inspect "$zeabur_image" >/dev/null 2>&1; then
    zeabur_image=$(content_tag "$zeabur_image")
    zeabur_import=("$zeabur_image")
  fi
  docker pull -q busybox:1.37 >/dev/null
  # One plain image: Docker's containerd store keeps busybox as a multi-platform index.
  printf 'FROM busybox:1.37\nLABEL com.zeabur.test=busybox\n' |
    DOCKER_BUILDKIT=0 docker build -q -t busybox:1.37 - >/dev/null

  # --- Chart ------------------------------------------------------------------------------------
  if [[ "$chart" == oci://* ]]; then
    rm -rf "$state/chart"
    helm pull "$chart" --destination "$state/chart"
    chart=$(echo "$state"/chart/*.tgz)
  fi
  if [[ ! -e "$chart" ]]; then
    echo "error: no chart at $chart" >&2
    exit 1
  fi
  chart="$(cd "$(dirname "$chart")" && pwd)/$(basename "$chart")"

  # --- Cluster ----------------------------------------------------------------------------------
  # kube-apiserver re-reads its AuthenticationConfiguration by path; it starts with no issuer.
  # The file lives in a Docker volume, so this also works where the script runs in a container.
  if ! k3d cluster get "$cluster" >/dev/null 2>&1; then
    docker volume rm -f "$authn_volume" >/dev/null
    docker volume create "$authn_volume" >/dev/null
    printf 'apiVersion: apiserver.config.k8s.io/v1\nkind: AuthenticationConfiguration\nanonymous:\n  enabled: false\njwt: []\n' |
      docker run --rm -i -v "$authn_volume:/authn" busybox:1.37 sh -c 'cat >/authn/authn.yaml'
    {
      echo "mirrors:"
      printf '  "%s":\n    endpoint: ["http://%s:5000"]\n' "$builder_registry" "$builder_registry_ip"
      if [[ -n "${ZEABUR_REGISTRY_MIRRORS:-}" ]]; then
        IFS=, read -ra mirrors <<<"$ZEABUR_REGISTRY_MIRRORS"
        for mirror in "${mirrors[@]}"; do
          [[ "$mirror" == ?*=?* ]] || { echo "error: ZEABUR_REGISTRY_MIRRORS: want registry=endpoint, got $mirror" >&2; exit 1; }
          printf '  "%s":\n    endpoint: ["%s"]\n' "${mirror%%=*}" "${mirror#*=}"
        done
      fi
    } >"$state/registries.yaml"
    # No k3d load balancer: port 443 maps straight onto the server node.
    k3d cluster create "$cluster" --image "${K3S_IMAGE:-rancher/k3s:v1.36.5-k3s1}" --no-lb \
      --wait --timeout 300s --kubeconfig-update-default=false --kubeconfig-switch-context=false \
      --registry-config "$state/registries.yaml" \
      --volume "$authn_volume:/etc/rancher/k3s/zeabur-identity@server:0" \
      --k3s-arg "--kube-apiserver-arg=authentication-config=/etc/rancher/k3s/zeabur-identity/authn.yaml@server:0" \
      --port "443:443@server:0:direct"
  fi
  k3d kubeconfig get "$cluster" >"$KUBECONFIG"
  import "$identity_image" busybox:1.37 "${zeabur_import[@]}"

  retry 300 "Traefik to be installed" kubectl -n kube-system get deployment/traefik
  kubectl -n kube-system rollout status deployment/traefik --timeout=300s

  # Both host names resolve to Traefik in the cluster too, so Zeabur reaches Zeabur Identity at
  # its public URL (https://docs.k3s.io/networking/networking-services#coredns).
  kubectl apply -f - >/dev/null <<EOF
apiVersion: v1
kind: ConfigMap
metadata: {name: coredns-custom, namespace: kube-system}
data:
  zeabur-dev.override: |
    rewrite name exact $identity_host traefik.kube-system.svc.cluster.local answer auto
    rewrite name exact $zeabur_host traefik.kube-system.svc.cluster.local answer auto
EOF
  kubectl -n kube-system rollout restart deployment/coredns >/dev/null
  kubectl -n kube-system rollout status deployment/coredns --timeout=120s
  # A pod per try, read through its log: `kubectl run --rm -i` may fail to attach to a pod
  # that has already ended and then leaves it behind.
  resolves() {
    kubectl delete pod dns-check --ignore-not-found --wait >/dev/null
    kubectl run dns-check --restart=Never --image=busybox:1.37 --image-pull-policy=Never \
      -- nslookup "$identity_host" >/dev/null &&
      kubectl wait pod/dns-check --for=jsonpath='{.status.phase}'=Succeeded --timeout=60s >/dev/null &&
      kubectl logs dns-check | grep -q "^Name:.*$identity_host"
  }
  retry 120 "$identity_host to resolve in the cluster" resolves
  kubectl delete pod dns-check --wait >/dev/null

  # --- TLS: a CA of this cluster, one certificate for both host names ---------------------------
  if [[ ! -f "$tls/tls.crt" ]]; then
    mkdir -p "$tls"
    openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 30 \
      -subj "/CN=Zeabur dev CA" -keyout "$tls/ca.key" -out "$tls/ca.crt" \
      -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign" 2>/dev/null
    openssl req -new -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes \
      -subj "/CN=zeabur dev" -keyout "$tls/tls.key" -out "$tls/tls.csr" 2>/dev/null
    printf 'subjectAltName=DNS:%s,DNS:%s\nextendedKeyUsage=serverAuth\nbasicConstraints=critical,CA:FALSE\n' \
      "$identity_host" "$zeabur_host" >"$tls/ext.cnf"
    openssl x509 -req -in "$tls/tls.csr" -CA "$tls/ca.crt" -CAkey "$tls/ca.key" -days 30 \
      -set_serial "0x$(openssl rand -hex 16)" -extfile "$tls/ext.cnf" -out "$tls/tls.crt" 2>/dev/null
  fi
  apply_secret() { # $1 = namespace, $@ = kubectl create secret arguments
    local namespace="$1"
    shift
    kubectl -n "$namespace" create secret "$@" --dry-run=client -o yaml | kubectl apply -f - >/dev/null
  }

  # --- Full profile: the components Zeabur works with -------------------------------------------
  # Before Zeabur, so that e2e-l2's removal check counts them as the cluster's own.
  if [[ "$profile" == full ]]; then
    full_profile "$builder_src"
  fi

  # --- Zeabur Identity, as zeabur-installer installs it -----------------------------------------
  export ZEABUR_IDENTITY_ISSUER="https://$identity_host"
  "$identityctl" crds | kubectl apply --server-side -f - >/dev/null
  "$identityctl" manifests --image "$identity_image" --serve-arg=--invitations |
    kubectl apply --server-side --force-conflicts -f - >/dev/null
  kubectl -n zeabur-identity rollout status deployment/zeabur-identity --timeout=300s
  apply_secret zeabur-identity tls zeabur-identity-tls --cert="$tls/tls.crt" --key="$tls/tls.key"

  # kube-apiserver trusts Zeabur Identity once it has re-read the file (about once a minute).
  reloads() {
    kubectl get --raw /metrics | awk '/^apiserver_authentication_config_controller_automatic_reloads_total/ &&
      /status="success"/ { n += $NF } END { print n + 0 }'
  }
  local authn before
  authn=$("$identityctl" kube-config --distro k3s 2>/dev/null)
  if [[ "$authn" != "$(docker exec "$node" cat /etc/rancher/k3s/zeabur-identity/authn.yaml)" ]]; then
    before=$(reloads)
    docker exec -i "$node" sh -c 'cat >/etc/rancher/k3s/zeabur-identity/.authn.yaml.tmp &&
      mv /etc/rancher/k3s/zeabur-identity/.authn.yaml.tmp /etc/rancher/k3s/zeabur-identity/authn.yaml' <<<"$authn"
    reloaded() { (($(reloads) > before)); }
    retry 300 "kube-apiserver to load Zeabur Identity's configuration" reloaded
  fi

  # --- What the cluster looks like before Zeabur ------------------------------------------------
  # The projects the browser opens come first: their namespaces stay after Zeabur's removal.
  kubectl apply -f "$root/web/e2e/manifests/talker.yaml" \
    -f "$root/web/e2e/manifests/second-project.yaml" >/dev/null
  if ! kubectl get helmchart.helm.cattle.io -n kube-system zeabur >/dev/null 2>&1; then
    snapshot >"$state/before.txt"
  fi

  # --- Zeabur, from zeabur-installer's HelmChart ------------------------------------------------
  kubectl create namespace zeabur --dry-run=client -o yaml | kubectl apply -f - >/dev/null
  if ! kubectl -n zeabur-identity get oauthclients.identity.zeabur.com zeabur >/dev/null 2>&1 ||
    ! kubectl -n zeabur get secret zeabur-oidc >/dev/null 2>&1; then
    local secret
    "$identityctl" client create zeabur --type confidential --allow-elevation --audience kubernetes \
      --display-name Zeabur --home-url "https://$zeabur_host/" \
      --redirect-uri "https://$zeabur_host/auth/callback" 2>"$state/client.err" |
      kubectl apply -f - >/dev/null
    secret=$(tail -n 1 "$state/client.err")
    rm -f "$state/client.err"
    apply_secret zeabur generic zeabur-oidc --from-literal=client-secret="$secret"
  fi
  if ! kubectl -n zeabur get secret zeabur-plan-key >/dev/null 2>&1; then
    apply_secret zeabur generic zeabur-plan-key --from-literal=plan-key="$(openssl rand -hex 32)"
  fi
  apply_secret zeabur tls zeabur-tls --cert="$tls/tls.crt" --key="$tls/tls.key"
  kubectl -n zeabur create configmap zeabur-identity-ca --from-file=ca.crt="$tls/ca.crt" \
    --dry-run=client -o yaml | kubectl apply -f - >/dev/null

  cat >"$state/install.yaml" <<EOF
access: {mode: local, identity: $identity_host, zeabur: $zeabur_host}
sources:
  identityImage: $identity_image
  identityctl: $identityctl
  zeaburImage: $zeabur_image
  zeaburChart: $chart
EOF
  cargo build -q --locked -p zeabur-installer
  "$root/target/debug/zeabur-installer" --config "$state/install.yaml" manifests --cluster-name "$cluster" |
    kubectl apply --server-side --force-conflicts -f - >/dev/null
  # What only this cluster sets: Zeabur Proxy when its image is built here, and the Builder's
  # registry.
  local values="{}"
  if [[ -n "$proxy_image" ]]; then
    values=$(jq -c --arg r "${proxy_image%:*}" --arg t "${proxy_image##*:}" \
      '.proxy = {enabled: true, image: {repository: $r, tag: $t}}' <<<"$values")
  fi
  if [[ "$profile" == full ]]; then
    values=$(jq -c --arg r "$builder_registry" '.builds.registry = $r' <<<"$values")
  fi
  kubectl apply -f - >/dev/null <<EOF
apiVersion: helm.cattle.io/v1
kind: HelmChartConfig
metadata: {name: zeabur, namespace: kube-system}
spec:
  valuesContent: '$values'
EOF

  running() {
    [[ "$(kubectl -n zeabur get deployment zeabur -o jsonpath='{.spec.template.spec.containers[0].image}')" == "$zeabur_image" ]] &&
      kubectl -n zeabur rollout status deployment/zeabur --timeout=10s &&
      curl -sf --cacert "$tls/ca.crt" --resolve "$zeabur_host:443:127.0.0.1" "https://$zeabur_host/api/healthz"
  }
  if ! retry 600 "Zeabur to run $zeabur_image over HTTPS" running; then
    kubectl -n kube-system logs job/helm-install-zeabur --tail=50 >&2 || true
    kubectl -n zeabur get pods,events >&2 || true
    exit 1
  fi
  echo "$profile" >"$state/profile"

  echo
  echo "Zeabur:  https://$zeabur_host"
  if kubectl -n zeabur-identity get users.identity.zeabur.com admin >/dev/null 2>&1; then
    echo "Admin:   enrolled; a new setup link: $identityctl setup-link --username admin"
  else
    "$identityctl" setup-link --username admin --valid-for 24h
  fi
  echo "CA:      $tls/ca.crt (trust it in the browser, or accept the certificate)"
  echo "kubectl: export KUBECONFIG=$KUBECONFIG"
}

full_profile() { # $1 = Zeabur Builder working tree, or empty for the pinned commit
  local builder_src="$1"
  catalog_install certificates/cert-manager cert-manager
  # The ClusterIssuer local access names, signing with this cluster's CA.
  apply_secret cert-manager tls zeabur-local-ca --cert="$tls/ca.crt" --key="$tls/ca.key"
  kubectl apply -f - >/dev/null <<EOF
apiVersion: cert-manager.io/v1
kind: ClusterIssuer
metadata: {name: zeabur-local-ca}
spec:
  ca: {secretName: zeabur-local-ca}
EOF
  catalog_install databases/cloudnative-pg cnpg-system
  catalog_install databases/percona-mongodb psmdb-operator
  catalog_install image-updates/keel keel

  # The test S3 the database backup test writes to, with its bucket.
  kubectl apply -f "$root/web/e2e/manifests/s3.yaml" >/dev/null
  kubectl -n s3-test rollout status deployment/seaweedfs --timeout=180s
  bucket() {
    kubectl -n s3-test exec deploy/seaweedfs -- sh -c \
      'echo s3.bucket.create -name backups | weed shell -master=localhost:9333 -filer=localhost:8888' 2>/dev/null |
      grep -q 'created bucket\|already exists'
  }
  retry 60 "the test S3's bucket" bucket

  # --- Zeabur Builder, from its repository's deploy files ---------------------------------------
  if [[ -z "$builder_src" ]]; then
    builder_src="$state/builder"
    checkout "$builder_src" "$(tr -d '[:space:]' <"$root/ZEABUR_BUILDER_PINNED")" \
      "${ZEABUR_BUILDER_GIT:-https://github.com/zeabur-oss/builder.git}"
  fi
  builder_src="$(cd "$builder_src" && pwd)"
  (cd "$builder_src" && nix develop "$builder_src" --command ./scripts/build-images.sh zeabur-builder dev)
  local tag image images=()
  tag="dev-$(for image in builder builder-job builder-railpack; do
    docker image inspect --format '{{.Id}}' "zeabur-builder/$image:dev"
  done | sha256sum | cut -c1-12)"
  for image in builder builder-job builder-railpack; do
    docker tag "zeabur-builder/$image:dev" "zeabur-builder/$image:$tag"
    echo "zeabur-builder/$image:$tag" >>"$state/images"
    images+=("zeabur-builder/$image:$tag")
  done
  import "${images[@]}"
  local examples="$builder_src/deploy/examples"
  kubectl apply --server-side -f "$builder_src/deploy/crds.json" >/dev/null
  kubectl wait --for condition=Established --timeout 60s crd/builds.builder.zeabur.com >/dev/null
  for namespace in builder-system builder-registry; do
    kubectl create namespace "$namespace" --dry-run=client -o yaml | kubectl apply -f - >/dev/null
  done
  # Zot without accounts takes anonymous pushes, and a controller without
  # ZEABUR_BUILDER_REGISTRY_AUTH_PREFIX hands builds no credentials: any namespace can build.
  kubectl -n builder-registry create configmap zot --dry-run=client -o yaml --from-literal=config.json='{
    "storage": {"rootDirectory": "/var/lib/registry"},
    "http": {"address": "0.0.0.0", "port": "5000"}, "log": {"level": "info"}}' | kubectl apply -f - >/dev/null
  apply_secret builder-registry generic zot-htpasswd --from-literal=htpasswd=
  kubectl apply -f "$examples/registry.yaml" --dry-run=client -o json |
    jq --arg ip "$builder_registry_ip" '(.items[] | select(.kind == "Service") | .spec.clusterIP) = $ip' |
    kubectl apply -f - >/dev/null
  kubectl apply -f "$examples/buildkitd.yaml" >/dev/null
  sed -e "s|__IMAGE_PREFIX__|zeabur-builder|g" -e "s|__TAG__|$tag|g" -e "s|__HOST_USERS__|false|g" \
    -e "s|__IDLE__|600|g" -e '/ZEABUR_BUILDER_REGISTRY_AUTH_PREFIX/d' "$examples/controller.yaml" |
    kubectl apply -f - >/dev/null
  kubectl -n builder-system rollout status deployment/zeabur-builder --timeout=180s
  kubectl -n builder-registry rollout status deployment/registry --timeout=180s
}

case "${1:-}" in
  up) shift; up "$@" ;;
  down) down ;;
  snapshot) snapshot ;;
  *) usage ;;
esac
