#!/usr/bin/env bash
# The browser part of scripts/e2e.sh, on the cluster `scripts/dev-cluster.sh up` made: makes the
# e2e users again from scratch, then runs web/e2e/full with Playwright. Arguments go to Playwright:
#   scripts/e2e-browser.sh e2e/full/roles.spec.ts
#
# Every e2e user is deleted with the RBAC bindings Zeabur made for them (label zeabur.com/member)
# and made again the way the product makes users, so each has what that way gives today:
# `admin` from `zeabur-identityctl setup-link`, `e2e-admin` and `e2e-member` from
# `zeabur-identityctl invite`, and the members the roles test invites through Zeabur. Users of an
# older run would keep what the code of their day gave them.
#
# On a cluster of `--profile full`, a spec that would skip for want of Zeabur Builder, CloudNativePG
# or the Percona Operator fails instead. Run inside `nix develop`; uses target/dev-cluster
# (kubeconfig, CA, zeabur-identityctl), never the default kubeconfig.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/target/dev-cluster"
source "$root/scripts/wait.sh"
tls="$out/tls"
zeabur_identityctl="$out/identityctl"
zeabur_host=zeabur.localhost
first=zeabur-e2e           # web/e2e/manifests/talker.yaml
second=zeabur-e2e-second   # web/e2e/manifests/second-project.yaml
users=(admin e2e-admin e2e-developer e2e-viewer e2e-member e2e-db-admin e2e-db-developer e2e-db-viewer
  e2e-git-admin e2e-git-developer e2e-git-private-admin e2e-git-private-developer
  e2e-image-admin e2e-image-developer e2e-usage-admin e2e-usage-viewer
  e2e-terminal-admin e2e-terminal-developer e2e-upload-admin e2e-upload-developer)

for file in "$out/kubeconfig" "$out/profile" "$tls/ca.crt" "$zeabur_identityctl"; do
  [[ -e "$file" ]] || { echo "error: $file is missing; run scripts/dev-cluster.sh up first" >&2; exit 1; }
done
export KUBECONFIG="$out/kubeconfig"
export ZEABUR_IDENTITY_ISSUER="https://identity.localhost"
full=""
[[ "$(cat "$out/profile")" == full ]] && full=1
# Whether the cluster has CRD `$1`; on the full profile, that it must.
has_crd() {
  kubectl get crd "$1" >/dev/null 2>&1 && return 0
  [[ -z "$full" ]] || { echo "error: the full profile has no CRD $1" >&2; exit 1; }
  return 1
}

# --- The e2e users, from scratch ------------------------------------------------------------------
# A User owns its credentials, sessions and links: a foreground deletion waits for them too.
selector="zeabur.com/member in ($(IFS=,; echo "${users[*]}"))"
kubectl delete rolebindings,clusterrolebindings -A -l "$selector" --wait >/dev/null
kubectl -n zeabur-identity delete users.identity.zeabur.com "${users[@]}" \
  --ignore-not-found --cascade=foreground --wait >/dev/null
setup_link=$("$zeabur_identityctl" setup-link --username admin)
admin_link=$("$zeabur_identityctl" invite --username e2e-admin --admin)
member_link=$("$zeabur_identityctl" invite --username e2e-member)
db_admin_link=$("$zeabur_identityctl" invite --username e2e-db-admin --admin)
image_admin_link=$("$zeabur_identityctl" invite --username e2e-image-admin --admin)
usage_admin_link=$("$zeabur_identityctl" invite --username e2e-usage-admin --admin)
terminal_admin_link=$("$zeabur_identityctl" invite --username e2e-terminal-admin --admin)
# Deploy from Git, when Zeabur Builder runs here (e2e/full/git.spec.ts skips otherwise), and from a
# private repository when E2E_PRIVATE_GIT_REPO names one (e2e/full/git-private.spec.ts), and from an
# uploaded folder (e2e/full/upload.spec.ts).
git_admin_link="" git_private_admin_link="" upload_admin_link=""
if has_crd builds.builder.zeabur.com; then
  git_admin_link=$("$zeabur_identityctl" invite --username e2e-git-admin --admin)
  upload_admin_link=$("$zeabur_identityctl" invite --username e2e-upload-admin --admin)
  if [[ -n "${E2E_PRIVATE_GIT_REPO:-}" ]]; then
    git_private_admin_link=$("$zeabur_identityctl" invite --username e2e-git-private-admin --admin)
  fi
fi

# --- The projects ---------------------------------------------------------------------------------
kubectl apply -f "$root/web/e2e/manifests/talker.yaml" -f "$root/web/e2e/manifests/second-project.yaml" >/dev/null
wait_for "$first" kubectl -n "$first" rollout status deployment/talker --timeout=180s
wait_for "$second" kubectl -n "$second" rollout status deployment/talker --timeout=180s

# --- Databases, when an operator and the test S3 run here (e2e/full/databases.spec.ts skips otherwise)
s3_endpoint="" mongodb=""
if has_crd clusters.postgresql.cnpg.io; then
  if kubectl -n s3-test get deployment seaweedfs >/dev/null 2>&1; then
    s3_endpoint=http://s3.s3-test.svc:8333
  elif [[ -n "$full" ]]; then
    echo "error: the full profile has no test S3 (namespace s3-test)" >&2
    exit 1
  fi
  has_crd perconaservermongodbs.psmdb.percona.com && mongodb=1
fi

# --- The browser ----------------------------------------------------------------------------------
# Chromium accepts the served certificates by their keys (ZEABUR_E2E_SPKI): cert-manager, on the
# full profile, issues its own. The test's own requests (page.request) go through Node, which
# takes the cluster's CA from NODE_EXTRA_CA_CERTS.
spki=$(for host in identity.localhost "$zeabur_host"; do
  openssl s_client -connect 127.0.0.1:443 -servername "$host" </dev/null 2>/dev/null |
    openssl x509 -pubkey -noout | openssl pkey -pubin -outform der | openssl dgst -sha256 -binary | base64
done | paste -sd,)
rm -rf "$out/results"
cd "$root/web"
NODE_EXTRA_CA_CERTS="$tls/ca.crt" \
  ZEABUR_E2E_URL="https://$zeabur_host" ZEABUR_E2E_SPKI="$spki" \
  ZEABUR_E2E_SETUP_LINK="$setup_link" ZEABUR_E2E_ADMIN_LINK="$admin_link" \
  ZEABUR_E2E_MEMBER_LINK="$member_link" ZEABUR_E2E_DB_ADMIN_LINK="$db_admin_link" \
  ZEABUR_E2E_IMAGE_ADMIN_LINK="$image_admin_link" ZEABUR_E2E_USAGE_ADMIN_LINK="$usage_admin_link" \
  ZEABUR_E2E_TERMINAL_ADMIN_LINK="$terminal_admin_link" \
  ZEABUR_E2E_GIT_ADMIN_LINK="$git_admin_link" ZEABUR_E2E_GIT_PRIVATE_ADMIN_LINK="$git_private_admin_link" \
  ZEABUR_E2E_UPLOAD_ADMIN_LINK="$upload_admin_link" \
  ZEABUR_E2E_S3_ENDPOINT="$s3_endpoint" ZEABUR_E2E_MONGODB="$mongodb" \
  ZEABUR_E2E_DB_KEEP="${ZEABUR_E2E_DB_KEEP:-}" \
  ZEABUR_E2E_NAMESPACE="$first" ZEABUR_E2E_SECOND_NAMESPACE="$second" ZEABUR_E2E_SERVICE=talker \
  bunx playwright test -c playwright.full.config.ts --reporter=line --output="$out/results" "$@"
