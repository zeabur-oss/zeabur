---
default: patch
---

# Service Settings warns when usage is near the limit or above the request

When the busiest replica of a container uses 90% or more of its memory maximum, the CPU and memory fields in a service's Settings now say that the container is near its limit and restarts if it goes over. When the busiest replica uses more than the reserved CPU or memory, they say that it may get less when the node is busy. The warnings follow the fields as you type, and nothing shows without usage data.
