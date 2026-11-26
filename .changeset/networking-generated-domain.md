---
default: patch
---

# Add domain on the Networking tab starts with the generated address

When the cluster has an application domain, **Add domain** on a service's **Networking** tab now starts with the same generated address that the add-service forms use, such as `web-shop.apps.example.com`. The address is selected, so typing replaces it with your own domain. The field stays empty when the cluster has no application domain or the service already has that address.
