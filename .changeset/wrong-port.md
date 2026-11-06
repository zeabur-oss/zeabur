---
default: patch
---

# A built app that doesn't listen on its port says so

When a built service's health check is refused on its port a minute after the container started, Overview says the app isn't listening there and how to fix it: read `PORT`, or listen on `0.0.0.0` instead of `127.0.0.1`. When the image declares another port with `EXPOSE`, it offers **Use port …**, which moves the container port, health checks, `PORT` and the port the service's private addresses send traffic to.
