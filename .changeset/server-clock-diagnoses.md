---
default: patch
---

# Diagnoses and build times follow the server's clock

Project, database, object storage and build reads, and the cluster events' `sinceSeconds`, count time from the server's clock instead of the machine's. On a real cluster nothing changes; a fixed clock (tests, the demo's screenshots) now sees recent events as recent.
