---
default: minor
---

# Connect to any service from outside the cluster

`zeaburctl connect <project>/<name>` also reaches a service that is not a database: it forwards a local port to the service's Kubernetes Service and prints `127.0.0.1:<port>`. When the service has several ports, pick one with `<project>/<name>:<port>`.
