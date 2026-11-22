---
default: patch
---

# The env editor opens on the main container

A service that has init containers lists its regular containers first, so the environment editor opens on the container that runs the app.
