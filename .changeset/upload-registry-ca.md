---
default: patch
---

# Folder uploads trust the registry's own CA

A folder upload now trusts the CA that Zeabur Builder puts in the Build's upload Secret, so it reaches zot without a CA set in the chart. A failed upload also shows its error and a Retry button on the service overview, not only on the Builds tab.
