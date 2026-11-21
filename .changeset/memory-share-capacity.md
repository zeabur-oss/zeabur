---
default: patch
---

# Memory alert no longer fires on a healthy small server

The Overview divided the memory in use by the whole machine, including the system's, by allocatable memory, which leaves out the system reserve. A new install on a 4 GB server showed "memory used 153%". It now divides by the machine's memory.
