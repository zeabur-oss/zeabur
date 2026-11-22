---
default: patch
---

# Deleting a template service takes what its template made

Deleting a service that a template made also deletes the domain, certificate and volume the template made for it, unless another service still uses them.
