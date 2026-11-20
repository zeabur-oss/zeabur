---
default: patch
---

# A Compose variable such as `discovery.type` imports again

Creating an image or built service no longer refuses a runtime variable name that is not a C identifier. Kubernetes judges the name in the plan's dry run, so names such as `discovery.type` from a Compose file plan as before. Build variables keep the stricter rule.
