#!/usr/bin/env bash
# Integration tests (JSON Patch writes, dry run, identity, revocation), `zeaburctl doctor`, and
# browser tests in local mode with the embedded interface, on the development cluster
# (scripts/dev-cluster.sh, which also reads ZEABUR_IDENTITY_SRC, K3S_IMAGE and
# ZEABUR_REGISTRY_MIRRORS; without ZEABUR_IDENTITY_SRC it installs the Zeabur Identity release that
# ZEABUR_IDENTITY_PINNED names). The cluster is deleted at the end; ZEABUR_CI_KEEP=1 keeps it.
set -euo pipefail
cd "$(dirname "$0")/../.."

# As k3d's cluster admin, with no Zeabur dashboard and no Zeabur Identity: nothing critical, the Identity
# checks skipped; the exit code is
# the most serious status in the JSON (crates/zeaburctl/README.md).
check_doctor() {
  cargo build --locked -p zeaburctl --no-default-features
  local code=0 expected
  ./target/debug/zeaburctl doctor --output json >target/doctor.json || code=$?
  cat target/doctor.json
  [[ $code -eq 0 || $code -eq 1 ]] || { echo "unexpected exit code $code"; return 1; }
  jq -e '
    def status(name): .checks[] | select(.name == name) | .status;
    (.checks | map(.name)) == ["cluster-version", "components", "cluster-gaps",
      "zeabur-identity-installed", "zeabur-identity-client", "zeabur-identity-user", "zeabur-identity-issuer-trust"]
    and all(.checks[]; (.status | IN("ok", "warning", "critical", "skipped", "error"))
      and (.findings | type == "array") and (.fix == null or (.fix | type == "string")))
    and (status("components") | IN("ok", "warning"))
    and status("zeabur-identity-installed") == "skipped"
    and status("zeabur-identity-client") == "skipped"
    and status("zeabur-identity-user") == "skipped"
  ' target/doctor.json
  expected=$(jq '.checks | map(.status)
    | if any(. == "critical") then 2 elif any(. == "warning") then 1
      elif any(. == "error") then 3 else 0 end' target/doctor.json)
  [[ $code -eq $expected ]] || { echo "exit code $code, expected $expected"; return 1; }
}

# A failed browser test's files stay on the machine (or need a download on GitHub): print what
# explains it into the log, each test's page snapshot and the pods it looked at.
browser_failure_context() {
  local context
  for context in target/e2e-ui/results/*/error-context.md; do
    [[ -f "$context" ]] || continue
    echo "::group::$context"
    cat "$context"
    echo "::endgroup::"
  done
  kubectl get pods,events -A -o wide || true
}

if [[ -z "${ZEABUR_CI_KEEP:-}" ]]; then
  trap './scripts/dev-cluster.sh down' EXIT
fi
# Bare: the cluster tests' people are no admins, whom Zeabur's admission policy would stop.
./scripts/dev-cluster.sh up --profile bare
export KUBECONFIG="$PWD/target/dev-cluster/kubeconfig"
cargo nextest run --locked --run-ignored only -E 'binary(kubernetes)'
check_doctor
if ! { ./scripts/build-ui.sh && ./scripts/e2e-ui.sh; }; then
  browser_failure_context
  exit 1
fi
