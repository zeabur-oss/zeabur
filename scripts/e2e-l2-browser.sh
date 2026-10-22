#!/usr/bin/env bash
# The browser part of scripts/e2e-l2.sh, on the cluster it made (one kept with ZEABUR_L2_KEEP=1
# too): makes the e2e users again from scratch, then runs web/e2e/l2 with Playwright. Arguments go
# to Playwright:
#   scripts/e2e-l2-browser.sh e2e/l2/roles.spec.ts
#
# Every e2e user is deleted with the RBAC bindings Zeabur made for them (label zeabur.com/member)
# and made again the way the product makes users, so each has what that way gives today:
# `admin` from `zeabur-identityctl setup-link`, `e2e-admin` from `zeabur-identityctl invite`, and
# the members the roles test invites through Zeabur. Users of an older run would keep what the
# code of their day gave them.
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
users=(admin e2e-admin e2e-developer e2e-viewer)

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

# --- The projects ---------------------------------------------------------------------------------
kubectl apply -f "$root/web/e2e/manifests/talker.yaml" -f "$root/web/e2e/manifests/second-project.yaml" >/dev/null
kubectl -n "$first" rollout status deployment/talker --timeout=180s
kubectl -n "$second" rollout status deployment/talker --timeout=180s

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
  ZEABUR_L2_NAMESPACE="$first" ZEABUR_L2_SECOND_NAMESPACE="$second" ZEABUR_L2_SERVICE=talker \
  bunx playwright test -c playwright.l2.config.ts --reporter=line --output="$out/results" "$@"
