---
default: patch
---

# The experience program sends one event for each save

`app_change_submitted` is now sent once for every save, whatever it changes, with the answer you got. Before, only a save of exactly one change to one Deployment counted, so most saves sent nothing. `rollout_finished`, `change_id` and `operation` are gone, and Zeabur OSS no longer reads the Deployment with your credential after a save.
