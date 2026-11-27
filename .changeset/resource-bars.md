---
default: patch
---

# Service Settings shows CPU and memory as bars beside the fields

Under each container's CPU and memory fields in a service's Settings, a thin bar shows the reserved amount to the maximum on a scale up to the largest node, with a tick at the container's current usage in its busiest pod. The bar follows the fields as you type. The line that named the largest node's size is gone: each bar shows it.
