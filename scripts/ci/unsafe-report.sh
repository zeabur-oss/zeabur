#!/usr/bin/env bash
# shellcheck source-path=SCRIPTDIR
source "$(dirname "$0")/lib.sh"

bun install --frozen-lockfile
status=0
node_modules/.bin/zeabur-oss-unsafe-report zeabur zeaburctl zeabur-proxy zeabur-installer || status=$?
keep target/unsafe-report.md
exit "$status"
