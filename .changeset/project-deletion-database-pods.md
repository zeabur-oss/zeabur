---
default: patch
---

# Project deletion with a running database

Deleting a project with a running CloudNativePG database no longer leaves the project `Terminating` for up to 5 minutes. The database is deleted together with its pods before the project's namespace.
