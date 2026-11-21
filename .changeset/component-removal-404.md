---
default: patch
---

# Removing a component no longer fails when an object is already gone

Removing a catalog component could fail with `404` when helm-controller finished deleting a HelmChart while Zeabur was deleting it again. A removal now leaves alone an object that is already being deleted, and a delete that finds the object gone counts as done.
