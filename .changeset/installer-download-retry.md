---
default: patch
---

# The installer retries a failed download

`zeabur-installer apply` tries a download again up to 3 times after a 5xx or 429 answer, a connection error or a timeout, waiting 1, 2 and 4 seconds and following `Retry-After` up to 30 seconds. Other 4xx answers and a SHA-256 mismatch still fail at once.
