#!/usr/bin/env bash
# scripts/ci/e2e.sh on the full profile, which adds Zeabur Builder, CloudNativePG and Percona
# MongoDB: the browser specs that deploy from Git, from an uploaded folder and a database run here
# and skip in `e2e`.
set -euo pipefail
exec "$(dirname "$0")/e2e.sh" --profile full
