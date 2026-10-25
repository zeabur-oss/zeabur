---
name: zeabur-cluster-tests
description: Run Zeabur's Kubernetes integration and end-to-end tests on a disposable k3s cluster. Use when changing zeabur-kube, anything that reads or writes cluster objects, the server, its manifests or the container image.
---

# Cluster tests

The cluster tests are `#[ignore]`d and need a disposable cluster where you are cluster admin.
Run Docker/k3d on a Linux machine or VM, not on a workstation you care about; sync the working
tree there with `rsync -a --delete --exclude target --exclude node_modules --exclude web/dist
./ host:zeabur/` and run the commands below inside `nix develop` on that machine.

**On a VM, snapshot its clean state and restore it after every run.** The tests run in the VM's
own environment, not a fresh container: what one run leaves (clusters, images, volumes, build
output, `node_modules`, tools installed by hand) can make the next pass where a fresh GitHub
runner fails. Take the snapshot once Nix and Docker are installed (after one `nix develop`, so
the store is warm, if you like), and restore it before the next run (`virsh snapshot-revert`,
`prlctl snapshot-switch`, or your hypervisor's equivalent). A pull request's GitHub checks
remain the answer.

## Cluster
```sh
nix run .#dev-cluster -- up [--profile bare|full] [--identity-src ../identity]
export KUBECONFIG=$PWD/target/dev-cluster/kubeconfig   # never your normal context
nix run .#dev-cluster -- down
```
`scripts/dev-cluster.sh` is the one cluster setup: the `kubernetes` and `kubernetes-l2` jobs,
`e2e-l2.sh` and `release-smoke.sh` all use it. It installs Zeabur Identity (built from
`ZEABUR_IDENTITY_PINNED`, fetched from `ZEABUR_IDENTITY_GIT`, or from `--identity-src`) and Zeabur
from `zeabur-installer manifests` (the installer's own HelmChart and values; a `HelmChartConfig`
adds Zeabur Proxy), and imports busybox for the exec test. `--profile full` adds cert-manager,
CloudNativePG with Barman Cloud, Percona MongoDB, Keel and zot (each with the Helm commands Zeabur
Catalog gives), the test S3, and Zeabur Builder's chart from `ZEABUR_BUILDER_PINNED`, pushing
anonymously to zot. The node pulls `zot.zot.svc:5000` as zeabur-installer sets it up: a mirror to
zot's fixed ClusterIP 10.43.0.50, checked against zot's CA.
`--profile bare` is the cluster alone: run `binary(kubernetes)` and `e2e-ui.sh` there (the
`kubernetes` job does), since Zeabur's ValidatingAdmissionPolicy `zeabur-project-namespace` stops
their non-admin people from changing namespace labels. To run a whole
job as CI does, use `scripts/ci-local.sh kubernetes` (see `zeabur-ci`).

## Run
```sh
cargo nextest run --locked --run-ignored only -E 'binary(kubernetes)'   # every crate's tests/kubernetes.rs
./scripts/build-ui.sh && ./scripts/e2e-ui.sh                             # browser tests, local mode
./scripts/e2e-l2-browser.sh           # web/e2e/l2, Zeabur in the cluster
./scripts/e2e-l2.sh                   # up, web/e2e/l2, rights, memory, removal, down
```
- `e2e-ui.sh` recreates the namespace `zeabur-e2e` from `web/e2e/manifests/talker.yaml` (a
  healthy service, a ConfigMap reader, and `crasher`, which exits 1 at once), waits until
  `crasher` has restarted twice, starts the release `zeaburctl ui`, and runs `web/e2e` with
  Playwright. Failures land in `target/e2e-ui/results/<test>/` (`error-context.md`, a trace).
- `e2e-l2.sh [up options]` brings the cluster up, runs `web/e2e/l2`, checks from outside, and
  deletes the cluster (`ZEABUR_L2_KEEP=1` keeps it and skips the removal check). Its header lists
  every step.
- `e2e-l2-browser.sh [playwright args]` runs only `web/e2e/l2` on the cluster. It deletes the e2e
  users and their `zeabur.com/member` bindings and makes them again first, so every user comes
  from today's code; never reuse a user from an earlier run. On the full profile a spec that
  would skip for a missing component fails instead.
- `e2e/l2/git-private.spec.ts` deploys a private GitHub repository with a deploy key and runs
  only with `E2E_PRIVATE_GIT_REPO` set (e.g. `git@github.com:zeabur-oss/oss-private-git-demo.git`).
  With `E2E_GH_DEPLOY_KEY=1` it adds the key read-only and removes it with `gh repo deploy-key`
  (gh signed in with admin on that repository); otherwise it prints `E2E_DEPLOY_KEY_ADD …` and
  `E2E_DEPLOY_KEY_REMOVE …` lines and waits for someone to do each.

## Writing one
- Start from `zeabur_test_support::sandbox::Sandbox::new(prefix, rules)` (dev-dependency with the
  `sandbox` feature): the admin context only prepares a fresh namespace, a service account
  standing in for a person, its Role of `rules` and RoleBinding, and a token from the
  TokenRequest API. Every assertion goes through the person (`user()`, `user_api()`), never
  through the admin client (`admin_api()`).
- Test the boundary, not only the happy path: a namespace outside the person's rights is `403`;
  deleting their RoleBinding makes the very next request `403`; another field manager's field is
  a `409` without force.
- Dropping the sandbox deletes its namespace, and the cluster-scoped roles named with
  `delete_cluster_role`, even when the test fails.

## What the kubelet reports for a crash-looping container
Observed on k3s v1.36.5 with a container that exits at once; tests about a failing pod must hold
in every one of these states, not only the first:
- **After the first restart**: `state.waiting.reason: CrashLoopBackOff`; the previous run's log
  (`previous=true`) is readable.
- **From the second restart on**, for most of each back-off: `state.terminated.reason: Error`,
  and `lastState` names the run before, which the kubelet has already removed: `previous=true`
  answers `unable to retrieve container logs for containerd://…` (with status 200; Zeabur reports
  it as a `log-unavailable` problem). The current log (`previous=false`) holds the crashed run's
  output.
- **For a moment after each restart**: `state.running`, with the last exit in `lastState`; the
  interface shows "Running, n restarts, last exit 1".

Wait on `restartCount` (as `e2e-ui.sh` does), and read both logs until one has the line.

## What to check when something fails
- **`403` where you expected success**: `kubectl auth can-i --as system:serviceaccount:<ns>:alice
  <verb> <resource> -n <ns>`. WebSocket exec needs `create` on `pods/exec`, not only `get`.
- **The exec test times out**: the pod's image was not imported or pulled; `kubectl describe pod
  shell -n <ns>`.
- **It fails on GitHub but not here**: see `zeabur-ci` (reading the job's log, reproducing with
  `scripts/ci-local.sh`, timing and `pipefail` pitfalls).

Clean up with `nix run .#dev-cluster -- down`.
