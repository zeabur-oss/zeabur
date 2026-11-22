---
default: patch
---

# The desktop app uses url 2.5.9

The desktop app now builds with `url` 2.5.9, which fixes RUSTSEC-2026-0592. In 2.5.8, `Url::origin()` on a URL with many nested `blob:` prefixes overflowed the stack.
