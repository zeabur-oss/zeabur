---
default: minor
---

# API tokens on the Members page

Admins can now create API tokens on the Members page, so scripts and CI can call the Kubernetes API. Each token is a ServiceAccount in the `zeabur-api-tokens` namespace, which the chart creates, and gets roles the same way a member does: Developer or Viewer in chosen projects. A token is never an admin; cluster-wide work uses an admin's own kubeconfig. A token expires and is shown once with a kubeconfig template and the cluster's CA certificate. Renew issues a new token for the same ServiceAccount, and Delete revokes every token issued for it. Tokens work without Zeabur Identity, so the Members page lists them even when Identity isn't installed.
