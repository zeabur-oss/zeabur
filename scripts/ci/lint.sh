#!/usr/bin/env bash
# The short checks: each needs about a minute to set up a runner and its dev shell but much less
# to run. `lint.sh` installs the root packages and runs both groups one after another; CI installs
# them once and runs the groups side by side. The cargo group stays in one process because its
# steps share the target/ lock.
set -euo pipefail
cd "$(dirname "$0")/../.."
source scripts/lib.sh

static() {
  cargo fmt --all --check
  nixfmt --check flake.nix
  shellcheck scripts/*.sh scripts/ci/*.sh
  jactionlint
  (cd web; bun run fmt:check)
  (cd crates/zeabur-installer/web; bun run fmt:check)
  (cd scripts/shots; bun run fmt:check)
  bun fmt --check scripts/*.ts
  bun check scripts/
  bun test scripts/
}

cargo_checks() {
  cargo lint
  RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items --locked
  cargo deny --locked check
  # Writes target/unsafe-report.md, which the workflow uploads.
  node_modules/.bin/zeabur-oss-unsafe-report zeabur zeaburctl zeabur-proxy zeabur-installer
  ./scripts/check-levels.sh
  # The Helm chart: lint, what every rendering promises, and the values it refuses.
  ./scripts/check-chart.sh
}

case "${1:-all}" in
  static) static ;;
  cargo) cargo_checks ;;
  all) bun install --frozen-lockfile; static; cargo_checks ;;
  *) echo "usage: lint.sh [static|cargo]" >&2; exit 2 ;;
esac
