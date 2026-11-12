#!/usr/bin/env bash
# One issue for each nightly job that fails, "Nightly <job> failing": opened on the first failure,
# commented on each night it keeps failing, closed when the job passes. A job that was skipped or
# cancelled changes nothing. Every issue carries the label `ci`, created on first use, and only
# open issues with that label are matched, so a person's issue of the same title is left alone. Run by the nightly-issues job of
# .github/workflows/ci.yml with
#   RESULTS   the `needs` context as JSON ({"<job>": {"result": "success|failure|..."}})
#   RUN_URL   the run's address
#   GH_TOKEN  a token that may write issues
set -euo pipefail

: "${RESULTS:?}" "${RUN_URL:?}"

label=ci
gh label create "$label" --description "Reported by an automated CI job" --color ededed 2>/dev/null || true

while IFS=$'\t' read -r job result; do
  title="Nightly $job failing"
  number=$(gh issue list --state open --label "$label" --search "\"$title\" in:title" --json number,title \
    --jq "map(select(.title == \"$title\")) | first | .number // empty")
  case "$result" in
    failure)
      if [[ -n "$number" ]]; then
        gh issue comment "$number" --body "Still failing: $RUN_URL"
      else
        gh issue create --label "$label" --title "$title" --body "The nightly \`$job\` job failed: $RUN_URL

This issue closes itself when the job passes again."
      fi
      ;;
    success)
      if [[ -n "$number" ]]; then
        gh issue close "$number" --comment "Passed again: $RUN_URL"
      fi
      ;;
  esac
done < <(jq -r 'to_entries[] | [.key, .value.result] | @tsv' <<<"$RESULTS")
