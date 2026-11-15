#!/usr/bin/env bash
# Lints zh-TW text with zhtw-mcp (https://github.com/sysprog21/zhtw-mcp). Fails on any warning or
# error. Pipe the text in, or name files; extra arguments go to `zhtw-mcp lint`.
#
#   producer-of-zh-TW-text | check-zh-tw.sh [lint options]
#   check-zh-tw.sh [lint options] file…
#
# Rules and glossary come from .zhtw-mcp.toml in the repository root, or from $ZHTW_CONFIG. Without
# either, zhtw-mcp's defaults apply.
# Edit this in zeabur-oss/.github (skills/zeabur-writing/scripts/check-zh-tw.sh); the weekly skills sync overwrites the copy in other repositories.
set -euo pipefail

root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
config="${ZHTW_CONFIG:-$root/.zhtw-mcp.toml}"
args=()
[[ -f "$config" ]] && args+=(--config "$config")

if [[ -t 0 && $# -eq 0 ]]; then
  echo "usage: producer | check-zh-tw.sh [lint options], or check-zh-tw.sh file…" >&2
  exit 2
fi
# With no file argument, lint standard input.
files=0
for a in "$@"; do [[ "$a" != -* && -e "$a" ]] && files=1; done
if ((files)); then
  exec zhtw-mcp "${args[@]}" lint "$@"
fi
exec zhtw-mcp "${args[@]}" lint "$@" --
