---
default: minor
---

# Dragonfly has no backups

A Dragonfly database no longer has a **Backups** tab. Dragonfly keeps its data in snapshots on its volume and loads the newest one when it starts. A snapshot folder in a bucket is where the store keeps its data, not a separate copy, so Zeabur no longer sets one up or restores a new database from it.
