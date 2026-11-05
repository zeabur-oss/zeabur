---
default: patch
---

# A worker that joined before Longhorn is prepared for it

`plan` and `apply` on a joined node now find Longhorn's manager running there and install the iSCSI initiator and NFS client, start iscsid and load iscsi_tcp, as they do on a node that joined with `longhorn: true`. `doctor` on the server names every node whose Longhorn checks of packages, kernel modules or the NFS client fail, and `doctor` on the node checks them too.
