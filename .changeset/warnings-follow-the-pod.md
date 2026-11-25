---
default: patch
---

# Warnings go away with their cause

A crash loop is no longer reported once the container has run ready for a minute, and replicas that a release is replacing no longer raise warnings. The Overview leaves out warnings about pods that are gone, ready, or whose init container has since completed. A review that raises the replicas of a service no longer says the service has only one replica.
