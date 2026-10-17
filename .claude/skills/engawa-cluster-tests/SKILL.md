---
name: engawa-cluster-tests
description: Run Engawa's Kubernetes integration and end-to-end tests on a disposable k3s cluster. Use when changing engawa-kube, anything that reads or writes cluster objects, the server, its manifests or the container image.
---

# Cluster tests

The cluster tests are `#[ignore]`d and need a disposable cluster where you are cluster admin.
Run Docker/k3d on a Linux machine or VM, not on a workstation you care about; sync the working
tree there with `rsync -a --delete --exclude target --exclude node_modules --exclude web/dist
./ host:engawa/` and run the commands below inside `nix develop` on that machine.

## Cluster
```sh
k3d cluster create engawa --image rancher/k3s:v1.36.5-k3s1 --no-lb --wait --timeout 300s
docker pull busybox:1.37 && k3d image import busybox:1.37 --cluster engawa   # the exec test's pod
```

## Run
```sh
cargo nextest run --locked --run-ignored only -E 'binary(kubernetes)'   # every crate's tests/kubernetes.rs
./scripts/build-ui.sh && ./scripts/e2e-ui.sh                             # browser tests, local mode
./scripts/e2e-l2.sh                   # in a cluster of its own with Genkan (needs ../genkan, port 443)
```

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

## What to check when something fails
- **`403` where you expected success**: `kubectl auth can-i --as system:serviceaccount:<ns>:alice
  <verb> <resource> -n <ns>`. WebSocket exec needs `create` on `pods/exec`, not only `get`.
- **The exec test times out**: the pod's image was not imported or pulled; `kubectl describe pod
  shell -n <ns>`.

Clean up with `k3d cluster delete engawa`.
