---
default: minor
---

# API tokens for a project

Project settings now have API tokens, so scripts and CI can call the Kubernetes API with the Developer or Viewer role. Each token belongs to its own ServiceAccount, expires, and is shown once with a kubeconfig template and the cluster's CA certificate. Renew issues a new token for the same ServiceAccount; Delete revokes every token issued for it. Developers can make tokens with the roles they hold, and only admins can make one for every namespace. Three admission policies in the chart keep these writes to API tokens.
