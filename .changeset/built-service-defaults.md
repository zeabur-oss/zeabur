---
default: patch
---

# Git and folder services start with port 8080, and a public domain when the cluster has a domain suffix

The add drawers name the service after the repository or folder (or `web` before one is chosen) and, when the cluster has a domain suffix, fill in the public domain from the start. With Railpack the port defaults to 8080, which Zeabur passes as `PORT`, so a website gets a URL without extra steps. With Dockerfile the service starts with no network until a port is filled in. A folder with a Dockerfile switches the build to Dockerfile.
