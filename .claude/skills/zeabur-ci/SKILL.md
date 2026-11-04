---
name: zeabur-ci
description: Investigate a failed or flaky CI or release run of Zeabur (GitHub Actions or scripts/ci-local.sh) — read the job's log, find the cause, reproduce it on a machine sized like the runner, and fix the test or the code rather than retrying. Use when a check on a pull request or on main fails, or a test passes and fails without a change.
---

# When CI fails

Follow `zeabur-oss-ci` (install: `npx skills add zeabur-oss/skills --skill zeabur-oss-ci`;
`skills-lock.json` pins it): how CI runs `scripts/ci/<job>.sh`, reading the log,
finding the cause, reproducing, fixing, and its release recovery notes. This skill adds Zeabur's
jobs and tools.

## Jobs

The jobs of `.github/workflows/ci.yml`, each `nix develop -c scripts/ci/<job>.sh`: `lint`
(format, clippy, docs, cargo-deny, unsafe-report, levels, chart in a row), `test` and `ui` on every
pull request; `fuzz`, `no-ui`, `wording`, `cluster` (cluster tests, `zeaburctl doctor`,
browser tests in local mode) and `e2e` (Zeabur and Zeabur Identity in a cluster,
`scripts/e2e.sh`) only in the merge queue, on a tag, nightly and by hand; the cluster jobs run on
`scripts/dev-cluster.sh`. What each cluster job runs is in
`zeabur-cluster-tests`.

`.github/workflows/release.yml` runs on a `vX.Y.Z` or `vX.Y.Z-<pre-release>` tag: zeabur-oss/.github's
shared release workflow, each stage `bun scripts/release.ts <stage>`: `build`
(Linux amd64/arm64, macOS arm64: `scripts/release-build.sh`, then `scripts/release-smoke.sh` on
Linux), `sign` (macOS), `publish`, `attest`, `finish` (the installers),
`release` (`assets`, the draft release) and `verify` (on the draft's assets);
`@zeabur-oss/release` (`package.json`) pushes, signs and verifies. A failed release run:
`zeabur-oss-ci`'s RELEASE-RECOVERY.md, then [RELEASE-RECOVERY.md](RELEASE-RECOVERY.md) here.

## Reading a failure

- A failed browser test prints each test's `error-context.md` (the page as Playwright saw it) and
  `kubectl get pods,events -A` in the step "Browser test failure context"; read that before
  guessing. Playwright's line reporter gives the file and line of the assertion.
- Polling and isolation rules: `docs/internal/testing.md`, "Waiting and isolation". Known timing
  cases are in `zeabur-cluster-tests`.

## Reproducing

```sh
rsync -a --delete --exclude target --exclude node_modules --exclude web/dist ./ host:zeabur/
ssh host 'cd zeabur && nix develop -c scripts/ci/cluster.sh'   # one job
ssh host 'cd zeabur && scripts/ci-local.sh'                # every job, a result line each
```
- `scripts/ci-local.sh [job…]` runs each job in its own copy of the checkout, three at a time
  (`CI_LOCAL_PARALLEL`), the cluster jobs alone at the end; results in
  `target/ci-local/<job>.result`, logs next to them, a failed job's copy kept under `work/`. Its
  header documents the settings.
- It refuses to start with less than 25 GB free (`CI_LOCAL_MIN_FREE_GB`).
- The cluster jobs download the Zeabur Identity release `ZEABUR_IDENTITY_PINNED` names from
  GitHub, and the node pulls its image from ghcr.io. Set `ZEABUR_IDENTITY_SRC` to a checkout synced
  along to build it instead.

## Fixing

Run what the change touches locally (`bun run fmt:check`, `bun run lint`, `shellcheck`,
`cargo nextest run -p …`), then let CI run the rest; see `zeabur-workflow`.
