#!/usr/bin/env bash
# Fails if a tracked file writes the brand capitalised: it is lowercase, `engawa`, everywhere a
# person reads it, at the start of a sentence too
# (https://github.com/engawahq/landing/blob/main/CONTRIBUTING.md). Only the whole word is matched:
# identifiers that contain it (`NotFromEngawa`) and `ENGAWA_*` variables pass.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# The bracket keeps this line from matching itself.
if git grep -n -I -w '[E]ngawa'; then
  echo "error: write the brand in lowercase, engawa (lines above)" >&2
  exit 1
fi
