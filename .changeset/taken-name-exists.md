---
default: patch
---

# A name that is already taken says the object exists

Creating a service over an object left behind, such as a `<name>-env` Secret that kubectl did not delete, no longer fails with "changed after the plan was made". The review says the object already exists in the project, so you can delete it or pick another name.
