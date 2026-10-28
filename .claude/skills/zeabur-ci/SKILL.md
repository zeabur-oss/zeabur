---
name: zeabur-ci
description: Investigate a failed or flaky CI or release run of Zeabur (GitHub Actions or scripts/ci-local.sh) — read the job's log, find the cause, reproduce it on a machine sized like the runner, and fix the test or the code rather than retrying. Use when a check on a pull request or on main fails, or a test passes and fails without a change.
---

# When CI fails

`.github/workflows/ci.yml` calls zeabur-oss/.github's shared CI workflow, which runs one job per
line of `scripts/ci.sh --list` (shown as `ci / <job>`): the checks (format, clippy, docs, test,
cargo-deny, unsafe-report, levels, fuzz, ui, no-ui, chart, wording), `kubernetes` (cluster tests,
`zeaburctl doctor`, browser tests in local mode) and `kubernetes-l2` (Zeabur and Zeabur Identity
in a cluster, `scripts/e2e-l2.sh`), both on `scripts/dev-cluster.sh`; a job's `$CI_ARTIFACTS`
is uploaded as `ci-<job>`. `dco / DCO` runs on pull requests only. What each cluster job runs
is in `zeabur-cluster-tests`.

`.github/workflows/release.yml` runs on a `vX.Y.Z` tag: all of `ci.yml` (`workflow_call`), then
zeabur-oss/.github's shared release workflow, each stage `scripts/release.sh <stage>`: `build`
(Linux amd64/arm64, macOS arm64: `scripts/release-build.sh`, then `scripts/release-smoke.sh` on
Linux and the docs screenshots on Linux amd64), `publish` (`scripts/release-publish.sh`), `attest`,
`finish` (the installers), `release` (`scripts/release-assets.sh`, the draft release) and `verify`
(`scripts/release-verify.sh` on the draft's assets). For a failed
release run, read [RELEASE-RECOVERY.md](RELEASE-RECOVERY.md) before changing permissions,
packages or artifacts.

## 1. Read the failure
```sh
gh run view <run> --json headSha,headBranch,event,jobs \
  --jq '{headSha,headBranch,event,jobs:[.jobs[]|{name,conclusion,databaseId}]}'
gh api repos/zeabur-oss/zeabur/actions/jobs/<job>/logs | sed 's/\x1b\[[0-9;]*m//g' > job.log
grep -nE 'panicked|FAILED|Error:|error\[|failed' job.log | head
```
- `gh run view --log` prints nothing while another job of the run is still going; the job's
  `logs` endpoint works at once.
- **`cancelled` is not a failure.** `concurrency` has `cancel-in-progress: true`: a new push to
  the same branch (or to main) cancels the run before it.
- A failed browser test prints each test's `error-context.md` (the page as Playwright saw it) and
  `kubectl get pods,events -A` in the step "Browser test failure context"; read that before
  guessing. Playwright's line reporter gives the file and line of the assertion.
- Note which commit failed (`headSha`) and whether the same test failed before on main
  (`gh run list --branch main --workflow CI`).

## 2. Find the cause before changing anything
State a hypothesis that explains the exact output, then prove it: reproduce it, or show the
state that makes it happen. Do not fix what you have not explained.

- **Runner size is rarely the cause.** The jobs run on `ubuntu-26.04-arm`; GitHub's standard
  runner for public repositories has 4 vCPU, 16 GB of memory and a 14 GB SSD
  ([GitHub-hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)).
  Before blaming it, reproduce the failure on a larger machine; if it fails there too, it is not
  the size.
- **Timing.** A test that asserts one state of something that changes on its own (a pod, a
  watch, a poll every few seconds) fails whenever it looks at the wrong moment. Poll with a
  deadline for what must eventually hold, and accept every state the system may legitimately be
  in (`docs/internal/testing.md`, "Waiting and isolation"). Known cases are in `zeabur-cluster-tests`.
- **Shell pipelines under `pipefail`.** GitHub runs `run:` steps with `bash -eo pipefail`, and
  the scripts set it too. `producer | grep -q pattern` can fail although the pattern is there:
  `grep -q` exits at the first match, the producer gets `SIGPIPE`, and the pipeline reports the
  producer's failure. Capture first, then match: `out=$(producer); grep -q pattern <<<"$out"`.
- **A full disk** fails jobs without an error of their own: a log cut off mid-line, a node that
  stops scheduling pods.

## 3. Reproduce
Every job is `scripts/ci.sh <job>`, which GitHub runs too, so run the same script on a Linux
machine or VM with Nix (and Docker for the cluster jobs), never on a workstation you care about
(the cluster jobs create clusters, images and volumes):
```sh
rsync -a --delete --exclude target --exclude node_modules --exclude web/dist ./ host:zeabur/
ssh host 'cd zeabur && scripts/ci.sh kubernetes'           # one job
ssh host 'cd zeabur && scripts/ci-local.sh'                # every job, a result line each
```
- `scripts/ci-local.sh [job…]` runs each job in its own copy of the checkout, three at a time
  (`CI_LOCAL_PARALLEL`), the cluster jobs alone at the end; results in
  `target/ci-local/<job>.result`, logs next to them, a failed job's copy kept under `work/`. Its
  header documents the settings.
- To match GitHub, give the VM 4 vCPU and 16 GB. Use a bigger one to rule size out.
- **Snapshot the VM's clean state and restore it after every run.** GitHub starts each job on a
  fresh machine; a VM keeps what the last run left (clusters, images, volumes, build output,
  `node_modules`, tools installed by hand), which can make a job pass here and fail there. Take
  the snapshot once Nix and Docker are installed, and restore it before the next run
  (`virsh snapshot-revert`, `prlctl snapshot-switch`, or your hypervisor's equivalent). The pull
  request's GitHub checks remain the answer.
- One run at a time per machine: check `pgrep -af 'ci-local.sh|ci.sh'` first.
- It refuses to start with less than 25 GB free (`CI_LOCAL_MIN_FREE_GB`). Reclaim space with
  `docker builder prune -f` and `docker volume prune -f`.
- The cluster jobs fetch Zeabur Identity at `ZEABUR_IDENTITY_PINNED` from `ZEABUR_IDENTITY_GIT`
  (default GitHub); point it at a mirror, or set `ZEABUR_IDENTITY_SRC` to a checkout synced along.
- For a timing failure, also observe the state directly (a `kubectl get -w`, or a loop printing
  the field the test reads every few seconds). One green run proves little; a recording of the
  state the test missed proves the cause.

## 4. Fix
- **A flaky test is broken**: fix it or remove it; never retry until green.
- Fix the test when the product behaves as designed, the product when it does not. Keep the
  assertion as strong as the promise: widen it to every legitimate state, not to anything.
- Update the comment that described the old belief (in the test and in the scripts it relies on).
- Run what the change touches locally (`bun run fmt:check`, `bun run lint`, `shellcheck`,
  `cargo nextest run -p …`), then let CI run the rest. The pull request says what failed (with
  the run link), why, how that was shown, and what changed; see `zeabur-workflow`.
