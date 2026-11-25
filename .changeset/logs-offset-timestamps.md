---
default: patch
---

# Logs tab hides time zone offset timestamps

The Logs tab now removes timestamps that carry a time zone offset, such as `+08:00`, from the line text. It shows them in your time zone when you turn on Timestamps, and the histogram counts these lines by their time.
