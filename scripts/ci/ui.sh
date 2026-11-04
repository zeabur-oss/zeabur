#!/usr/bin/env bash
# The web interface: banned styles, lint, unused code and layer boundaries, new findings in the changed
# files, build, unit tests, generated API types up to date, the installer's interface built and linted, no use of a
# deprecated declaration in either, size budget, and a build that embeds it. The health
# and duplication report after it does not fail the job.
set -euo pipefail
cd "$(dirname "$0")/../.."

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
git diff --exit-code -- web/src/api/zeabur.ts
./scripts/build-installer-ui.sh
(cd crates/zeabur-installer/web; bun run lint)
(cd web; bun scripts/check-deprecated.ts . ../crates/zeabur-installer/web)
./scripts/ui-budget.sh
ZEABUR_UI_DIST="$PWD/web/dist" cargo build --profile ci --locked -p zeabur -p zeaburctl
ZEABUR_UI_DIST="$PWD/web/dist" cargo nextest run --cargo-profile ci --locked -p zeabur-ui --features interface
(cd web; bun run fallow health --report-only --top 10; bun run fallow dupes --summary)
