---
default: patch
---

# Doctor says how to delete a Longhorn node that Kubernetes removed

After you remove a worker, Longhorn keeps a node entry for it. `zeabur-installer doctor` used to say the node joined without Longhorn. It now says Longhorn still lists the node and gives the command to delete it. Add and maintain nodes → Remove a node has the full steps.
