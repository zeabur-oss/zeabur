---
default: patch
---

# A CrowdSec agent comes back after its node restarts

After a restart the agent registers its Pod name again and the LAPI refuses it as already registered, so the agent stayed down. `doctor` now reports such an agent, and `apply` deletes its machine in the LAPI so the agent registers on its next retry. This stays until the CrowdSec chart fixes the registration itself.
