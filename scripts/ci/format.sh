#!/usr/bin/env bash
# shellcheck source-path=SCRIPTDIR
source "$(dirname "$0")/lib.sh"

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
