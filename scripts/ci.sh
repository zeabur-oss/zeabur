#!/usr/bin/env bash
# One CI job, as .github/workflows/ci.yml runs it (through zeabur-oss/.github's shared CI workflow,
# one job each) and as anyone runs it locally: the workflow only checks out, installs Nix and
# restores caches, then calls this script. Everything a job checks is here, so a job behaves the
# same on GitHub and on any Linux machine with Nix (and Docker for the cluster jobs); there is no
# runner image to emulate.
#
#   scripts/ci.sh <job>      # run one job; it enters its Nix dev shell itself
#   scripts/ci.sh --list     # the job names, without the dev shell
#
# A job leaves what is worth keeping (the unsafe report, the cluster's measurements) in
# $CI_ARTIFACTS when it is set; the workflow uploads it as the artifact ci-<job>.
#
# The cluster jobs run on the development cluster (scripts/dev-cluster.sh, which also reads
# ZEABUR_IDENTITY_SRC, K3S_IMAGE and ZEABUR_REGISTRY_MIRRORS; without ZEABUR_IDENTITY_SRC it fetches
# Zeabur Identity at ZEABUR_IDENTITY_PINNED itself) and delete it when they end; set
# ZEABUR_CI_KEEP=1 to keep it for a look afterwards.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

check_jobs=(format clippy docs test cargo-deny unsafe-report levels fuzz ui no-ui chart wording)
cluster_jobs=(kubernetes kubernetes-l2)

if [[ "${1:-}" == --list ]]; then
  printf '%s\n' "${check_jobs[@]}" "${cluster_jobs[@]}"
  exit 0
fi
job="${1:?usage: scripts/ci.sh <job> | --list}"
printf '%s\n' "${check_jobs[@]}" "${cluster_jobs[@]}" | grep -qx -- "$job" ||
  { echo "error: no job named $job (scripts/ci.sh --list)" >&2; exit 1; }

# Each job runs in its dev shell: wording in `.#zh` (zhtw-mcp builds from source), the rest in the
# default one. The marker tells a re-run inside the shell apart from a call outside it.
shell=default
[[ "$job" == wording ]] && shell=zh
if [[ "${ZEABUR_CI_SHELL:-}" != "$shell" ]]; then
  export ZEABUR_CI_SHELL="$shell"
  exec nix develop "$root#$shell" --command "$0" "$@"
fi

export RUSTDOCFLAGS="-D warnings"

# Copies a file into $CI_ARTIFACTS, which the workflow uploads; nothing without it or the file.
keep() { # $1 = file
  [[ -n "${CI_ARTIFACTS:-}" && -f "$1" ]] || return 0
  mkdir -p "$CI_ARTIFACTS"
  cp "$1" "$CI_ARTIFACTS/"
}

# The image's size and Zeabur's memory after kubernetes-l2, in the run's summary when there is one.
l2_report() {
  local measurements="target/dev-cluster/measurements.json"
  keep "$measurements"
  [[ -n "${GITHUB_STEP_SUMMARY:-}" ]] || return 0
  {
    echo "### Zeabur in the cluster (${K3S_IMAGE:-the default k3s})"
    echo "| Measure | Bytes |"
    echo "|---|---|"
    echo "| Image \`zeabur\` | $(docker image inspect --format '{{.Size}}' zeabur:dev 2>/dev/null || echo n/a) |"
    echo "| Pod working set after 60 s idle | $(jq -r .pod_working_set_bytes "$measurements" 2>/dev/null || echo n/a) |"
  } >>"$GITHUB_STEP_SUMMARY"
}

# As k3d's cluster admin, without Zeabur Identity: nothing critical and every check runs; the exit code is
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
    and status("zeabur-identity-installed") == "warning"
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

