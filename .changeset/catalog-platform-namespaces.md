---
default: patch
---

# Catalog components get the platform label, so isolated projects let operators in

Every namespace the catalog creates for a component is now labelled `zeabur.com/platform: "true"`, in each install form. Isolated projects admit these namespaces, so operators such as CloudNativePG can reach the databases they run in a project, and new projects are isolated. A component installed before this release gets the label the next time Zeabur updates it. To label it now, run `kubectl label namespace <namespace> zeabur.com/platform=true`.
