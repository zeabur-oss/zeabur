---
name: engawa-cluster-tests
description: Run Engawa's Kubernetes integration and end-to-end tests on a disposable k3s cluster. Use when changing engawa-kube, anything that reads or writes cluster objects, the server, its manifests or the container image.
---

# Cluster tests

The cluster tests are `#[ignore]`d and need a disposable cluster where you are cluster admin.
Run Docker/k3d on a Linux machine or VM, not on a workstation you care about; sync the working
tree there with `rsync -a --delete --exclude target --exclude node_modules --exclude web/dist
./ host:engawa/` and run the commands below inside `nix develop` on that machine.

**On a VM, snapshot its clean state and restore it after every run.** The tests run in the VM's
own environment, not a fresh container: what one run leaves (clusters, images, volumes, build
output, `node_modules`, tools installed by hand) can make the next pass where a fresh GitHub
runner fails. Take the snapshot once Nix and Docker are installed (after one `nix develop`, so
the store is warm, if you like), and restore it before the next run (`virsh snapshot-revert`,
`prlctl snapshot-switch`, or your hypervisor's equivalent). A pull request's GitHub checks
remain the answer.

## Cluster
```sh
export KUBECONFIG=$(mktemp)   # keep k3d away from your normal context
k3d cluster create engawa --image rancher/k3s:v1.36.5-k3s1 --no-lb --wait --timeout 300s
docker pull busybox:1.37 && k3d image import busybox:1.37 --cluster engawa   # the exec test's pod
```
The `kubernetes` job in `.github/workflows/ci.yml` rebuilds busybox as a single-platform image
before importing it and checks that the node really has it; do the same if the import seems to
succeed but pods cannot find the image. To run the whole job as CI does, use
`scripts/ci-local.sh kubernetes` (see `engawa-ci`).

## Run
```sh
cargo nextest run --locked --run-ignored only -E 'binary(kubernetes)'   # every crate's tests/kubernetes.rs
./scripts/build-ui.sh && ./scripts/e2e-ui.sh                             # browser tests, local mode
./scripts/e2e-l2.sh                   # in a cluster of its own with Genkan (needs ../genkan, port 443)
```
- `e2e-ui.sh` recreates the namespace `engawa-e2e` from `web/e2e/manifests/talker.yaml` (a
  healthy service, a ConfigMap reader, and `crasher`, which exits 1 at once), waits until
  `crasher` has restarted twice, starts the release `engawactl ui`, and runs `web/e2e` with
  Playwright. Failures land in `target/e2e-ui/results/<test>/` (`error-context.md`, a trace).
- `e2e-l2.sh` builds Genkan at the commit in `GENKAN_PINNED` from `GENKAN_SRC` (default
  `../genkan`, refused at another commit), creates the cluster `engawa-l2`
  (`ENGAWA_L2_KEEP=1` keeps it), and runs `web/e2e/l2`. Its header lists every step.

## Writing one
- Start from `engawa_test_support::sandbox::Sandbox::new(prefix, rules)` (dev-dependency with the
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
  answers `unable to retrieve container logs for containerd://…` (with status 200; Engawa reports
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
- **It fails on GitHub but not here**: see `engawa-ci` (reading the job's log, reproducing with
  `scripts/ci-local.sh`, timing and `pipefail` pitfalls).

Clean up with `k3d cluster delete engawa`.
