---
name: zeabur-oss-ci
description: How CI works in every zeabur-oss repository (the shared workflows of zeabur-oss/.github calling scripts/ci.sh) and how to investigate a failed or flaky CI or release run — read the job's log, find the cause, reproduce it on a Linux machine sized like the runner, and fix the test or the code rather than retrying. Use when a check on a pull request or on main fails, a test passes and fails without a change, a release run fails, or when adding a CI job.
---

# Zeabur OSS CI

## How it is wired

`.github/workflows/ci.yml` calls `zeabur-oss/.github/.github/workflows/ci.yml@<sha> # main` and
`dco.yml` at the same pin; Renovate moves the pin. The shared workflow runs one job per line of
`scripts/ci.sh --list` (shown as `ci / <job>`), plus zizmor on `.github`, and an `ok` job that
rulesets require. `dco / DCO` runs on pull requests only.

The contract of `scripts/ci.sh` (zeabur-oss/.github `README.md`):

- `scripts/ci.sh --list` prints the job names, one per line, without the Nix dev shell.
- `scripts/ci.sh <job>` runs one job. It enters the repository's dev shell itself, so a job runs
  the same on GitHub and on any Linux machine with Nix (and Docker, for cluster jobs).
- A job may leave files in `$CI_ARTIFACTS`; they are uploaded as the artifact `ci-<job>`.
- Inputs of the shared `ci.yml`: `runner` (default `ubuntu-26.04-arm`), `fetch-depth`,
  `rust-cache`, and `cache-paths` with `cache-key-files`.

A new check is a new job in `scripts/ci.sh`, not a new workflow step.

`.github/workflows/release.yml` runs on a `vX.Y.Z` tag: all of `ci.yml` (`workflow_call`), then
the shared release workflow, each stage `scripts/release.sh <stage>`. For a failed release run,
read [RELEASE-RECOVERY.md](RELEASE-RECOVERY.md) before changing permissions, packages or
artifacts.

## 1. Read the failure

```sh
gh run view <run> --json headSha,headBranch,event,jobs \
  --jq '{headSha,headBranch,event,jobs:[.jobs[]|{name,conclusion,databaseId}]}'
gh api repos/zeabur-oss/<repo>/actions/jobs/<job>/logs | sed 's/\x1b\[[0-9;]*m//g' > job.log
grep -nE 'panicked|FAILED|Error:|error\[|failed' job.log | head
```

- `gh run view --log` prints nothing while another job of the run is still going; the job's
  `logs` endpoint works at once.
- **`cancelled` is not a failure.** `concurrency` has `cancel-in-progress: true`: a new push to
  the same branch (or to main) cancels the run before it.
- Read the artifact `ci-<job>` (`gh run download <run> -n ci-<job>`) before guessing.
- Note which commit failed (`headSha`) and whether the same test failed before on main
  (`gh run list --branch main --workflow CI`).

## 2. Find the cause before changing anything

State a hypothesis that explains the exact output, then prove it: reproduce it, or show the
state that makes it happen. Do not fix what you have not explained.

- **Runner size is rarely the cause.** GitHub's standard runner for public repositories has
  4 vCPU, 16 GB of memory and a 14 GB SSD
  ([GitHub-hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)).
  Before blaming it, reproduce the failure on a larger machine; if it fails there too, it is not
  the size.
- **Timing.** A test that asserts one state of something that changes on its own (a pod, a
  watch, a poll every few seconds) fails whenever it looks at the wrong moment. Poll with a
  deadline for what must eventually hold, and accept every state the system may legitimately be
  in.
- **Shell pipelines under `pipefail`.** GitHub runs `run:` steps with `bash -eo pipefail`, and
  the scripts set it too. `producer | grep -q pattern` can fail although the pattern is there:
  `grep -q` exits at the first match, the producer gets `SIGPIPE`, and the pipeline reports the
  producer's failure. Capture first, then match: `out=$(producer); grep -q pattern <<<"$out"`.
- **A full disk** fails jobs without an error of their own: a log cut off mid-line, a node that
  stops scheduling pods.

## 3. Reproduce

Run the same `scripts/ci.sh <job>` on a Linux machine or VM with Nix (and Docker for the cluster
jobs), following `zeabur-oss-cluster-tests` for where and how (sync, snapshot and restore, one
run at a time):

```sh
ssh host 'cd <repo> && scripts/ci.sh <job>'
```

- To match GitHub, give the VM 4 vCPU and 16 GB. Use a bigger one to rule size out.
- Reclaim disk with `docker builder prune -f` and `docker volume prune -f`.
- For a timing failure, also observe the state directly (a `kubectl get -w`, or a loop printing
  the field the test reads every few seconds). One green run proves little; a recording of the
  state the test missed proves the cause.

## 4. Fix

- **A flaky test is broken**: fix it or remove it; never retry until green.
- Fix the test when the product behaves as designed, the product when it does not. Keep the
  assertion as strong as the promise: widen it to every legitimate state, not to anything.
- Update the comment that described the old belief (in the test and in the scripts it relies on).
- Run what the change touches locally, then let CI run the rest. The pull request says what
  failed (with the run link), why, how that was shown, and what changed; see
  `zeabur-oss-workflow`.
