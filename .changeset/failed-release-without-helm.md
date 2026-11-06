---
default: patch
---

# apply says how to undo a failed chart without Helm on the server

When a chart's Helm release is left `failed`, `apply` now prints commands that run Helm inside the cluster, with helm-controller's own image and ServiceAccount: one shows the release's history. The other rolls it back, or uninstalls it when the first install failed. A server installed with zeabur-installer has no Helm, so before this you had to install it or copy the admin kubeconfig to another machine.

If you switched an install from `single-node` to `multi-node` on 0.2.0-beta.0 or beta.1, zot's release may already be `failed`, and the zot command in the catalog v0.4.0 notes doesn't fix it. Run the rollback that `apply` prints, then `apply` again.
