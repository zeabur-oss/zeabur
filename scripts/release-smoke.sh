#!/usr/bin/env bash
# Tries release artifacts as they will ship, never rebuilding them: the engawactl archive, and the
# chart installed on a disposable k3s cluster of its own. With an image (a local candidate from
# release-build.sh), the chart runs exactly that image, imported into the node; without one, the
# node pulls the image the chart names, as an installation would (release-verify.sh).
# Run inside `nix develop` on Linux amd64 or arm64 with Docker; it never touches the caller's
# kubeconfig.
#   scripts/release-smoke.sh dist/engawactl-v0.0.2-linux-arm64.tar.gz deploy/chart/engawa engawa:release-arm64
#   scripts/release-smoke.sh engawactl-v0.0.2-linux-arm64.tar.gz oci://ghcr.io/engawahq/charts/engawa@sha256:…
set -euo pipefail

if [[ $# != 2 && $# != 3 ]]; then
  echo "usage: release-smoke.sh <engawactl-archive> <chart> [<image>]" >&2
  exit 2
fi
archive="$1"
chart="$2"
image="${3:-}"
: "${IN_NIX_SHELL:?run inside nix develop}"
case "$(uname -s):$(uname -m)" in
  Linux:x86_64) arch=amd64 ;;
  Linux:aarch64) arch=arm64 ;;
  *) echo "release smoke runs on Linux amd64 or arm64 only" >&2; exit 2 ;;
esac
k3s_image="${K3S_IMAGE:-rancher/k3s:v1.36.5-k3s1}"
work="$(mktemp -d)"
cluster="engawa-release-${work##*.}"
cluster="${cluster,,}"
local_image="docker.io/library/engawa:smoke-${cluster#engawa-release-}"
namespace=engawa
cluster_started=false
image_tagged=false
forward_pid=
export KUBECONFIG="$work/kubeconfig"

