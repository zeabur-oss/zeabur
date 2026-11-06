---
default: minor
---

# API tokens on the Members page

Admins can now create API tokens on the Members page, so scripts and CI can call the Kubernetes API. Each token is a ServiceAccount in the `zeabur-api-tokens` namespace, which the chart creates, and gets roles the same way a member does: Developer or Viewer in chosen projects, or full rights to the cluster as an admin token. A token expires and is shown once with a kubeconfig template and the cluster's CA certificate. Renew issues a new token for the same ServiceAccount, and Delete revokes every token issued for it.
