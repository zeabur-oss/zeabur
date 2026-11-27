---
default: minor
---

# List each Kubernetes Service with its addresses by kind

The Networking tab has one **Kubernetes Services** section in place of **Private network** and **Public ports**. Each Service shows its type and, under each port, its addresses: in this project, from another project in this cluster, on the nodes for a `NodePort` or `LoadBalancer` Service, and from the load balancer. A `NodePort` Service made with YAML now shows its node addresses. Zeabur OSS reads the nodes as the person; when it can't, it says so and shows the node port alone.

The API gives each `Port` its `addresses`, each with a `kind`. `Endpoint.externalAddresses` is removed: read the addresses of kind `loadBalancer` instead.
