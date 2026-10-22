#!/usr/bin/env bash
# The browser part of scripts/e2e-l2.sh, on the cluster it made (one kept with ZEABUR_L2_KEEP=1
# too): makes the e2e users again from scratch, then runs web/e2e/l2 with Playwright. Arguments go
# to Playwright:
#   scripts/e2e-l2-browser.sh e2e/l2/roles.spec.ts
#
# Every e2e user is deleted with the RBAC bindings Zeabur made for them (label zeabur.com/member)
# and made again the way the product makes users, so each has what that way gives today:
# `admin` from `zeabur-identityctl setup-link`, `e2e-admin` and `e2e-member` from
# `zeabur-identityctl invite`, and the members the roles test invites through Zeabur. Users of an
# older run would keep what the code of their day gave them.
#
# Run inside `nix develop`; uses target/e2e-l2 (kubeconfig, TLS, zeabur-identityctl), never the
# default kubeconfig.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/target/e2e-l2"
tls="$out/tls"
zeabur_identityctl="$out/zeabur-identity/target/release/zeabur-identityctl"
zeabur_host=zeabur.localhost
first=zeabur-e2e           # web/e2e/manifests/talker.yaml
second=zeabur-e2e-second   # web/e2e/manifests/second-project.yaml
users=(admin e2e-admin e2e-developer e2e-viewer e2e-member e2e-db-admin e2e-db-developer e2e-db-viewer)

for file in "$out/kubeconfig" "$tls/ca.crt" "$tls/tls.crt" "$zeabur_identityctl"; do
  [[ -e "$file" ]] || { echo "error: $file is missing; run scripts/e2e-l2.sh first" >&2; exit 1; }
done
export KUBECONFIG="$out/kubeconfig"
export ZEABUR_IDENTITY_ISSUER="https://identity.localhost"

# --- The e2e users, from scratch ------------------------------------------------------------------
# A User owns its credentials, sessions and links: a foreground deletion waits for them too.
selector="zeabur.com/member in ($(IFS=,; echo "${users[*]}"))"
kubectl delete rolebindings,clusterrolebindings -A -l "$selector" --wait >/dev/null
kubectl -n zeabur-identity delete users.identity.zeabur.com "${users[@]}" \
  --ignore-not-found --cascade=foreground --wait >/dev/null
setup_link=$("$zeabur_identityctl" setup-link --username admin)
admin_link=$("$zeabur_identityctl" invite --username e2e-admin --group admins \
  --elevated-group admins)
member_link=$("$zeabur_identityctl" invite --username e2e-member)
db_admin_link=$("$zeabur_identityctl" invite --username e2e-db-admin --group admins \
  --elevated-group admins)

# --- The projects ---------------------------------------------------------------------------------
kubectl apply -f "$root/web/e2e/manifests/talker.yaml" -f "$root/web/e2e/manifests/second-project.yaml" >/dev/null
kubectl -n "$first" rollout status deployment/talker --timeout=180s
kubectl -n "$second" rollout status deployment/talker --timeout=180s

# --- Databases, when an operator runs here (e2e/l2/databases.spec.ts skips otherwise) ------------
s3_endpoint="" mongodb=""
if kubectl get crd clusters.postgresql.cnpg.io >/dev/null 2>&1; then
  kubectl apply -f "$root/web/e2e/manifests/s3.yaml" >/dev/null
  kubectl -n s3-test rollout status deployment/seaweedfs --timeout=180s
  for _ in $(seq 1 30); do
    kubectl -n s3-test exec deploy/seaweedfs -- sh -c \
      'echo s3.bucket.create -name backups | weed shell -master=localhost:9333 -filer=localhost:8888' \
      2>/dev/null | grep -q 'created bucket\|already exists' && break
    sleep 2
  done
  s3_endpoint=http://s3.s3-test.svc:8333
  kubectl get crd perconaservermongodbs.psmdb.percona.com >/dev/null 2>&1 && mongodb=1
fi

# --- The browser ----------------------------------------------------------------------------------
# Chromium accepts the run's certificate by its key (ZEABUR_L2_SPKI); the test's own requests
# (page.request) go through Node, which takes the run's CA from NODE_EXTRA_CA_CERTS.
spki=$(openssl x509 -in "$tls/tls.crt" -pubkey -noout | openssl pkey -pubin -outform der |
  openssl dgst -sha256 -binary | base64)
rm -rf "$out/results"
cd "$root/web"
NODE_EXTRA_CA_CERTS="$tls/ca.crt" \
  ZEABUR_L2_URL="https://$zeabur_host" ZEABUR_L2_SPKI="$spki" \
  ZEABUR_L2_SETUP_LINK="$setup_link" ZEABUR_L2_ADMIN_LINK="$admin_link" \
  ZEABUR_L2_MEMBER_LINK="$member_link" ZEABUR_L2_DB_ADMIN_LINK="$db_admin_link" \
  ZEABUR_L2_S3_ENDPOINT="$s3_endpoint" ZEABUR_L2_MONGODB="$mongodb" \
  ZEABUR_L2_DB_KEEP="${ZEABUR_L2_DB_KEEP:-}" \
  ZEABUR_L2_NAMESPACE="$first" ZEABUR_L2_SECOND_NAMESPACE="$second" ZEABUR_L2_SERVICE=talker \
  bunx playwright test -c playwright.l2.config.ts --reporter=line --output="$out/results" "$@"
