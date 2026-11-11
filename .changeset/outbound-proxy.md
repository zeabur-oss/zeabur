---
default: patch
---

# Zeabur's outbound requests can go through an HTTP proxy

Set `outboundProxy` in the chart to send the requests Zeabur makes itself (images, backups, GitHub) through an `http://` proxy with a `CONNECT` tunnel. Local addresses, `.svc`, `.cluster.local` and the cluster network are never sent to the proxy, and the same addresses are refused as without one.
