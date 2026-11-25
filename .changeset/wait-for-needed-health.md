---
default: patch
---

# A component installs after the one it needs passes its health checks

Zeabur OSS used to install a component as soon as helm-controller finished the component it needs, and ran the health checks only at the end. A chart that calls an operator's webhook, such as VictoriaMetrics with its operator, then failed while the webhook was still starting, and the install stayed in progress. Now each component's health checks run once its own steps are ready, and the next component waits for them.
