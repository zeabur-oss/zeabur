---
default: patch
---

# The status light shows a StatefulSet or DaemonSet update until it finishes

After an image change on a StatefulSet or a DaemonSet, the status light said Running while the old pods still served, and the rollout panel said the update was still under way. Both now use the same kstatus rules, so both show the update until every pod runs the new template. A Deployment whose old ReplicaSet still has pods also shows as rolling out. An `OnDelete` StatefulSet still waits for its pods to be deleted, and the rollout panel now shows it as done once every pod was recreated.
