---
default: minor
---

# A build's commit status on GitHub opens the service's Builds tab

When Zeabur creates a service's Build, or gives a running service a Build, it sets `builder.zeabur.com/details-url` on the Build to the service's Builds tab under `--public-url`. Zeabur Builder links the commit status of each build to it, so **Details** on GitHub opens the build in Zeabur. A Build made before this change gets the link the next time its source is set.
