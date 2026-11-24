---
default: patch
---

# `zeabur components install --timeout` takes more forms

`--timeout` still takes `90s`, `10m` and `1h`. It now also takes `1h30m`, `1.5h` and `2 hours`. A number too large for a time limit now fails with an error where it used to stop at the largest value.
