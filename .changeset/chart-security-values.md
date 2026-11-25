---
default: patch
---

# The chart sets pod security, ServiceAccount annotations and priority class by value

`podSecurityContext` and `securityContext` set the security contexts of Zeabur, Zeabur Proxy and the install-time Job, `serviceAccount.annotations` annotates Zeabur's ServiceAccount (for workload identity on a managed cluster), and `priorityClassName` sets the pods' priority class. The defaults render the same objects as before.
