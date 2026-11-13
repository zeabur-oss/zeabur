---
default: patch
---

# The installer retries a failed download

`zeabur-installer apply` tries a download again up to 3 times after a 408, 429, 502, 503 or 504 answer, a connection error or a timeout, waiting 1, 2 and 4 seconds or the `Retry-After` the server gives (seconds or a date) up to 30 seconds, and stops at once when it asks for longer. Other answers, including 500, and a SHA-256 mismatch still fail at once.
