#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
source scripts/lib.sh

cargo nextest run --workspace --all-features --locked --no-tests=warn
scripts/check-database-updater.sh
