---
default: patch
---

# Signing in from far away takes one redirect less

A page opened without a session goes straight to Zeabur Identity instead of through `/auth/login` first. The API gzips JSON answers from a kilobyte up.
