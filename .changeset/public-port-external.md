---
default: minor
---

# Choose the external port of a public port

**Open a port** in the Networking tab has an **External port** field. A service that listens on 8080 can take connections from the internet on 12345, and other services in the project still connect to it on 8080. The `publicPort` change takes the same port as `external`.