cleanup() {
  local status=$?
  trap - EXIT
  if [[ -n "$forward_pid" ]]; then
    kill "$forward_pid" 2>/dev/null || true
    wait "$forward_pid" 2>/dev/null || true
  fi
  if [[ "$status" != 0 && -s "$KUBECONFIG" ]]; then
    kubectl --request-timeout=10s -n "$namespace" get pods -o wide >&2 || true
    kubectl --request-timeout=10s -n "$namespace" describe deployment engawa >&2 || true
    kubectl --request-timeout=10s -n "$namespace" logs deployment/engawa --tail=100 >&2 || true
    kubectl --request-timeout=10s -n "$namespace" get events --sort-by=.lastTimestamp >&2 || true
  fi
  [[ "$cluster_started" == true ]] && { k3d cluster delete "$cluster" >&2 || true; }
  [[ "$image_tagged" == true ]] && { docker image rm "$local_image" >/dev/null || true; }
  rm -rf "$work"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# The archive holds the binary and the licence, nothing else; the binary is streamed out rather
# than extracted, so no ownership, link or path in the archive is honoured.
listing="$(tar -tzf "$archive")"
if [[ "$listing" != $'LICENSE\nengawactl' ]]; then
  echo "expected an archive of LICENSE and engawactl, got: $listing" >&2
  exit 1
fi
tar -xOzf "$archive" engawactl >"$work/engawactl"
chmod 0755 "$work/engawactl"
version="$("$work/engawactl" --version)"
echo "$version"
version="${version#engawactl }"
expected="$(basename "$archive")"
expected="${expected#engawactl-v}"
expected="${expected%-linux-*}"
if [[ "$version" != "$expected" ]]; then
  echo "engawactl says version $version, the archive is $expected" >&2
  exit 1
fi

helm_args=()
if [[ -n "$image" ]]; then
  # Pin the candidate by its image ID: a new tag changes no bytes, and pullPolicy Never below
  # keeps any registry from replacing it.
  image_id="$(docker image inspect --format '{{.Id}}' "$image")"
  platform="$(docker image inspect --format '{{.Os}}/{{.Architecture}}' "$image_id")"
  if [[ "$platform" != "linux/$arch" ]]; then
    echo "image $image is $platform, this machine linux/$arch" >&2
    exit 1
  fi
  docker tag "$image_id" "$local_image"
  image_tagged=true
  # Only this platform: with Docker's containerd store an image can be a multi-platform index
  # whose other platforms are absent, and the node's import then fails while k3d reports success.
  docker image save --platform "linux/$arch" --output "$work/image.tar" "$local_image"
  helm_args+=(--set image.repository="${local_image%:*}" --set image.tag="${local_image##*:}"
    --set image.digest= --set image.pullPolicy=Never)
fi

cluster_started=true
k3d cluster create "$cluster" --image "$k3s_image" --no-lb --wait --timeout 300s \
  --kubeconfig-update-default=false --kubeconfig-switch-context=false \
  --k3s-arg '--disable=traefik@server:0' --k3s-arg '--disable=servicelb@server:0'
k3d kubeconfig get "$cluster" >"$KUBECONFIG"
if [[ -n "$image" ]]; then
  k3d image import "$work/image.tar" --cluster "$cluster"
  # The import's exit code is not proof: read what the node has. Capture first, then match
  # (`grep -q` under pipefail; see the engawa-ci skill).
  images="$(docker exec "k3d-$cluster-server-0" crictl images)"
  if ! grep -q "docker.io/library/engawa *${local_image##*:}" <<<"$images"; then
    echo "the node does not have $local_image after the import" >&2
    exit 1
  fi
fi

# The chart as people install it: its default values but the required ones, which name hosts
# that do not resolve. engawa starts without its issuer and asks again at sign-in.
kubectl create namespace "$namespace"
kubectl -n "$namespace" create secret generic engawa-oidc --from-literal=client-secret=release-smoke
helm install engawa "$chart" --namespace "$namespace" --wait --timeout 5m \
  --set publicUrl=https://engawa.release-smoke.invalid \
  --set issuer.url=https://genkan.release-smoke.invalid \
  --set issuer.clientSecret.existingSecret=engawa-oidc "${helm_args[@]}"
running="$(kubectl -n "$namespace" get pods -l app.kubernetes.io/name=engawa \
  -o jsonpath='{.items[0].status.containerStatuses[0].imageID}')"
echo "engawa runs $running"

# The kernel picks the local port, so concurrent runs do not collide. The log exists before the
# port-forward starts writing to it.
: >"$work/port-forward.log"
kubectl -n "$namespace" port-forward --address 127.0.0.1 service/engawa :80 >"$work/port-forward.log" 2>&1 &
forward_pid=$!
port=
for ((attempt = 0; attempt < 60; attempt++)); do
  kill -0 "$forward_pid" 2>/dev/null || { cat "$work/port-forward.log" >&2; exit 1; }
  log="$(cat "$work/port-forward.log")"
  if [[ "$log" =~ Forwarding\ from\ 127\.0\.0\.1:([0-9]+) ]]; then
    port="${BASH_REMATCH[1]}"
    break
  fi
  sleep 1
done
[[ -n "$port" ]] || { echo "port-forward did not start" >&2; exit 1; }
health="$(curl --fail --silent --show-error --max-time 10 "http://127.0.0.1:$port/api/healthz")"
[[ "$health" == ok ]] || { echo "/api/healthz answered '$health'" >&2; exit 1; }

# engawactl against the same cluster as its admin, without Genkan: a JSON report and an exit code
# of 0 (all ok) or 1 (warnings: Genkan is missing), as in scripts/ci.sh.
code=0
"$work/engawactl" doctor --output json >"$work/doctor.json" || code=$?
[[ $code -eq 0 || $code -eq 1 ]] || { cat "$work/doctor.json" >&2; echo "engawactl doctor exited $code" >&2; exit 1; }
jq -e '.checks | length > 0' "$work/doctor.json" >/dev/null

echo "smoke passed: $(basename "$archive"), chart $chart${image:+, image $image}"
