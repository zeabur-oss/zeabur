---
default: patch
---

# The project list leaves out projects you cannot open

A member's project list, Home, the project switcher and the command palette leave out the projects whose contents the member may not read, and the project list ends with how many there are. A project that could not be read for another reason, such as a timeout, still shows, marked as not read. The namespaces that are not projects yet, with Adopt, show only to people who may create projects. The project list in the API gives each project `readable`, which is `false` when kube-apiserver refuses to list its workloads.
