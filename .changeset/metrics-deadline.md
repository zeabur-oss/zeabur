---
default: patch
---

# Usage stops waiting for a metrics API that doesn't answer

The Nodes page, cluster usage and a project's usage give metrics-server five seconds. When kube-apiserver cannot reach it, it answered only after about a minute, and the page showed nothing until then. Now the page shows nodes and requests at once and says usage is unavailable.
