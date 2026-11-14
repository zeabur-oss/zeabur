---
default: patch
---

# The installer's downloads and usage data use Zeabur's own HTTP client

`zeabur-installer` and the usage data Zeabur sends now go through the same client as Zeabur's other outbound requests. `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY` and `NO_PROXY` follow curl's rules, the proxy must be an `http://` proxy that tunnels with `CONNECT`, and local addresses are never sent to it.
