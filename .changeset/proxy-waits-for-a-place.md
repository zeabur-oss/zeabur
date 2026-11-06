---
default: patch
---

# A service's usage history no longer fails at random behind Zeabur Proxy

The usage history asks for CPU, memory and restarts at once, but Zeabur Proxy let each person run only two queries at a time and refused the third straight away, with `too many queries in flight`. When the refused one was CPU or memory, the page said the metrics store didn't answer. A query now waits up to 10 seconds for a place. The limits stay the same.
