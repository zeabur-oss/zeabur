---
default: minor
---

# Plans return what kube-apiserver warns about their dry runs

A project plan's objects and a component plan's writes carry `warnings`: the text of the `Warning` headers kube-apiserver sent for the plan's dry run of the write, or of a Pod the write starts. They include Pod Security admission, deprecated APIs and `ValidatingAdmissionPolicy` warnings. A component plan dry-runs a change to the labels of a Namespace that exists, so Pod Security admission names the Pods in it that break a new `enforce` level.
