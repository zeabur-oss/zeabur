#!/usr/bin/env bash
# The interface's zh-TW wording (.zhtw-mcp.toml); runs in the zh dev shell, where zhtw-mcp builds
# from source.
# shellcheck source-path=SCRIPTDIR
source "$(dirname "$0")/lib.sh"

(cd web; bun ci)
./scripts/check-zh-tw.sh
