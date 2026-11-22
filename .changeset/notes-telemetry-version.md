---
default: patch
---

# The telemetry command after install keeps the installed version

The `helm upgrade` command that the chart prints to join the experience program now names the installed chart version. Before, it upgraded to the latest stable release, so on a pre-release it went back to an older version.
