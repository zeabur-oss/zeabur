---
default: patch
---

# kube moves to 5.2.0 for RUSTSEC-2026-0600

The advisory covers kube's `openssl-tls` connector, which Zeabur OSS and Zeabur Desktop do not build, so they were not exposed. kube 5.2.0 has the fix, and with it the dependency audit passes again. Which certificates a kubeconfig with no `certificate-authority` trusts does not change.
