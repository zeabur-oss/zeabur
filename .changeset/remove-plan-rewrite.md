---
default: patch
---

# The remove dialog tells a rewrite of another component from a delete

When a component you remove added settings to another component, Zeabur first installs that other component again without those settings. The remove dialog listed that step like a delete, so it read as if the other component went away too. Each write in the dialog now says "Update" or "Delete", and a rewrite step says the component keeps running without the settings the removed component added. The plan API gives each write an `op`, `apply` or `delete`.
