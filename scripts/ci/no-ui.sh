#!/usr/bin/env bash
# The API-only build needs no JavaScript toolchain: bun is taken off PATH.
set -euo pipefail
cd "$(dirname "$0")/../.."

./scripts/build-without-ui.sh
