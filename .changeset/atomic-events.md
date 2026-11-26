---
default: patch
---

# The experience program sends `signed_in` at each sign-in

`signed_in` is sent each time someone signs in, in place of `zeabur_active`, and PostHog counts daily people from it. `feature_used` is still sent once a day for each feature and set of values, and names nobody.
