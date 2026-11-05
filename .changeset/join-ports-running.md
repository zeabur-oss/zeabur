---
default: patch
---

# Plan on a joined node doesn't ask for its own ports to be free

On a node whose `k3s-agent`, `rke2-agent` or `rke2-server` already runs, `plan` says ports 80, 443 and 10250 belong to the running unit, instead of listing them as ports that must be free. `apply` already skipped that check there.
