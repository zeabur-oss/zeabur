---
default: patch
---

# Dragonfly starts on any server and shows its logs

Each size sets a CPU limit, the thread count and `maxmemory` together, so a store no longer exits on servers with more CPUs than its memory allows threads for. Small now needs 384 MiB. Dragonfly's pods are found by the labels its operator sets, so Logs, the terminal and crash diagnosis see them.
