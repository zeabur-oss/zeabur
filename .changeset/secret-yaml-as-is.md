---
default: patch
---

# A secret's YAML shows its values, and its keys are edited one at a time

The YAML of a secret is the object as the cluster stores it, with `data` in base64, for a person who may read the secret. In the key editor each value is masked until you press its eye, a copy button copies the decoded value, and a binary value shows its size with a download. Saving sends only the keys you changed. A secret's YAML is planned like any other object's: removing a key from `data` removes it from the secret.
