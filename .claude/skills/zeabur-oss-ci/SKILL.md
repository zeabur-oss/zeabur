---
name: zeabur-oss-ci
description: How CI works in every zeabur-oss repository (a plain .github/workflows/ci.yml that lists its jobs, each running scripts/ci/<job>.sh) and how to investigate a failed or flaky CI or release run — read the job's log, find the cause, reproduce it on a Linux machine sized like the runner, and fix the test or the code rather than retrying. Use when a check on a pull request, in the merge queue or on main fails, a test passes and fails without a change, a release run fails, or when adding a CI job.
---

<!-- Edit this in zeabur-oss/skills (skills/zeabur-oss-ci/SKILL.md); the weekly skills sync overwrites the copy in other repositories. -->

# Zeabur OSS CI

## How it is wired

`.github/workflows/ci.yml` lists its jobs directly. Each job checks out, runs
`zeabur-oss/.github/actions/ci-setup@<sha>` (Nix and caches), then `nix develop -c scripts/ci/<job>.sh`
(zeabur's `wording` job uses `.#zh`). A job that leaves files uploads them itself with
`actions/upload-artifact`; the artifact name is in the job.

- Pull requests run the fast jobs. The slow ones (cluster, e2e, kubernetes, fuzz, no-ui, wording,
  catalog smoke) run only in the merge queue, the nightly schedule and manual runs, through an `if:`
  on the event.
- The required check is a plain job `ci / ok` with `if: always()` (`check / ok` and `smoke-ok` in
  catalog). zizmor is the reusable `zizmor.yml` (`zizmor / zizmor`) and runs on every pull request.
  `dco / DCO` runs on pull requests only.
- `update.yml` opens one weekly pull request (Monday 02:30 Taipei; `workflow_dispatch` for an urgent
  one) that moves the workflow pins and the shared skills together. Renovate runs daily from 03:00
  to 07:00 Taipei, one pull request per update; K3s follows its stable channel.

A new check is a new job in `ci.yml` plus `scripts/ci/<job>.sh`.

`.github/workflows/release.yml` runs on a `vX.Y.Z` tag and calls the shared release workflow, each
stage `bun scripts/release.ts <stage>`. For a failed release run,
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
- Read the job's artifact, if it uploads one (`gh run download <run> -n <name>`), before guessing.
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

Run the same `nix develop -c scripts/ci/<job>.sh` on a Linux machine or VM with Nix (and Docker for the cluster
jobs), following `zeabur-oss-cluster-tests` for where and how (sync, snapshot and restore, one
run at a time):

```sh
ssh host 'cd <repo> && nix develop -c scripts/ci/<job>.sh'
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
