#!/usr/bin/env bash
# One CI job, as zeabur-oss/.github's shared CI workflow runs it and as anyone runs it locally. The
# jobs are the rows of scripts/ci/jobs; each one is scripts/ci/<job>.sh, run in its Nix dev shell.
# This file is the same in every zeabur-oss repository.
#
#   scripts/ci.sh <job>             # run one job; it enters its dev shell itself
#   scripts/ci.sh --list            # every job name
#   scripts/ci.sh --matrix <tier> [changed-files]
#                                   # the shared workflow's matrix: `fast` or `all`, without Nix
#
# A job may leave files in $CI_ARTIFACTS; the workflow uploads them as the artifact `ci-<job>`.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
table=scripts/ci/jobs

# Each row as `job key=value...`, with the `*` row's values first.
rows() {
  awk '
    /^[[:space:]]*(#|$)/ { next }
    $1 == "*" { $1 = ""; defaults = $0; next }
    { job = $1; $1 = ""; print job defaults $0 }
  ' "$table"
}

# The value of key $2 in job $1's row, or $3.
attr() {
  rows | awk -v job="$1" -v key="$2" -v fallback="${3:-}" '
    $1 == job { for (i = 2; i <= NF; i++) if (index($i, key "=") == 1) value = substr($i, length(key) + 2); found = 1 }
    END { if (!found) exit 1; print (value == "" ? fallback : value) }
  '
}

case "${1:-}" in
  --list)
    rows | awk '{ print $1 }'
    exit 0
    ;;
  # A job inside a group runs only in its group. With a list of changed files, a job is left out
  # when every file matches one of its skip-if-only patterns (bash patterns, `*` also matches `/`).
  # Keys other than tier, in, shell and skip-if-only go to the workflow as they are.
  --matrix)
    tier="${2:?usage: scripts/ci.sh --matrix fast|all [changed-files]}"
    changed=()
    [[ -z "${3:-}" ]] || mapfile -t changed <"$3"
    out=()
    while read -r job fields; do
      t=fast inside=0 skip="" json=""
      for field in $fields; do
        key="${field%%=*}" value="${field#*=}"
        case "$key" in
          tier) t="$value" ;;
          in) inside=1 ;;
          shell) ;;
          skip-if-only) skip="$value" ;;
          *) json+=",\"$key\":\"$value\"" ;;
        esac
      done
      [[ $inside == 0 ]] || continue
      [[ $tier != fast || $t == fast ]] || continue
      if [[ -n "$skip" && ${#changed[@]} -gt 0 ]]; then
        IFS=, read -ra patterns <<<"$skip"
        needed=0
        for path in "${changed[@]}"; do
          hit=0
          for pattern in "${patterns[@]}"; do
            # shellcheck disable=SC2053 # the pattern is meant to match as a pattern
            [[ "$path" == $pattern ]] && { hit=1; break; }
          done
          [[ $hit == 1 ]] || { needed=1; break; }
        done
        [[ $needed == 1 ]] || continue
      fi
      out+=("{\"job\":\"$job\"$json}")
    done < <(rows)
    (IFS=,; echo "[${out[*]}]")
    exit 0
    ;;
esac

job="${1:?usage: scripts/ci.sh <job> | --list | --matrix fast|all [changed-files]}"
attr "$job" tier >/dev/null || { echo "error: no job named $job (scripts/ci.sh --list)" >&2; exit 1; }

# A group runs every member, also after one fails, and fails if any did.
members=$(rows | awk -v group="$job" '{ for (i = 2; i <= NF; i++) if ($i == "in=" group) print $1 }')
if [[ -n "$members" ]]; then
  failed=()
  for member in $members; do
    echo "::group::$member"
    "$0" "$member" || failed+=("$member")
    echo "::endgroup::"
  done
  [[ ${#failed[@]} -eq 0 ]] || { echo "::error::failed: ${failed[*]}"; exit 1; }
  exit 0
fi

# The marker tells a run inside the job's dev shell apart from a call outside it.
shell=$(attr "$job" shell default)
if [[ "${CI_SHELL:-}" != "$shell" ]]; then
  export CI_SHELL="$shell"
  exec nix develop "$root#$shell" --command "$0" "$@"
fi
exec "$root/scripts/ci/$job.sh"
