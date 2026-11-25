---
default: patch
---

# One replica limit for every service

A service runs 0 to 20 replicas wherever you set the count. The service settings page used to accept up to 1000, and the server took any number. The server now refuses a count above 20 and names the `replicas` field, and the settings page shows the limit the server sets.
