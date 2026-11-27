---
default: minor
---

# A build's commit status on GitHub opens the service's Builds tab

When Zeabur creates a service's Build, or gives a running service a Build, it sets `builder.zeabur.com/details-url` on the Build to the service's Builds tab under `--public-url`. From Zeabur Builder v0.3.1 (zeabur-oss/builder#273), the commit status of each build links to it, so **Details** on GitHub opens the service's builds in Zeabur. Zeabur Builder v0.3.0 ignores the annotation. A Build made before this change gets the link the next time its source is set.
