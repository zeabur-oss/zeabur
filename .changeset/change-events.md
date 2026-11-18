---
default: minor
---

# The experience program reports changes of a Deployment and their rollout

When it is on, Zeabur sends `app_change_submitted` when the cluster answers a new image, a new replica count or new environment variables for one Deployment, and `rollout_finished` when an accepted change becomes available or passes its progress deadline. Zeabur reads the Deployment with the person's own credential to see it.
