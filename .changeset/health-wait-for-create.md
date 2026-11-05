---
default: patch
---

# A health check waits for the objects it checks to appear

`apply` and `doctor` no longer fail at once with "no matching resources found" when a check covers objects a controller creates after install, such as Longhorn's nodes or Traefik's pods. They wait for at least one to exist, then for its condition, both within the check's timeout.
