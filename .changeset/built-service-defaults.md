---
default: patch
---

# Git and folder services start with port 8080 and a public domain

The add drawers name the service after the repository or folder (or `web` before one is chosen) and fill in the public domain from the start. With Railpack the port defaults to 8080, which Zeabur passes as `PORT`, so a website gets a URL without extra steps. With Dockerfile the port comes from its `EXPOSE`, read from an uploaded folder; with none, the service has no network.
