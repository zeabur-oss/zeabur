#!/usr/bin/env bash
# The API-only build needs no JavaScript toolchain: bun is taken off PATH.
# shellcheck source-path=SCRIPTDIR
source "$(dirname "$0")/lib.sh"

./scripts/build-without-ui.sh
