---
default: minor
---

# Helm installs join the experience program by default

The chart now sets `telemetry.enabled: true` and makes the installation ID itself: a random UUID, kept in the ConfigMap `zeabur-installation-id` so upgrades keep it. Set `telemetry.installationId` to use your own, which you need when a tool such as Argo CD renders the chart with `helm template`. A release that never set `telemetry.enabled` joins on its next upgrade, and the upgrade's notes say so. To stay out, set `telemetry.enabled=false` or `DO_NOT_TRACK=1`. What is sent and how to turn it off: https://oss.zeabur.com/docs/zeabur/reference/telemetry/
