---
default: minor
---

# Install and remove components from the Components page

On K3s and RKE2, a cluster administrator installs a component from **Cluster** → **Components**: Zeabur shows each step and the objects it writes, then writes one step at a time and waits for it to be ready, starting with cert-manager. **Manage** continues an unfinished install, or removes a finished one: Zeabur deletes what it installed in reverse order and keeps CRDs and keys. Template updates read the same `catalog.url`. Zeabur downloads Zeabur Catalog from `https://oss-catalog.zeabur.com/v1/` and keeps the last release in the ConfigMap `zeabur-catalog`; set `catalog.url` to serve a mirror to a cluster without internet access.
