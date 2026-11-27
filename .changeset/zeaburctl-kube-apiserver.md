---
default: patch
---

# zeaburctl acts through kube-apiserver only, and Developers can run `zeaburctl up`

`zeaburctl up` no longer has a server mode: it always acts with your kubeconfig through the Kubernetes API, as `zeaburctl ui` and `zeaburctl connect` do, and pushes the folder from your computer. `--server` and `ZEABUR_SERVER` are removed.

A Developer in any project now gets a RoleBinding in the build registry's namespace to the new `zeabur-registry-client` ClusterRole, so `zeaburctl up` works with a Developer's kubeconfig or API token. The binding is made when an admin saves the member's or token's access: save each Developer's access once to give existing Developers this right. Images in the build registry are not separated by project.

`zeaburctl connect` takes `<name>` with `--project`, and `<project>/<name>` still works. Without a project, `connect` and `up` use the namespace of the kubeconfig context, which an API token's kubeconfig sets when the token has a role in one project only.

A new guide, [Connect from your computer or CI](https://oss.zeabur.com/docs/zeabur/deploy/from-outside/), covers an API token's kubeconfig, `zeaburctl connect` with psql or DBeaver, and `zeaburctl up` from GitHub Actions.
