---
default: minor
---

# Edit or turn off a database's backups

A database's **Backups** tab now has **Edit** and **Turn off backups**. Edit changes the destination, schedule, retention or access keys. The form never shows the saved keys, and leaving them empty keeps them. Turning off stops the schedule and keeps the keys secret and the backups already in the bucket.

Dragonfly no longer offers backups. Its snapshot folder is where it keeps its data and loads it from on start, not a separate copy, so a Dragonfly database keeps its data on its volume and has no **Backups** tab.
