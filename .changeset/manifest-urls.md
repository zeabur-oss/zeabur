---
default: minor
---

# Install Falco, System Upgrade Controller and Traefik's Gateway API CRDs

Zeabur OSS now installs components whose releases are manifest files at URLs. It downloads each file and uses it only when its SHA-256 matches the one in Zeabur Catalog. A release is left out when the cluster already has the CRDs it lists in `skipIfPresent`, such as the policy report CRDs that Kyverno installs. Removing a component keeps the CRDs Zeabur OSS wrote, and the removal dialog lists them under **Stays on the cluster**. The cluster where Zeabur OSS runs needs to reach these URLs.
