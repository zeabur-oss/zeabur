---
default: patch
---

# Doctor names a Longhorn node that Kubernetes removed

After a worker is removed, Longhorn keeps a node entry for it. `zeabur-installer doctor` said a node had joined without Longhorn. It now says that Longhorn still lists a node Kubernetes no longer has and how to delete it, and the removal steps in the node guide include that step.
