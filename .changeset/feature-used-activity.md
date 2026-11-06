---
default: patch
---

# Experience program also counts log, terminal and invitation use

With the experience program on, Zeabur's server also sends `feature_used` with `logs_viewed` when a log stream opens, `terminal_opened` when a terminal session is ready and `member_invited` when an invitation succeeds, at most once a day for each.
