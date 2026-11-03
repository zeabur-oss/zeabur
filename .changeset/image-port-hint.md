---
default: minor
---

# Suggest the port an image declares

When Zeabur Builder built a service's image and the image declares ports with `EXPOSE`, the Networking tab of a service with no private address says which ports the image declares and offers **Add a private address on port …** for each TCP port its containers and Services do not use yet. The button plans the same Service as the form; nothing changes until the plan is applied. Builder reports the ports from the version that records them in a Build's `status.exposedPorts`.
