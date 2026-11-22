---
default: patch
---

# Zeabur reads every Zeabur Catalog release on `/v1/`

Zeabur no longer refuses a Zeabur Catalog release of a newer minor version, such as 0.6.0, with "needs a newer Zeabur". Every release on `/v1/` only adds to the API, so Zeabur reads any of them and leaves out the fields it does not know.
