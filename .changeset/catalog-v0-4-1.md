---
default: patch
---

# Zeabur ships with Zeabur Catalog v0.4.1

On a personal install with less than about 1.7 GiB of memory, VictoriaMetrics and VictoriaLogs are no longer installed. Templates show their summary in English, Traditional Chinese and Simplified Chinese, and the memory they used when idle. A component shows as tested only once the catalog's tests passed on its current file, on the server's Kubernetes distribution and platform. Switching between `single-node` and `multi-node` no longer changes the size of zot's and VictoriaMetrics' volumes, so both keep running. CrowdSec agents come back after their node restarts, and CrowdSec now needs cert-manager. An install created on `multi-node` runs three commands before its first upgrade, as the [catalog's v0.4.0 release notes](https://github.com/zeabur-oss/catalog/releases/tag/v0.4.0) say.

On `multi-node`, the database and storage operators, Reloader and k8up request about what they use, half or less of before, and VictoriaMetrics requests 224 MiB instead of 512 MiB while keeping two weeks of history ([v0.4.1 release notes](https://github.com/zeabur-oss/catalog/releases/tag/v0.4.1)).
