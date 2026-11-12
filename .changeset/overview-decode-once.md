---
default: patch
---

# The project overview decodes each object once

Reading a project overview allocates about 22% fewer bytes and 41% fewer times, and runs faster, because each list is decoded straight into its type. Secrets read through the generic path are decoded without their values.
