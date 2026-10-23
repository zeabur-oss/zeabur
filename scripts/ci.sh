#!/usr/bin/env bash
# One CI job, as .github/workflows/ci.yml runs it and as anyone runs it locally: the workflow only
# checks out, installs Nix and restores caches, then calls this script. Everything a job checks is
# here, so a job behaves the same on GitHub and on any Linux machine with Nix (and Docker for the
# cluster jobs); there is no runner image to emulate.
#
#   scripts/ci.sh <job>      # run one job; it enters its Nix dev shell itself
#   scripts/ci.sh --list     # the job names, `check` matrix first
#
# The cluster jobs create k3d clusters (`zeabur`, `zeabur-l2`) and delete them when they end; set
# ZEABUR_CI_KEEP=1 to keep `zeabur` for a look afterwards. K3S_IMAGE picks the k3s release.
# kubernetes-l2 builds Zeabur Identity from ZEABUR_IDENTITY_SRC (default ../identity) at the commit in ZEABUR_IDENTITY_PINNED.
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
k3s_image="${K3S_IMAGE:-rancher/k3s:v1.36.5-k3s1}"

# The `check` matrix in ci.yml must list exactly the jobs above, or a job runs only locally.
check_matrix() {
  local listed
  listed=$(awk '/^  check:/{in_check=1} /^  [a-z]/ && !/^  check:/{in_check=0}
    in_check && /^          - name: /{sub(/^          - name: /, ""); print}' .github/workflows/ci.yml)
  if [[ "$listed" != "$(printf '%s\n' "${check_jobs[@]}")" ]]; then
    echo "error: the check matrix in ci.yml differs from scripts/ci.sh:" >&2
    diff <(printf '%s\n' "${check_jobs[@]}") <(echo "$listed") >&2 || true
    return 1
  fi
}

# The disposable cluster of the kubernetes job, in a kubeconfig of its own so that it never
# changes the machine's current context.
start_cluster() {
  export KUBECONFIG="$root/target/ci/kubeconfig"
  mkdir -p "$(dirname "$KUBECONFIG")"
  k3d cluster delete zeabur >/dev/null 2>&1 || true
  if [[ -z "${ZEABUR_CI_KEEP:-}" ]]; then
    trap 'k3d cluster delete zeabur >/dev/null 2>&1 || true' EXIT
  fi
  k3d cluster create zeabur --image "$k3s_image" --no-lb --wait --timeout 300s
  # Docker's containerd store keeps busybox as a multi-platform index, and the k3s node's import
  # fails on the platforms it lacks while k3d still reports success. Rebuild it as one plain image
  # (the classic builder writes no index or attestations; the label makes it a new image rather
  # than the same index), import that, and fail unless the node really has it. Read the whole
  # list before matching: `grep -q` exits at the first match, and under pipefail the SIGPIPE
  # crictl then gets would fail the check.
  docker pull busybox:1.37
  printf 'FROM busybox:1.37\nLABEL com.zeabur.test=busybox\n' |
    DOCKER_BUILDKIT=0 docker build -t busybox:1.37 -
  k3d image import busybox:1.37 --cluster zeabur
  local images
  images=$(docker exec k3d-zeabur-server-0 crictl images)
  grep -q 'docker.io/library/busybox *1.37' <<<"$images"
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
    check_matrix
    cargo fmt --all --check
    nixfmt --check flake.nix
    shellcheck scripts/*.sh
    actionlint
    (cd web; bun ci; bun run fmt:check)
    ;;
  clippy) cargo lint ;;
  docs) cargo doc --workspace --no-deps --document-private-items --locked ;;
  test) cargo nextest run --workspace --all-features --locked --no-tests=warn ;;
  cargo-deny) cargo deny --locked check ;;
  unsafe-report) ./scripts/unsafe-report.sh ;;
  levels) ./scripts/check-levels.sh ;;
  # Zeabur Proxy's request parser for a minute (crates/zeabur-proxy/fuzz). Without a sanitizer the
  # stable toolchain builds it; the target's own assertions are what it checks.
  fuzz)
    cd crates/zeabur-proxy/fuzz
    cargo fuzz run parse --sanitizer none -- -max_total_time=60
    ;;
  # The web interface: banned styles, generated sentences up to date (before the build regenerates
  # them), lint, unused code and layer boundaries, new findings in the changed files, build, unit
  # tests, generated API types up to date, size budget, and a release build that embeds it. The
  # health and duplication report after it does not fail the job.
  ui)
    ./scripts/check-ui-styles.sh
    ./scripts/check-messages.sh
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
  # Integration tests (JSON Patch writes, dry run, identity, revocation), `zeaburctl doctor`,
  # browser tests in local mode with the embedded interface, and the image.
  kubernetes)
    start_cluster
    cargo nextest run --locked --run-ignored only -E 'binary(kubernetes)'
    check_doctor
    if ! { ./scripts/build-ui.sh && ./scripts/e2e-ui.sh; }; then
      browser_failure_context
      exit 1
    fi
    ./scripts/build-image.sh zeabur:ci
    ./scripts/build-image.sh zeabur-proxy:ci zeabur-proxy
    ;;
  # Zeabur in a cluster with Zeabur Identity, from their images and Zeabur's chart, through a browser:
  # passkey sign-in, elevation, terminal, rights, memory, removal.
  kubernetes-l2) K3S_IMAGE="$k3s_image" ./scripts/e2e-l2.sh ;;
esac
