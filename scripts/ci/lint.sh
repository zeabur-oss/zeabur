#!/usr/bin/env bash
# The short checks, one after another: each needs about a minute to set up a runner and its dev
# shell but much less to run.
set -euo pipefail
cd "$(dirname "$0")/../.."

cargo fmt --all --check
nixfmt --check flake.nix
shellcheck scripts/*.sh scripts/ci/*.sh
actionlint
(cd web; bun ci; bun run fmt:check)
(cd crates/zeabur-installer/web; bun ci; bun run fmt:check)
bun install --frozen-lockfile
bun fmt --check scripts/*.ts
bun check scripts/
bun test scripts/

cargo lint
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items --locked
cargo deny --locked check
# Writes target/unsafe-report.md, which the workflow uploads.
node_modules/.bin/zeabur-oss-unsafe-report zeabur zeaburctl zeabur-proxy zeabur-installer
./scripts/check-levels.sh
# The Helm chart: lint, what every rendering promises, and the values it refuses.
./scripts/check-chart.sh
