---
default: patch
---

# Zeabur Proxy's image is published with each release

`ghcr.io/zeabur-oss/proxy` is signed and attested with each release, and the chart pins it by digest. 0.1.1 did not publish it, so an installation with VictoriaMetrics or VictoriaLogs left `zeabur-proxy` unable to pull its image.
