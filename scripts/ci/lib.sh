# shellcheck shell=bash
# Sourced by every scripts/ci/<job>.sh: strict mode, the repository root as the working directory,
# and what jobs share. This file is the same in every zeabur-oss repository.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
export RUSTDOCFLAGS="-D warnings"

# Copies a file into $CI_ARTIFACTS, which the workflow uploads; nothing without it or the file.
keep() { # $1 = file
  [[ -n "${CI_ARTIFACTS:-}" && -f "$1" ]] || return 0
  mkdir -p "$CI_ARTIFACTS"
  cp "$1" "$CI_ARTIFACTS/"
}

# Appends stdin to the run's summary when there is one, and prints it.
summary() {
  tee -a "${GITHUB_STEP_SUMMARY:-/dev/null}"
}
