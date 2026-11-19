---
default: major
---

# The server judges the cluster's health

The API now says what the cluster page shows, so every client reads the same verdicts. `GET /cluster/nodes` gives each node its issues, groups them for the attention list, sorts the nodes worst first and names the worst one, with the thresholds the bars use. `GET /cluster/upgrades` replaces `severity` with `badge` and `attention`, and a newer Zeabur release carries its `health`. `GET /cluster/backups` gives a `verdict`, a project's backups say whether cluster backups cover it, namespace usage says whether memory is near its maximum, and volumes carry their `health`. Clients that read `severity` from the upgrades must read `badge` instead.
