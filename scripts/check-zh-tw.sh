#!/usr/bin/env bash
# Lints the interface's zh-TW wording with zhtw-mcp (https://github.com/sysprog21/zhtw-mcp): every
# zh-TW string, as web/scripts/zh-tw-strings.ts prints it, against the rules and glossary in
# .zhtw-mcp.toml. Fails on any warning or error. Run inside `nix develop .#zh` (bun and
# zhtw-mcp), after `bun ci` in web/. Extra arguments go to `zhtw-mcp lint` (e.g. `--format json`).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

(cd web && bun scripts/zh-tw-strings.ts) | zhtw-mcp --config .zhtw-mcp.toml lint "$@" --
