#!/usr/bin/env bash
# Zeabur Desktop for macOS as a user gets it: built in the Nix shell, bundled (scripts/build-desktop.sh,
# signed ad hoc) and checked to link nothing a Mac without Nix lacks (scripts/check-portable.sh).
set -euo pipefail
cd "$(dirname "$0")/../.."
./scripts/build-desktop.sh app
