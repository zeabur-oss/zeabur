#!/usr/bin/env bash
# shellcheck source-path=SCRIPTDIR
source "$(dirname "$0")/lib.sh"

cargo nextest run --workspace --all-features --locked --no-tests=warn
