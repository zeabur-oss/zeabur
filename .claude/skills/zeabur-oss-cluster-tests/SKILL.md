---
name: zeabur-oss-cluster-tests
description: Where and how any zeabur-oss repository runs its Kubernetes integration and end-to-end tests (a disposable k3d, k3s or kind cluster on a Linux machine or VM, its own kubeconfig, a VM snapshot restored after every run), and how to write and debug a cluster test. Use before running or writing a test that needs a cluster or Docker, or when one fails.
---

# Zeabur OSS cluster tests

The repository's cluster-test skill (or its `CONTRIBUTING.md`) has the cluster script, the test
commands, its profiles and its known failures. These rules hold in every repository.

## Where they run

- **A disposable cluster where you are cluster admin**, with its own kubeconfig
  (`export KUBECONFIG=<the script's kubeconfig>`), never your normal context or one you did not
  create.
- **On a Linux machine or VM with Docker and Nix**, not on a workstation you care about: the
  tests create clusters, images and volumes. Sync the working tree there and run every command
  inside `nix develop` on that machine:
  `rsync -a --delete --exclude target --exclude node_modules ./ host:<repo>/` (exclude the
  repository's other build output too).
- **On a VM, snapshot its clean state and restore it after every run.** The tests run in the VM's
  own environment, not a fresh container: what one run leaves (clusters, images, volumes, build
  output, `node_modules`, tools installed by hand) can make the next pass where a fresh GitHub
  runner fails. Take the snapshot once Nix and Docker are installed (after one `nix develop`, so
  the store is warm, if you like), and restore it before the next run (`virsh snapshot-revert`,
  `prlctl snapshot-switch`, or your hypervisor's equivalent).
- One run at a time per machine: check `pgrep -af 'ci-local.sh|ci.sh'` first.
- A pull request's GitHub checks remain the answer. To run a cluster job as CI does, run
  `scripts/ci.sh <job>` on that machine (`zeabur-oss-ci`).
- Clean up with the repository's `down` or `cluster delete` command when you are done.

## Rust cluster tests

Cluster tests are `#[ignore]`d and live in each crate's `tests/kubernetes.rs`:

```sh
cargo nextest run --locked --run-ignored only -E 'binary(kubernetes)'
```

## Writing one

- **Assert through a person.** The admin context only prepares a fresh namespace, a service
  account standing in for a person, its Role and RoleBinding, and a token from the TokenRequest
  API. Every assertion goes through that person's credential, never through the admin client.
  Clean up the namespace and cluster-scoped objects even when the test fails.
- **Test the boundary, not only the happy path**: a namespace outside the person's rights is
  `403`; deleting their RoleBinding makes the very next request `403`; another field manager's
  field is a `409` without force.
- **A fake API server shows our logic.** What only kube-apiserver decides (RBAC, server-side
  apply, `resourceVersion` conflicts, admission, schema validation) needs a test against a real
  one.
- **Wait, do not sleep.** Something that changes on its own (a pod, a watch, a poll every few
  seconds) is polled with a deadline for what must eventually hold, and the assertion accepts
  every state the system may legitimately be in.

## What to check when something fails

- **`403` where you expected success**: `kubectl auth can-i --as system:serviceaccount:<ns>:<sa>
  <verb> <resource> -n <ns>`. WebSocket exec needs `create` on `pods/exec`, not only `get`.
- **A pod never starts**: `kubectl describe pod <pod> -n <ns>`; the image was not imported or
  pulled. `k3d image import` can print `ctr` "content digest … not found" and still exit 0 for a
  multi-platform image whose other platforms are absent; read its output and the pod's events.
- **`cannot change roleRef`**: a ClusterRoleBinding from an older install has a different role;
  delete it and apply again.
- **NodePort refused**: kube-proxy needs a moment after a Service is created; poll.
- **Concurrent deletes** on k3s can return 500 for the loser; code fails closed, tests accept it.
- **Memory**: measure it with the kubelet summary API
  (`/api/v1/nodes/<node>/proxy/stats/summary`).
- **It fails on GitHub but not here**: `zeabur-oss-ci`.
