---
default: minor
---

# Force delete a project held by finalizers

When a project stays in Deleting because an object's controller is gone, the banner offers **Force delete**. After you type the project name, Zeabur removes the finalizers from the objects Kubernetes is still waiting on, running as you, and never from the namespace itself. The plan lists each object and finalizer and warns that their controllers won't clean up what they made outside the cluster. For a Kubernetes finalizer such as `kubernetes.io/pvc-protection`, it adds a separate warning to deal with the Pod or node that holds it first.
