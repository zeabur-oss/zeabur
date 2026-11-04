#!/usr/bin/env bash
# Zeabur Proxy's request parser for a minute (crates/zeabur-proxy/fuzz). Without a sanitizer the
# stable toolchain builds it; the target's own assertions are what it checks.
# shellcheck source-path=SCRIPTDIR
source "$(dirname "$0")/lib.sh"

cd crates/zeabur-proxy/fuzz
cargo fuzz run parse --sanitizer none -- -max_total_time=60
