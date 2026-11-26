---
default: patch
---

# Services update to new image versions on their own, when you turn it on

A service that runs an image from a registry, such as PostgreSQL or Redis, has an **Automatic updates** section in Settings. Pick which new versions it takes (patch, minor, all versions, or a new image under the same tag) and when Zeabur Keel looks for them: every hour, or in a weekly window. Zeabur offers only the choices the image's tag allows: version choices need a tag like `7.2.4`.
