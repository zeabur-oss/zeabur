#!/usr/bin/env bash
# The jobs of this Benchmark run, named "<job> <runner> <repetition>", as a table in the run's
# summary: for each job and runner, the runs that passed, the median wait for a runner, the median
# and slowest run time, and the Instabox usage of one median run (vCPU and memory per second,
# https://instabox.sh/docs/account/pricing). GitHub's standard runners are free in public
# repositories.
set -euo pipefail
gh api --paginate "repos/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID/jobs?per_page=100" \
  --jq '.jobs[] | select(.name | split(" ") | length == 3)' |
  jq -rs '
    def med: sort | .[(length - 1) / 2 | floor];
    def secs: fromdateiso8601;
    def size: {"instabox-small": [2, 4], "instabox-medium": [4, 8], "instabox-large": [8, 16],
      "instabox-xlarge": [16, 32], "instabox-2xlarge": [32, 64]}[.];
    def usd($s; $runner): ($runner | size) as $z
      | if $z then "$\($s / 3600 * ($z[0] * 0.004 + $z[1] * 0.00225) * 10000 | round / 10000)" else "-" end;
    map(select(.started_at and .completed_at) | (.name | split(" ")) as [$job, $runner]
      | {$job, $runner, ok: (.conclusion == "success"),
         wait: ((.started_at | secs) - (.created_at | secs)),
         run: ((.completed_at | secs) - (.started_at | secs))})
    | group_by([.job, .runner])
    | ["| Job | Runner | Passed | Wait (median s) | Run (median s) | Run (slowest s) | Instabox usage per run |",
       "|---|---|---|---|---|---|---|"]
      + map(map(select(.ok)) as $ok | .[0] as $first
        | if ($ok | length) == 0 then "| \($first.job) | \($first.runner) | 0/\(length) | - | - | - | - |"
          else ($ok | map(.run) | med) as $run
          | "| \($first.job) | \($first.runner) | \($ok | length)/\(length) | \($ok | map(.wait) | med) | \($run) | \($ok | map(.run) | max) | \(usd($run; $first.runner)) |"
          end)
    | .[]' >>"${GITHUB_STEP_SUMMARY:-/dev/stdout}"
