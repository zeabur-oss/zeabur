---
default: minor
---

# Zeabur and `zeaburctl` tell people when a newer version is out

The cluster page shows Zeabur's version. Once a day, the browser checks GitHub's releases. When a newer one is out, the page says so and shows how to upgrade for the way Zeabur was installed. It also says when the running version has known security problems. `zeaburctl` prints one line when a newer version is out, at most once a day. Turn the update check off with the installer's review page checkbox or `updateCheck: false`, the chart's `updateCheck.enabled=false`, `DO_NOT_TRACK=1` or `NO_UPDATE_NOTIFIER`. See [Update check](docs/reference/telemetry.mdx#update-check). `GET /api/v1/cluster/zeabur` returns Zeabur's version, whether the update check is on, and how Zeabur was installed.
