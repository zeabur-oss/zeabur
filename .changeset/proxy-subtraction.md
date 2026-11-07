---
default: patch
---

# Zeabur Proxy leaves query concurrency to VictoriaMetrics and VictoriaLogs

Zeabur Proxy refused a third query from one person, or a seventeenth in all, while others were still running, so the usage tab could show "The metrics store did not respond." It now sends every allowed query on, and VictoriaMetrics and VictoriaLogs queue them under their own limits (`-search.maxConcurrentRequests`, `-search.maxQueueDuration`).

The proxy also no longer reads the namespace to cut history at its creation. The project's name is the boundary, so a new project with a deleted project's name shows its history until the storage's retention drops it. "Delete a project" says how to clear it first.

Logs and usage queries are no longer cut to the last week: they reach as far back as the storage keeps. The chart's `proxy.logs.maxHours` and `proxy.metrics.maxHours` are gone. The proxy also drops its own connection cap and header wait, since only Zeabur can reach it.
