---
default: patch
---

# The Logs tab of a container that has not started stays still

For a service whose container has not started yet, such as one waiting for its build, the Logs tab flashed between a placeholder and "no longer exists". It now says the container has not started.