case "$job" in
  format)
    cargo fmt --all --check
    nixfmt --check flake.nix
    shellcheck scripts/*.sh
    actionlint
    (cd web; bun ci; bun run fmt:check)
    (cd crates/zeabur-installer/web; bun ci; bun run fmt:check)
    ;;
  clippy) cargo lint ;;
  docs) cargo doc --workspace --no-deps --document-private-items --locked ;;
  test) cargo nextest run --workspace --all-features --locked --no-tests=warn ;;
  cargo-deny) cargo deny --locked check ;;
  unsafe-report)
    status=0
    ./scripts/unsafe-report.sh || status=$?
    keep target/unsafe-report.md
    exit "$status"
    ;;
  levels) ./scripts/check-levels.sh ;;
  # Zeabur Proxy's request parser for a minute (crates/zeabur-proxy/fuzz). Without a sanitizer the
  # stable toolchain builds it; the target's own assertions are what it checks.
  fuzz)
    cd crates/zeabur-proxy/fuzz
    cargo fuzz run parse --sanitizer none -- -max_total_time=60
    ;;
  # The web interface: banned styles, lint, unused code and layer boundaries, new findings in the changed
  # files, build, unit tests, generated API types up to date, the installer's interface built and linted, no use of a
  # deprecated declaration in either, size budget, and a release build that embeds it. The health
  # and duplication report after it does not fail the job.
  ui)
    ./scripts/check-ui-styles.sh
    ./scripts/build-ui.sh
    (
      cd web
      bun run lint
      bun run fallow dead-code --baseline fallow-baseline.json
      # On a pull request, HEAD is GitHub's merge commit and HEAD^ is the base branch.
      bun run fallow audit --base "${FALLOW_AUDIT_BASE:-HEAD^}"
      bun run test
      bun run gen:api
    )
    git diff --exit-code -- web/src/api/schema.d.ts
    ./scripts/build-installer-ui.sh
    (cd crates/zeabur-installer/web; bun run lint)
    (cd web; bun scripts/check-deprecated.ts . ../crates/zeabur-installer/web)
    ./scripts/ui-budget.sh
    ZEABUR_UI_DIST="$root/web/dist" cargo build --release --locked -p zeabur -p zeaburctl
    ZEABUR_UI_DIST="$root/web/dist" cargo nextest run --release --locked -p zeabur-ui
    (cd web; bun run fallow health --report-only --top 10; bun run fallow dupes --summary)
    ;;
  # The API-only build needs no JavaScript toolchain: bun is taken off PATH.
  no-ui) ./scripts/build-without-ui.sh ;;
  # The Helm chart: lint, what every rendering promises, and the values it refuses.
  chart) ./scripts/check-chart.sh ;;
  # The interface's zh-TW wording (.zhtw-mcp.toml), and no file naming the project by an old name.
  wording)
    (cd web; bun ci)
    ./scripts/check-zh-tw.sh
    ./scripts/check-brand.sh
    ;;
  # Integration tests (JSON Patch writes, dry run, identity, revocation), `zeaburctl doctor`, and
  # browser tests in local mode with the embedded interface.
  kubernetes)
    if [[ -z "${ZEABUR_CI_KEEP:-}" ]]; then
      trap './scripts/dev-cluster.sh down' EXIT
    fi
    # Bare: the cluster tests' people are no admins, whom Zeabur's admission policy would stop.
    ./scripts/dev-cluster.sh up --profile bare
    export KUBECONFIG="$root/target/dev-cluster/kubeconfig"
    cargo nextest run --locked --run-ignored only -E 'binary(kubernetes)'
    check_doctor
    if ! { ./scripts/build-ui.sh && ./scripts/e2e-ui.sh; }; then
      browser_failure_context
      exit 1
    fi
    ;;
  # Zeabur in a cluster with Zeabur Identity, from their images and Zeabur's chart, through a browser:
  # passkey sign-in, terminal, rights, memory, removal.
  kubernetes-l2)
    status=0
    ZEABUR_L2_KEEP="${ZEABUR_CI_KEEP:-}" ./scripts/e2e-l2.sh || status=$?
    l2_report
    exit "$status"
    ;;
esac
