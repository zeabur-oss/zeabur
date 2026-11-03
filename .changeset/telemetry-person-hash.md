---
default: patch
---

# Daily users are no longer counted again after Zeabur restarts

`zeabur_active` carries `person`, a hash of the user name and the day under a key derived from the plan key, so someone who uses Zeabur before and after a restart is counted once that day. The user name can't be worked out from it, and the hash changes every day.
