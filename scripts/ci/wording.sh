#!/usr/bin/env bash
# The interface's zh-TW wording (.zhtw-mcp.toml); runs in the zh dev shell, where zhtw-mcp builds
# from source.
set -euo pipefail
cd "$(dirname "$0")/../.."

(cd web; bun ci)
./scripts/check-zh-tw.sh
